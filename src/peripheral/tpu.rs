//! TPU (Tensor Processing Unit) MMIO peripheral.
//!
//! The TPU is modeled as a memory-mapped accelerator specialized for
//! quantized matrix multiplication operations, optimized for neural network
//! inference workloads.

use crate::error::{Result, SimError};
use crate::peripheral::dma;
use crate::traits::{
    Accelerator, AcceleratorPerfCounters, AcceleratorType, KernelType, Memory, Peripheral,
    Precision,
};
use crate::types::Addr;
use std::any::Any;
use std::collections::VecDeque;

/// TPU base address.
pub const TPU_BASE: u32 = 0x2002_0000;
/// TPU MMIO region size (4KB).
pub const TPU_SIZE: usize = 0x1000;

// Register offsets
mod regs {
    pub const CONTROL: u32 = 0x00;
    pub const STATUS: u32 = 0x04;
    pub const KERNEL_TYPE: u32 = 0x08;

    // Matrix A configuration
    pub const MATRIX_A_ADDR_LOW: u32 = 0x10;
    pub const MATRIX_A_ADDR_HIGH: u32 = 0x14;
    pub const MATRIX_A_ROWS: u32 = 0x18; // M
    pub const MATRIX_A_COLS: u32 = 0x1C; // K

    // Matrix B configuration
    pub const MATRIX_B_ADDR_LOW: u32 = 0x20;
    pub const MATRIX_B_ADDR_HIGH: u32 = 0x24;
    pub const MATRIX_B_ROWS: u32 = 0x28; // K
    pub const MATRIX_B_COLS: u32 = 0x2C; // N

    // Matrix C (result) configuration
    pub const MATRIX_C_ADDR_LOW: u32 = 0x30;
    pub const MATRIX_C_ADDR_HIGH: u32 = 0x34;

    // Quantization parameters
    pub const INPUT_SCALE: u32 = 0x40;
    pub const INPUT_ZERO_POINT: u32 = 0x44;
    pub const OUTPUT_SCALE: u32 = 0x48;
    pub const OUTPUT_ZERO_POINT: u32 = 0x4C;

    // Batch processing
    pub const BATCH_SIZE: u32 = 0x50;
    pub const BATCH_STRIDE_A: u32 = 0x54;
    pub const BATCH_STRIDE_B: u32 = 0x58;
    pub const BATCH_STRIDE_C: u32 = 0x5C;

    // Performance counters
    pub const MATRICES_COMPUTED_LOW: u32 = 0x60;
    pub const MATRICES_COMPUTED_HIGH: u32 = 0x64;
    pub const CYCLES_LOW: u32 = 0x68;
    pub const CYCLES_HIGH: u32 = 0x6C;
    pub const OPS_COUNT_LOW: u32 = 0x70;
    pub const OPS_COUNT_HIGH: u32 = 0x74;

    // Command queue
    pub const CMD_QUEUE_BASE_LOW: u32 = 0x80;
    pub const CMD_QUEUE_BASE_HIGH: u32 = 0x84;
    pub const CMD_QUEUE_LEN: u32 = 0x88;
    pub const CMD_QUEUE_NOTIFY: u32 = 0x8C;

    // Error status
    pub const ERROR_CODE: u32 = 0x90;
    pub const TASKS_DONE: u32 = 0x94;
    pub const TASKS_ERROR: u32 = 0x98;
}

// Control register bits
mod control_bits {
    pub const START: u32 = 1 << 0;
    pub const RESET: u32 = 1 << 1;
    pub const IRQ_EN: u32 = 1 << 2;
    pub const BATCH_MODE: u32 = 1 << 3;
    pub const CMD_QUEUE_MODE: u32 = 1 << 4;
}

// Status register bits
mod status_bits {
    pub const IDLE: u32 = 0;
    pub const BUSY: u32 = 1 << 0;
    pub const DONE: u32 = 1 << 1;
    pub const ERROR: u32 = 1 << 2;
    pub const IRQ_PENDING: u32 = 1 << 3;
}

/// TPU snapshot for visualization and debugging.
#[derive(Debug, Clone)]
pub struct TpuSnapshot {
    pub control: u32,
    pub status: u32,
    pub kernel_type: u32,
    pub matrix_a_addr: u64,
    pub matrix_b_addr: u64,
    pub matrix_c_addr: u64,
    pub m: u32,
    pub n: u32,
    pub k: u32,
    pub input_scale: u32,
    pub input_zero_point: i32,
    pub output_scale: u32,
    pub output_zero_point: i32,
    pub batch_size: u32,
    pub matrices_computed: u64,
    pub cycles: u64,
    pub ops_count: u64,
    pub error_code: u32,
    pub tasks_done: u32,
    pub tasks_error: u32,
    pub work_queue_len: usize,
}

/// Internal work item for async matrix multiplication.
#[derive(Debug, Clone)]
struct TpuWork {
    matrix_a_addr: u64,
    matrix_b_addr: u64,
    matrix_c_addr: u64,
    m: usize,
    n: usize,
    k: usize,
    input_scale: f32,
    input_zero_point: i32,
    output_scale: f32,
    output_zero_point: i32,
}

/// TPU peripheral implementation.
#[derive(Debug, Clone)]
pub struct Tpu {
    base: Addr,
    control: u32,
    status: u32,
    kernel_type: u32,

    // Matrix A configuration
    matrix_a_addr: u64,
    matrix_a_rows: u32, // M
    matrix_a_cols: u32, // K

    // Matrix B configuration
    matrix_b_addr: u64,
    matrix_b_rows: u32, // K
    matrix_b_cols: u32, // N

    // Matrix C configuration
    matrix_c_addr: u64,

    // Quantization parameters (stored as IEEE 754 float bits)
    input_scale: u32,
    input_zero_point: i32,
    output_scale: u32,
    output_zero_point: i32,

    // Batch processing
    batch_size: u32,
    batch_stride_a: u32,
    batch_stride_b: u32,
    batch_stride_c: u32,

    // Performance counters
    matrices_computed: u64,
    cycles: u64,
    ops_count: u64,

    // Command queue
    cmd_queue_addr: u64,
    cmd_queue_len: u32,
    pending_notify: bool,

    // Error tracking
    error_code: u32,
    tasks_done: u32,
    tasks_error: u32,

    // Async work queue
    work_queue: VecDeque<TpuWork>,
    soft_async_budget: Option<usize>,

    // Pending start flag (set by write to CONTROL.START, consumed by bus)
    pending_start: bool,
}

impl Default for Tpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Tpu {
    /// Create a new TPU peripheral.
    pub fn new() -> Self {
        Self::with_base(Addr::new(TPU_BASE))
    }

    /// Create a new TPU with a custom base address.
    pub fn with_base(base: Addr) -> Self {
        Self {
            base,
            control: 0,
            status: status_bits::IDLE,
            kernel_type: KernelType::MatMul as u32,
            matrix_a_addr: 0,
            matrix_a_rows: 0,
            matrix_a_cols: 0,
            matrix_b_addr: 0,
            matrix_b_rows: 0,
            matrix_b_cols: 0,
            matrix_c_addr: 0,
            input_scale: 0x3F800000, // 1.0f in IEEE 754
            input_zero_point: 0,
            output_scale: 0x3F800000,
            output_zero_point: 0,
            batch_size: 1,
            batch_stride_a: 0,
            batch_stride_b: 0,
            batch_stride_c: 0,
            matrices_computed: 0,
            cycles: 0,
            ops_count: 0,
            cmd_queue_addr: 0,
            cmd_queue_len: 0,
            pending_notify: false,
            error_code: 0,
            tasks_done: 0,
            tasks_error: 0,
            work_queue: VecDeque::new(),
            soft_async_budget: None,
            pending_start: false,
        }
    }

    /// Get a snapshot of the TPU state.
    pub fn snapshot(&self) -> TpuSnapshot {
        TpuSnapshot {
            control: self.control,
            status: self.status,
            kernel_type: self.kernel_type,
            matrix_a_addr: self.matrix_a_addr,
            matrix_b_addr: self.matrix_b_addr,
            matrix_c_addr: self.matrix_c_addr,
            m: self.matrix_a_rows,
            n: self.matrix_b_cols,
            k: self.matrix_a_cols,
            input_scale: self.input_scale,
            input_zero_point: self.input_zero_point,
            output_scale: self.output_scale,
            output_zero_point: self.output_zero_point,
            batch_size: self.batch_size,
            matrices_computed: self.matrices_computed,
            cycles: self.cycles,
            ops_count: self.ops_count,
            error_code: self.error_code,
            tasks_done: self.tasks_done,
            tasks_error: self.tasks_error,
            work_queue_len: self.work_queue.len(),
        }
    }

    /// Configure soft-asynchronous mode.
    pub fn set_soft_async_budget(&mut self, budget: Option<usize>) {
        self.soft_async_budget = budget;
    }

    /// Check if there's a pending descriptor notification.
    pub fn has_pending_descriptor_notify(&self) -> bool {
        self.pending_notify
    }

    /// Check if START bit was written (consumed by bus to trigger compute).
    pub fn has_pending_start(&self) -> bool {
        self.pending_start
    }

    /// Execute current kernel with memory access (called by bus).
    pub fn execute_with_memory(&mut self, ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>) {
        self.pending_start = false;
        self.execute_once_with_memory(ram_regions);
    }

    fn read_reg(&self, reg: u32) -> u32 {
        match reg {
            regs::CONTROL => self.control,
            regs::STATUS => self.status,
            regs::KERNEL_TYPE => self.kernel_type,
            regs::MATRIX_A_ADDR_LOW => self.matrix_a_addr as u32,
            regs::MATRIX_A_ADDR_HIGH => (self.matrix_a_addr >> 32) as u32,
            regs::MATRIX_A_ROWS => self.matrix_a_rows,
            regs::MATRIX_A_COLS => self.matrix_a_cols,
            regs::MATRIX_B_ADDR_LOW => self.matrix_b_addr as u32,
            regs::MATRIX_B_ADDR_HIGH => (self.matrix_b_addr >> 32) as u32,
            regs::MATRIX_B_ROWS => self.matrix_b_rows,
            regs::MATRIX_B_COLS => self.matrix_b_cols,
            regs::MATRIX_C_ADDR_LOW => self.matrix_c_addr as u32,
            regs::MATRIX_C_ADDR_HIGH => (self.matrix_c_addr >> 32) as u32,
            regs::INPUT_SCALE => self.input_scale,
            regs::INPUT_ZERO_POINT => self.input_zero_point as u32,
            regs::OUTPUT_SCALE => self.output_scale,
            regs::OUTPUT_ZERO_POINT => self.output_zero_point as u32,
            regs::BATCH_SIZE => self.batch_size,
            regs::BATCH_STRIDE_A => self.batch_stride_a,
            regs::BATCH_STRIDE_B => self.batch_stride_b,
            regs::BATCH_STRIDE_C => self.batch_stride_c,
            regs::MATRICES_COMPUTED_LOW => self.matrices_computed as u32,
            regs::MATRICES_COMPUTED_HIGH => (self.matrices_computed >> 32) as u32,
            regs::CYCLES_LOW => self.cycles as u32,
            regs::CYCLES_HIGH => (self.cycles >> 32) as u32,
            regs::OPS_COUNT_LOW => self.ops_count as u32,
            regs::OPS_COUNT_HIGH => (self.ops_count >> 32) as u32,
            regs::CMD_QUEUE_BASE_LOW => self.cmd_queue_addr as u32,
            regs::CMD_QUEUE_BASE_HIGH => (self.cmd_queue_addr >> 32) as u32,
            regs::CMD_QUEUE_LEN => self.cmd_queue_len,
            regs::ERROR_CODE => self.error_code,
            regs::TASKS_DONE => self.tasks_done,
            regs::TASKS_ERROR => self.tasks_error,
            _ => 0,
        }
    }

    fn write_reg(&mut self, reg: u32, value: u32) {
        match reg {
            regs::CONTROL => {
                self.control = value;
                if (self.control & control_bits::START) != 0 {
                    self.pending_start = true;
                    self.control &= !control_bits::START;
                }
                if (self.control & control_bits::RESET) != 0 {
                    self.reset_state();
                    self.control &= !control_bits::RESET;
                }
            }
            regs::STATUS => {
                // W1C for DONE/ERROR/IRQ_PENDING
                if (value & status_bits::DONE) != 0 {
                    self.status &= !status_bits::DONE;
                }
                if (value & status_bits::ERROR) != 0 {
                    self.status &= !status_bits::ERROR;
                    self.error_code = 0;
                }
                if (value & status_bits::IRQ_PENDING) != 0 {
                    self.status &= !status_bits::IRQ_PENDING;
                }
            }
            regs::KERNEL_TYPE => self.kernel_type = value,
            regs::MATRIX_A_ADDR_LOW => {
                self.matrix_a_addr = (self.matrix_a_addr & 0xFFFF_FFFF_0000_0000) | value as u64;
            }
            regs::MATRIX_A_ADDR_HIGH => {
                self.matrix_a_addr =
                    (self.matrix_a_addr & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
            }
            regs::MATRIX_A_ROWS => self.matrix_a_rows = value,
            regs::MATRIX_A_COLS => self.matrix_a_cols = value,
            regs::MATRIX_B_ADDR_LOW => {
                self.matrix_b_addr = (self.matrix_b_addr & 0xFFFF_FFFF_0000_0000) | value as u64;
            }
            regs::MATRIX_B_ADDR_HIGH => {
                self.matrix_b_addr =
                    (self.matrix_b_addr & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
            }
            regs::MATRIX_B_ROWS => self.matrix_b_rows = value,
            regs::MATRIX_B_COLS => self.matrix_b_cols = value,
            regs::MATRIX_C_ADDR_LOW => {
                self.matrix_c_addr = (self.matrix_c_addr & 0xFFFF_FFFF_0000_0000) | value as u64;
            }
            regs::MATRIX_C_ADDR_HIGH => {
                self.matrix_c_addr =
                    (self.matrix_c_addr & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
            }
            regs::INPUT_SCALE => self.input_scale = value,
            regs::INPUT_ZERO_POINT => self.input_zero_point = value as i32,
            regs::OUTPUT_SCALE => self.output_scale = value,
            regs::OUTPUT_ZERO_POINT => self.output_zero_point = value as i32,
            regs::BATCH_SIZE => self.batch_size = value.max(1),
            regs::BATCH_STRIDE_A => self.batch_stride_a = value,
            regs::BATCH_STRIDE_B => self.batch_stride_b = value,
            regs::BATCH_STRIDE_C => self.batch_stride_c = value,
            regs::CMD_QUEUE_BASE_LOW => {
                self.cmd_queue_addr = (self.cmd_queue_addr & 0xFFFF_FFFF_0000_0000) | value as u64;
            }
            regs::CMD_QUEUE_BASE_HIGH => {
                self.cmd_queue_addr =
                    (self.cmd_queue_addr & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
            }
            regs::CMD_QUEUE_LEN => self.cmd_queue_len = value,
            regs::CMD_QUEUE_NOTIFY => {
                if value != 0 {
                    self.pending_notify = true;
                }
            }
            _ => {}
        }
    }

    fn read_u8(&self, offset: u32) -> u8 {
        let reg = offset & !0x3;
        let shift = (offset & 0x3) * 8;
        ((self.read_reg(reg) >> shift) & 0xFF) as u8
    }

    fn write_u8(&mut self, offset: u32, value: u8) {
        let reg = offset & !0x3;
        let shift = (offset & 0x3) * 8;
        let mut current = self.read_reg(reg);
        current &= !(0xFF << shift);
        current |= (value as u32) << shift;
        self.write_reg(reg, current);
    }

    fn reset_state(&mut self) {
        self.status = status_bits::IDLE;
        self.error_code = 0;
        self.work_queue.clear();
    }

    // ── DMA helpers (delegated to shared module) ─────────────────────────

    fn read_guest_u8(ram: &mut dma::RamRegions, addr: u64) -> Result<u8> {
        dma::read_u8(ram, addr)
    }
    fn write_guest_u8(ram: &mut dma::RamRegions, addr: u64, value: u8) -> Result<()> {
        dma::write_u8(ram, addr, value)
    }
    fn read_guest_u32(ram: &mut dma::RamRegions, addr: u64) -> Result<u32> {
        dma::read_u32(ram, addr)
    }
    fn write_guest_u32(ram: &mut dma::RamRegions, addr: u64, value: u32) -> Result<()> {
        dma::write_u32(ram, addr, value)
    }

    // ── Compute kernels ────────────────────────────────────────────────

    /// INT8 quantized matmul with dequantization.
    ///
    /// A: [M x K] i8, B: [K x N] i8 → C: [M x N] i32 (raw accumulators)
    /// Then optionally dequantize: C_f = (C_i32 - zp_adjust) * scale
    fn kernel_matmul_int8(
        a: &[i8],
        b: &[i8],
        m: usize,
        n: usize,
        k: usize,
        input_scale: f32,
        input_zero_point: i32,
        output_scale: f32,
        output_zero_point: i32,
    ) -> Vec<i32> {
        let mut c = vec![0i32; m * n];
        let inv_out_scale = if output_scale != 0.0 {
            1.0 / output_scale
        } else {
            1.0
        };

        for i in 0..m {
            for j in 0..n {
                let mut acc: i32 = 0;
                for p in 0..k {
                    let a_val = a[i * k + p] as i32 - input_zero_point;
                    let b_val = b[p * n + j] as i32 - input_zero_point;
                    acc += a_val * b_val;
                }
                // Dequantize: acc * input_scale^2 / output_scale + output_zero_point
                let deq = (acc as f32 * input_scale * input_scale * inv_out_scale
                    + output_zero_point as f32)
                    .round();
                c[i * n + j] = deq as i32;
            }
        }
        c
    }

    /// Re-quantize an i32 accumulator to i8.
    fn kernel_requantize(data: &[i32], scale: f32, zero_point: i32) -> Vec<i8> {
        data.iter()
            .map(|&v| {
                let f = v as f32 * scale + zero_point as f32;
                f.round().clamp(-128.0, 127.0) as i8
            })
            .collect()
    }

    /// Quantize f32 → i8.
    fn kernel_quantize(data: &[f32], scale: f32, zero_point: i32) -> Vec<i8> {
        data.iter()
            .map(|&v| ((v / scale).round() + zero_point as f32).clamp(-128.0, 127.0) as i8)
            .collect()
    }

    /// Dequantize i8 → f32.
    fn kernel_dequantize(data: &[i8], scale: f32, zero_point: i32) -> Vec<f32> {
        data.iter()
            .map(|&v| (v as f32 - zero_point as f32) * scale)
            .collect()
    }

    // ── Execute path ───────────────────────────────────────────────────

    fn finish_op(&mut self, is_error: bool) {
        self.status &= !status_bits::BUSY;
        self.status |= status_bits::DONE;
        if is_error {
            self.status |= status_bits::ERROR;
        }
        if (self.control & control_bits::IRQ_EN) != 0 {
            self.status |= status_bits::IRQ_PENDING;
        }
    }

    fn execute_once_with_memory(&mut self, ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>) {
        self.status |= status_bits::BUSY;
        self.status &= !(status_bits::DONE | status_bits::ERROR);

        let kernel = KernelType::from_u32(self.kernel_type);
        let result = match kernel {
            KernelType::MatMul => self.execute_matmul_int8(ram_regions),
            KernelType::Quantize => self.execute_quantize(ram_regions),
            KernelType::Dequantize => self.execute_dequantize(ram_regions),
            KernelType::Requantize => self.execute_requantize(ram_regions),
            _ => {
                self.error_code = 1;
                self.tasks_error = self.tasks_error.wrapping_add(1);
                self.finish_op(true);
                return;
            }
        };

        match result {
            Ok(ops) => {
                self.ops_count = self.ops_count.wrapping_add(ops);
                self.matrices_computed = self.matrices_computed.wrapping_add(1);
                self.tasks_done = self.tasks_done.wrapping_add(1);
                self.finish_op(false);
            }
            Err(_) => {
                self.error_code = 2;
                self.tasks_error = self.tasks_error.wrapping_add(1);
                self.finish_op(true);
            }
        }
    }

    /// INT8 quantized MatMul: A[i8] * B[i8] → C[i32] with dequantization.
    fn execute_matmul_int8(
        &mut self,
        ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
    ) -> Result<u64> {
        let m = self.matrix_a_rows as usize;
        let k = self.matrix_a_cols as usize;
        let n = self.matrix_b_cols as usize;
        if m == 0 || k == 0 || n == 0 {
            return Ok(0);
        }

        let input_scale = f32::from_bits(self.input_scale);
        let output_scale = f32::from_bits(self.output_scale);

        // Read i8 matrices from guest memory
        let a = dma::read_i8_slice(ram_regions, self.matrix_a_addr, m * k)?;
        let b = dma::read_i8_slice(ram_regions, self.matrix_b_addr, k * n)?;

        let c = Self::kernel_matmul_int8(
            &a,
            &b,
            m,
            n,
            k,
            input_scale,
            self.input_zero_point,
            output_scale,
            self.output_zero_point,
        );

        // Write i32 results
        for (i, &v) in c.iter().enumerate() {
            Self::write_guest_u32(ram_regions, self.matrix_c_addr + (i as u64) * 4, v as u32)?;
        }

        let ops = (m * n * k * 2) as u64;
        self.cycles = self.cycles.wrapping_add(((m * n * k) / 4).max(1) as u64);
        Ok(ops)
    }

    /// Quantize f32 → i8
    fn execute_quantize(
        &mut self,
        ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
    ) -> Result<u64> {
        let m = self.matrix_a_rows as usize;
        let k = self.matrix_a_cols as usize;
        let count = if m > 0 && k > 0 { m * k } else { 0 };
        if count == 0 {
            return Ok(0);
        }

        let scale = f32::from_bits(self.input_scale);
        // Read f32 from matrix A, write i8 to matrix C
        let mut data = Vec::with_capacity(count);
        for i in 0..count {
            let bits = Self::read_guest_u32(ram_regions, self.matrix_a_addr + (i as u64) * 4)?;
            data.push(f32::from_bits(bits));
        }
        let quantized = Self::kernel_quantize(&data, scale, self.input_zero_point);
        for (i, &v) in quantized.iter().enumerate() {
            Self::write_guest_u8(ram_regions, self.matrix_c_addr + i as u64, v as u8)?;
        }

        self.cycles = self.cycles.wrapping_add(count as u64);
        Ok(count as u64)
    }

    /// Dequantize i8 → f32
    fn execute_dequantize(
        &mut self,
        ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
    ) -> Result<u64> {
        let m = self.matrix_a_rows as usize;
        let k = self.matrix_a_cols as usize;
        let count = if m > 0 && k > 0 { m * k } else { 0 };
        if count == 0 {
            return Ok(0);
        }

        let scale = f32::from_bits(self.output_scale);
        let data = dma::read_i8_slice(ram_regions, self.matrix_a_addr, count)?;
        let dequantized = Self::kernel_dequantize(&data, scale, self.output_zero_point);
        for (i, &v) in dequantized.iter().enumerate() {
            Self::write_guest_u32(
                ram_regions,
                self.matrix_c_addr + (i as u64) * 4,
                v.to_bits(),
            )?;
        }

        self.cycles = self.cycles.wrapping_add(count as u64);
        Ok(count as u64)
    }

    /// Requantize i32 → i8
    fn execute_requantize(
        &mut self,
        ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
    ) -> Result<u64> {
        let m = self.matrix_a_rows as usize;
        let k = self.matrix_a_cols as usize;
        let count = if m > 0 && k > 0 { m * k } else { 0 };
        if count == 0 {
            return Ok(0);
        }

        let scale = f32::from_bits(self.output_scale);
        let mut data = Vec::with_capacity(count);
        for i in 0..count {
            let v = Self::read_guest_u32(ram_regions, self.matrix_a_addr + (i as u64) * 4)?;
            data.push(v as i32);
        }
        let req = Self::kernel_requantize(&data, scale, self.output_zero_point);
        for (i, &v) in req.iter().enumerate() {
            Self::write_guest_u8(ram_regions, self.matrix_c_addr + i as u64, v as u8)?;
        }

        self.cycles = self.cycles.wrapping_add(count as u64);
        Ok(count as u64)
    }

    fn execute_once(&mut self) {
        // No-op without ram_regions; actual compute happens via bus write_byte → process_pending
        let m = self.matrix_a_rows as u64;
        let n = self.matrix_b_cols as u64;
        let k = self.matrix_a_cols as u64;
        let ops = m * n * k * 2;
        self.ops_count = self.ops_count.wrapping_add(ops);
        self.cycles = self.cycles.wrapping_add((ops / 4).max(1));
        self.matrices_computed = self.matrices_computed.wrapping_add(1);
        self.tasks_done = self.tasks_done.wrapping_add(1);
        self.status |= status_bits::DONE;
        if (self.control & control_bits::IRQ_EN) != 0 {
            self.status |= status_bits::IRQ_PENDING;
        }
    }

    /// Process pending descriptor notification.
    ///
    /// Each command descriptor (20 bytes):
    ///   [0..4]   kernel_type (u32)
    ///   [4..8]   matrix_a_addr (u32)
    ///   [8..12]  matrix_b_addr (u32)
    ///   [12..16] matrix_c_addr (u32)
    ///   [16..20] (M<<16 | K) packed u32  — rows of A, cols of A
    ///   [20..24] (N<<16) packed u32      — cols of B
    pub fn process_pending_descriptor_notify(
        &mut self,
        ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
    ) -> Result<()> {
        if !self.pending_notify {
            return Ok(());
        }
        self.pending_notify = false;
        self.status |= status_bits::BUSY;
        self.status &= !status_bits::DONE;

        for idx in 0..self.cmd_queue_len {
            let desc_base = self.cmd_queue_addr + (idx as u64) * 24;
            let kt = Self::read_guest_u32(ram_regions, desc_base)?;
            let a_addr = Self::read_guest_u32(ram_regions, desc_base + 4)? as u64;
            let b_addr = Self::read_guest_u32(ram_regions, desc_base + 8)? as u64;
            let c_addr = Self::read_guest_u32(ram_regions, desc_base + 12)? as u64;
            let mk = Self::read_guest_u32(ram_regions, desc_base + 16)?;
            let n_val = Self::read_guest_u32(ram_regions, desc_base + 20)?;

            self.kernel_type = kt;
            self.matrix_a_addr = a_addr;
            self.matrix_b_addr = b_addr;
            self.matrix_c_addr = c_addr;
            self.matrix_a_rows = mk >> 16;
            self.matrix_a_cols = mk & 0xFFFF;
            self.matrix_b_rows = self.matrix_a_cols;
            self.matrix_b_cols = n_val & 0xFFFF;

            self.execute_once_with_memory(ram_regions);
        }

        if self.cmd_queue_len == 0 {
            self.status &= !status_bits::BUSY;
            self.status |= status_bits::DONE;
            if (self.control & control_bits::IRQ_EN) != 0 {
                self.status |= status_bits::IRQ_PENDING;
            }
        }

        Ok(())
    }
}

impl Peripheral for Tpu {
    fn read(&self, offset: Addr) -> Result<u8> {
        let off = offset.raw();
        if off >= TPU_SIZE as u32 {
            return Err(SimError::InvalidAddress(offset));
        }
        Ok(self.read_u8(off))
    }

    fn write(&mut self, offset: Addr, value: u8) -> Result<()> {
        let off = offset.raw();
        if off >= TPU_SIZE as u32 {
            return Err(SimError::InvalidAddress(offset));
        }
        self.write_u8(off, value);
        Ok(())
    }

    fn base_addr(&self) -> Addr {
        self.base
    }

    fn size(&self) -> usize {
        TPU_SIZE
    }

    fn name(&self) -> &str {
        "TPU"
    }

    fn has_interrupt(&self) -> bool {
        (self.status & status_bits::IRQ_PENDING) != 0
    }

    fn acknowledge_interrupt(&mut self) {
        self.status &= !status_bits::IRQ_PENDING;
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn try_execute_pending(
        &mut self,
        ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
    ) -> Result<bool> {
        let mut did_work = false;
        if self.has_pending_start() {
            self.execute_with_memory(ram_regions);
            did_work = true;
        }
        if self.has_pending_descriptor_notify() {
            self.process_pending_descriptor_notify(ram_regions)?;
            did_work = true;
        }
        Ok(did_work)
    }
}

impl Accelerator for Tpu {
    fn accelerator_type(&self) -> AcceleratorType {
        AcceleratorType::Tpu
    }

    fn supported_precisions(&self) -> Vec<Precision> {
        vec![Precision::Int8, Precision::Fp32]
    }

    fn supported_kernels(&self) -> Vec<KernelType> {
        vec![
            KernelType::MatMul,
            KernelType::Quantize,
            KernelType::Dequantize,
            KernelType::Requantize,
        ]
    }

    fn performance_counters(&self) -> AcceleratorPerfCounters {
        AcceleratorPerfCounters {
            kernels_executed: self.matrices_computed,
            cycles_elapsed: self.cycles,
            bytes_transferred: 0,
            operations_count: self.ops_count,
            errors_count: self.tasks_error as u64,
        }
    }

    fn reset(&mut self) {
        self.control = 0;
        self.status = status_bits::IDLE;
        self.kernel_type = KernelType::MatMul as u32;
        self.matrix_a_addr = 0;
        self.matrix_a_rows = 0;
        self.matrix_a_cols = 0;
        self.matrix_b_addr = 0;
        self.matrix_b_rows = 0;
        self.matrix_b_cols = 0;
        self.matrix_c_addr = 0;
        self.input_scale = 0x3F800000;
        self.input_zero_point = 0;
        self.output_scale = 0x3F800000;
        self.output_zero_point = 0;
        self.batch_size = 1;
        self.batch_stride_a = 0;
        self.batch_stride_b = 0;
        self.batch_stride_c = 0;
        self.matrices_computed = 0;
        self.cycles = 0;
        self.ops_count = 0;
        self.cmd_queue_addr = 0;
        self.cmd_queue_len = 0;
        self.pending_notify = false;
        self.error_code = 0;
        self.tasks_done = 0;
        self.tasks_error = 0;
        self.work_queue.clear();
    }

    fn is_busy(&self) -> bool {
        (self.status & status_bits::BUSY) != 0
    }

    fn current_kernel(&self) -> Option<KernelType> {
        if self.is_busy() {
            Some(KernelType::from_u32(self.kernel_type))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_u32(tpu: &mut Tpu, reg: u32, value: u32) {
        for i in 0..4 {
            tpu.write(Addr::new(reg + i), ((value >> (i * 8)) & 0xFF) as u8)
                .unwrap();
        }
    }

    fn read_u32(tpu: &Tpu, reg: u32) -> u32 {
        let mut value = 0u32;
        for i in 0..4 {
            value |= (tpu.read(Addr::new(reg + i)).unwrap() as u32) << (i * 8);
        }
        value
    }

    #[test]
    fn test_tpu_basic() {
        let tpu = Tpu::new();
        assert_eq!(tpu.name(), "TPU");
        assert_eq!(tpu.size(), TPU_SIZE);
        assert!(!tpu.is_busy());
    }

    #[test]
    fn test_tpu_matrix_config() {
        let mut tpu = Tpu::new();
        write_u32(&mut tpu, regs::MATRIX_A_ROWS, 2);
        write_u32(&mut tpu, regs::MATRIX_A_COLS, 3);
        write_u32(&mut tpu, regs::MATRIX_B_ROWS, 3);
        write_u32(&mut tpu, regs::MATRIX_B_COLS, 4);

        let snapshot = tpu.snapshot();
        assert_eq!(snapshot.m, 2);
        assert_eq!(snapshot.n, 4);
        assert_eq!(snapshot.k, 3);
    }

    #[test]
    fn test_tpu_control_start_triggers_pending() {
        let mut tpu = Tpu::new();
        write_u32(&mut tpu, regs::CONTROL, control_bits::START);
        assert!(tpu.has_pending_start());
    }

    #[test]
    fn test_tpu_execute_with_memory() {
        use crate::memory::Ram;

        const RAM_BASE: u32 = 0x8000_0000;
        let mut ram: Vec<(Addr, usize, Box<dyn Memory>)> =
            vec![(Addr::new(RAM_BASE), 0x4000, Box::new(Ram::new(0x4000)))];

        let mut tpu = Tpu::new();
        write_u32(&mut tpu, regs::MATRIX_A_ROWS, 2);
        write_u32(&mut tpu, regs::MATRIX_A_COLS, 2);
        write_u32(&mut tpu, regs::MATRIX_B_COLS, 2);
        // Scale = 1.0
        write_u32(&mut tpu, regs::INPUT_SCALE, 1.0f32.to_bits());
        write_u32(&mut tpu, regs::OUTPUT_SCALE, 1.0f32.to_bits());
        write_u32(&mut tpu, regs::INPUT_ZERO_POINT, 0);
        write_u32(&mut tpu, regs::OUTPUT_ZERO_POINT, 0);
        write_u32(
            &mut tpu,
            regs::CONTROL,
            control_bits::START | control_bits::IRQ_EN,
        );

        // Write i8 matrices: A=[[1,2],[3,4]], B=[[5,6],[7,8]]
        let a_addr = RAM_BASE + 0x100;
        let b_addr = RAM_BASE + 0x110;
        let c_addr = RAM_BASE + 0x120;
        write_u32(&mut tpu, regs::MATRIX_A_ADDR_LOW, a_addr);
        write_u32(&mut tpu, regs::MATRIX_B_ADDR_LOW, b_addr);
        write_u32(&mut tpu, regs::MATRIX_C_ADDR_LOW, c_addr);

        let a: [i8; 4] = [1, 2, 3, 4];
        let b: [i8; 4] = [5, 6, 7, 8];
        for (i, &v) in a.iter().enumerate() {
            Tpu::write_guest_u8(&mut ram, (a_addr + i as u32) as u64, v as u8).unwrap();
        }
        for (i, &v) in b.iter().enumerate() {
            Tpu::write_guest_u8(&mut ram, (b_addr + i as u32) as u64, v as u8).unwrap();
        }

        tpu.execute_with_memory(&mut ram);
        assert!(!tpu.has_pending_start());

        let status = read_u32(&tpu, regs::STATUS);
        assert_ne!(status & status_bits::DONE, 0);
        assert!(tpu.has_interrupt());
        tpu.acknowledge_interrupt();
        assert!(!tpu.has_interrupt());

        // C = A*B = [[19,22],[43,50]] (with zero_point=0, scale=1)
        let c0 = Tpu::read_guest_u32(&mut ram, c_addr as u64).unwrap() as i32;
        let c1 = Tpu::read_guest_u32(&mut ram, (c_addr + 4) as u64).unwrap() as i32;
        let c2 = Tpu::read_guest_u32(&mut ram, (c_addr + 8) as u64).unwrap() as i32;
        let c3 = Tpu::read_guest_u32(&mut ram, (c_addr + 12) as u64).unwrap() as i32;
        assert_eq!(c0, 19);
        assert_eq!(c1, 22);
        assert_eq!(c2, 43);
        assert_eq!(c3, 50);
    }

    #[test]
    fn test_tpu_quantization_params() {
        let mut tpu = Tpu::new();
        // Set input scale to 0.5 (IEEE 754: 0x3F000000)
        write_u32(&mut tpu, regs::INPUT_SCALE, 0x3F000000);
        write_u32(&mut tpu, regs::INPUT_ZERO_POINT, 128);

        let snapshot = tpu.snapshot();
        assert_eq!(snapshot.input_scale, 0x3F000000);
        assert_eq!(snapshot.input_zero_point, 128);
    }

    #[test]
    fn test_tpu_reset() {
        let mut tpu = Tpu::new();
        write_u32(&mut tpu, regs::MATRIX_A_ROWS, 10);
        write_u32(&mut tpu, regs::CONTROL, control_bits::START);

        <Tpu as Accelerator>::reset(&mut tpu);

        assert_eq!(read_u32(&tpu, regs::MATRIX_A_ROWS), 0);
        assert_eq!(read_u32(&tpu, regs::MATRICES_COMPUTED_LOW), 0);
        assert_eq!(read_u32(&tpu, regs::STATUS), status_bits::IDLE);
    }
}
