//! GPU (Graphics Processing Unit) MMIO peripheral.
//!
//! The GPU is modeled as a memory-mapped accelerator for neural network
//! operations including matrix multiplication, convolution, and activation
//! functions.

use crate::error::{Result, SimError};
use crate::peripheral::dma;
use crate::traits::{
    Accelerator, AcceleratorPerfCounters, AcceleratorType, KernelType, Memory, Peripheral,
    Precision,
};
use crate::types::Addr;
use std::any::Any;
use std::collections::VecDeque;

/// GPU base address (V2 topology).
pub const GPU_BASE: u32 = 0x2001_2000;
/// GPU MMIO region size (4KB).
pub const GPU_SIZE: usize = 0x1000;

// Register offsets
mod regs {
    pub const CONTROL: u32 = 0x00;
    pub const STATUS: u32 = 0x04;
    pub const KERNEL_TYPE: u32 = 0x08;
    pub const PRECISION: u32 = 0x0C;

    // Input tensor descriptors (up to 3 inputs)
    pub const INPUT0_DESC_LOW: u32 = 0x10;
    pub const INPUT0_DESC_HIGH: u32 = 0x14;
    pub const INPUT1_DESC_LOW: u32 = 0x18;
    pub const INPUT1_DESC_HIGH: u32 = 0x1C;
    pub const INPUT2_DESC_LOW: u32 = 0x20;
    pub const INPUT2_DESC_HIGH: u32 = 0x24;

    // Output tensor descriptors (up to 2 outputs)
    pub const OUTPUT0_DESC_LOW: u32 = 0x30;
    pub const OUTPUT0_DESC_HIGH: u32 = 0x34;
    pub const OUTPUT1_DESC_LOW: u32 = 0x38;
    pub const OUTPUT1_DESC_HIGH: u32 = 0x3C;

    // Conv2d / Pool2d parameters
    pub const CONV_KERNEL_SIZE: u32 = 0x40; // (kernel_h << 16) | kernel_w
    pub const CONV_STRIDE: u32 = 0x44; // (stride_h << 16) | stride_w
    pub const CONV_PADDING: u32 = 0x48; // (pad_h << 16) | pad_w
    pub const CONV_INPUT_DIMS: u32 = 0x4C; // (in_h << 16) | in_w
    pub const CONV_CHANNELS: u32 = 0x90; // (in_channels << 16) | out_channels

    // Command queue
    pub const CMD_QUEUE_BASE_LOW: u32 = 0x50;
    pub const CMD_QUEUE_BASE_HIGH: u32 = 0x54;
    pub const CMD_QUEUE_LEN: u32 = 0x58;
    pub const CMD_QUEUE_NOTIFY: u32 = 0x5C;

    // Performance counters
    pub const KERNELS_EXECUTED_LOW: u32 = 0x60;
    pub const KERNELS_EXECUTED_HIGH: u32 = 0x64;
    pub const CYCLES_LOW: u32 = 0x68;
    pub const CYCLES_HIGH: u32 = 0x6C;
    pub const OPS_COUNT_LOW: u32 = 0x70;
    pub const OPS_COUNT_HIGH: u32 = 0x74;
    pub const BYTES_TRANSFERRED_LOW: u32 = 0x78;
    pub const BYTES_TRANSFERRED_HIGH: u32 = 0x7C;

    // Error status
    pub const ERROR_CODE: u32 = 0x80;
    pub const TASKS_DONE: u32 = 0x84;
    pub const TASKS_ERROR: u32 = 0x88;
}

// Control register bits
mod control_bits {
    pub const START: u32 = 1 << 0;
    pub const RESET: u32 = 1 << 1;
    pub const IRQ_EN: u32 = 1 << 2;
}

// Status register bits
mod status_bits {
    pub const IDLE: u32 = 0;
    pub const BUSY: u32 = 1 << 0;
    pub const DONE: u32 = 1 << 1;
    pub const ERROR: u32 = 1 << 2;
    pub const IRQ_PENDING: u32 = 1 << 3;
}

/// GPU snapshot for visualization and debugging.
#[derive(Debug, Clone)]
pub struct GpuSnapshot {
    pub control: u32,
    pub status: u32,
    pub kernel_type: u32,
    pub precision: u32,
    pub input0_desc_addr: u64,
    pub input1_desc_addr: u64,
    pub input2_desc_addr: u64,
    pub output0_desc_addr: u64,
    pub output1_desc_addr: u64,
    pub cmd_queue_addr: u64,
    pub cmd_queue_len: u32,
    pub kernels_executed: u64,
    pub cycles: u64,
    pub ops_count: u64,
    pub bytes_transferred: u64,
    pub error_code: u32,
    pub tasks_done: u32,
    pub tasks_error: u32,
    pub work_queue_len: usize,
    pub conv_kernel_size: u32,
    pub conv_stride: u32,
    pub conv_padding: u32,
    pub conv_input_dims: u32,
    pub conv_channels: u32,
}

/// Internal work item for async execution.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct GpuWork {
    kernel_type: KernelType,
    precision: Precision,
    input_desc_addrs: [u64; 3],
    output_desc_addrs: [u64; 2],
}

/// GPU peripheral implementation.
#[derive(Debug, Clone)]
pub struct Gpu {
    base: Addr,
    control: u32,
    status: u32,
    kernel_type: u32,
    precision: u32,

    // Tensor descriptor addresses
    input_desc_addrs: [u64; 3],
    output_desc_addrs: [u64; 2],

    // Command queue
    cmd_queue_addr: u64,
    cmd_queue_len: u32,
    pending_notify: bool,

    // Conv2d / Pool2d parameters
    conv_kernel_size: u32,
    conv_stride: u32,
    conv_padding: u32,
    conv_input_dims: u32,
    conv_channels: u32,

    // Performance counters
    kernels_executed: u64,
    cycles: u64,
    ops_count: u64,
    bytes_transferred: u64,

    // Error tracking
    error_code: u32,
    tasks_done: u32,
    tasks_error: u32,

    // Async work queue
    work_queue: VecDeque<GpuWork>,
    soft_async_budget: Option<usize>,

    // Pending start flag (set by write to CONTROL.START, consumed by bus)
    pending_start: bool,
}

impl Default for Gpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Gpu {
    /// Create a new GPU peripheral.
    pub fn new() -> Self {
        Self::with_base(Addr::new(GPU_BASE))
    }

    /// Create a new GPU with a custom base address.
    pub fn with_base(base: Addr) -> Self {
        Self {
            base,
            control: 0,
            status: status_bits::IDLE,
            kernel_type: KernelType::MatMul as u32,
            precision: Precision::Fp32 as u32,
            input_desc_addrs: [0; 3],
            output_desc_addrs: [0; 2],
            cmd_queue_addr: 0,
            cmd_queue_len: 0,
            pending_notify: false,
            conv_kernel_size: 0x0001_0001, // 1x1 default
            conv_stride: 0x0001_0001,      // stride 1x1
            conv_padding: 0,
            conv_input_dims: 0,
            conv_channels: 0x0001_0001, // 1 in, 1 out
            kernels_executed: 0,
            cycles: 0,
            ops_count: 0,
            bytes_transferred: 0,
            error_code: 0,
            tasks_done: 0,
            tasks_error: 0,
            work_queue: VecDeque::new(),
            soft_async_budget: None,
            pending_start: false,
        }
    }

    /// Get a snapshot of the GPU state.
    pub fn snapshot(&self) -> GpuSnapshot {
        GpuSnapshot {
            control: self.control,
            status: self.status,
            kernel_type: self.kernel_type,
            precision: self.precision,
            input0_desc_addr: self.input_desc_addrs[0],
            input1_desc_addr: self.input_desc_addrs[1],
            input2_desc_addr: self.input_desc_addrs[2],
            output0_desc_addr: self.output_desc_addrs[0],
            output1_desc_addr: self.output_desc_addrs[1],
            cmd_queue_addr: self.cmd_queue_addr,
            cmd_queue_len: self.cmd_queue_len,
            kernels_executed: self.kernels_executed,
            cycles: self.cycles,
            ops_count: self.ops_count,
            bytes_transferred: self.bytes_transferred,
            error_code: self.error_code,
            tasks_done: self.tasks_done,
            tasks_error: self.tasks_error,
            work_queue_len: self.work_queue.len(),
            conv_kernel_size: self.conv_kernel_size,
            conv_stride: self.conv_stride,
            conv_padding: self.conv_padding,
            conv_input_dims: self.conv_input_dims,
            conv_channels: self.conv_channels,
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
            regs::PRECISION => self.precision,
            regs::INPUT0_DESC_LOW => self.input_desc_addrs[0] as u32,
            regs::INPUT0_DESC_HIGH => (self.input_desc_addrs[0] >> 32) as u32,
            regs::INPUT1_DESC_LOW => self.input_desc_addrs[1] as u32,
            regs::INPUT1_DESC_HIGH => (self.input_desc_addrs[1] >> 32) as u32,
            regs::INPUT2_DESC_LOW => self.input_desc_addrs[2] as u32,
            regs::INPUT2_DESC_HIGH => (self.input_desc_addrs[2] >> 32) as u32,
            regs::OUTPUT0_DESC_LOW => self.output_desc_addrs[0] as u32,
            regs::OUTPUT0_DESC_HIGH => (self.output_desc_addrs[0] >> 32) as u32,
            regs::OUTPUT1_DESC_LOW => self.output_desc_addrs[1] as u32,
            regs::OUTPUT1_DESC_HIGH => (self.output_desc_addrs[1] >> 32) as u32,
            regs::CMD_QUEUE_BASE_LOW => self.cmd_queue_addr as u32,
            regs::CMD_QUEUE_BASE_HIGH => (self.cmd_queue_addr >> 32) as u32,
            regs::CMD_QUEUE_LEN => self.cmd_queue_len,
            regs::KERNELS_EXECUTED_LOW => self.kernels_executed as u32,
            regs::KERNELS_EXECUTED_HIGH => (self.kernels_executed >> 32) as u32,
            regs::CYCLES_LOW => self.cycles as u32,
            regs::CYCLES_HIGH => (self.cycles >> 32) as u32,
            regs::OPS_COUNT_LOW => self.ops_count as u32,
            regs::OPS_COUNT_HIGH => (self.ops_count >> 32) as u32,
            regs::BYTES_TRANSFERRED_LOW => self.bytes_transferred as u32,
            regs::BYTES_TRANSFERRED_HIGH => (self.bytes_transferred >> 32) as u32,
            regs::ERROR_CODE => self.error_code,
            regs::TASKS_DONE => self.tasks_done,
            regs::TASKS_ERROR => self.tasks_error,
            regs::CONV_KERNEL_SIZE => self.conv_kernel_size,
            regs::CONV_STRIDE => self.conv_stride,
            regs::CONV_PADDING => self.conv_padding,
            regs::CONV_INPUT_DIMS => self.conv_input_dims,
            regs::CONV_CHANNELS => self.conv_channels,
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
            regs::PRECISION => self.precision = value,
            regs::INPUT0_DESC_LOW => {
                self.input_desc_addrs[0] =
                    (self.input_desc_addrs[0] & 0xFFFF_FFFF_0000_0000) | value as u64;
            }
            regs::INPUT0_DESC_HIGH => {
                self.input_desc_addrs[0] =
                    (self.input_desc_addrs[0] & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
            }
            regs::INPUT1_DESC_LOW => {
                self.input_desc_addrs[1] =
                    (self.input_desc_addrs[1] & 0xFFFF_FFFF_0000_0000) | value as u64;
            }
            regs::INPUT1_DESC_HIGH => {
                self.input_desc_addrs[1] =
                    (self.input_desc_addrs[1] & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
            }
            regs::INPUT2_DESC_LOW => {
                self.input_desc_addrs[2] =
                    (self.input_desc_addrs[2] & 0xFFFF_FFFF_0000_0000) | value as u64;
            }
            regs::INPUT2_DESC_HIGH => {
                self.input_desc_addrs[2] =
                    (self.input_desc_addrs[2] & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
            }
            regs::OUTPUT0_DESC_LOW => {
                self.output_desc_addrs[0] =
                    (self.output_desc_addrs[0] & 0xFFFF_FFFF_0000_0000) | value as u64;
            }
            regs::OUTPUT0_DESC_HIGH => {
                self.output_desc_addrs[0] =
                    (self.output_desc_addrs[0] & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
            }
            regs::OUTPUT1_DESC_LOW => {
                self.output_desc_addrs[1] =
                    (self.output_desc_addrs[1] & 0xFFFF_FFFF_0000_0000) | value as u64;
            }
            regs::OUTPUT1_DESC_HIGH => {
                self.output_desc_addrs[1] =
                    (self.output_desc_addrs[1] & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
            }
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
            regs::CONV_KERNEL_SIZE => self.conv_kernel_size = value,
            regs::CONV_STRIDE => self.conv_stride = value,
            regs::CONV_PADDING => self.conv_padding = value,
            regs::CONV_INPUT_DIMS => self.conv_input_dims = value,
            regs::CONV_CHANNELS => self.conv_channels = value,
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

    /// Delegate to shared DMA module.
    fn read_guest_u32(ram: &mut dma::RamRegions, addr: u64) -> Result<u32> {
        dma::read_u32(ram, addr)
    }
    fn write_guest_u32(ram: &mut dma::RamRegions, addr: u64, value: u32) -> Result<()> {
        dma::write_u32(ram, addr, value)
    }
    fn read_guest_f32_slice(
        ram: &mut dma::RamRegions,
        addr: u64,
        count: usize,
    ) -> Result<Vec<f32>> {
        dma::read_f32_slice(ram, addr, count)
    }
    fn write_guest_f32_slice(ram: &mut dma::RamRegions, addr: u64, data: &[f32]) -> Result<()> {
        dma::write_f32_slice(ram, addr, data)
    }
    fn read_guest_i8_slice(ram: &mut dma::RamRegions, addr: u64, count: usize) -> Result<Vec<i8>> {
        dma::read_i8_slice(ram, addr, count)
    }
    fn read_tensor_desc(
        ram: &mut dma::RamRegions,
        desc_addr: u64,
    ) -> Result<(u64, usize, [u32; 3])> {
        dma::read_tensor_desc(ram, desc_addr)
    }

    /// Execute the current kernel using register-configured tensor descriptors.

    /// C = A * B  (FP32 row-major)
    /// A: [M x K], B: [K x N], C: [M x N]
    fn kernel_matmul_fp32(a: &[f32], b: &[f32], m: usize, n: usize, k: usize) -> Vec<f32> {
        let mut c = vec![0.0f32; m * n];
        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0f32;
                for p in 0..k {
                    sum += a[i * k + p] * b[p * n + j];
                }
                c[i * n + j] = sum;
            }
        }
        c
    }

    /// C = A + B  (element-wise, FP32)
    fn kernel_add_fp32(a: &[f32], b: &[f32]) -> Vec<f32> {
        a.iter().zip(b.iter()).map(|(&x, &y)| x + y).collect()
    }

    /// C = A * B  (element-wise, FP32)
    fn kernel_mul_fp32(a: &[f32], b: &[f32]) -> Vec<f32> {
        a.iter().zip(b.iter()).map(|(&x, &y)| x * y).collect()
    }

    /// dot(A, B) → single f32 written as output[0]
    fn kernel_dot_fp32(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b.iter()).map(|(&x, &y)| x * y).sum()
    }

    /// ReLU: max(0, x)
    fn kernel_relu_fp32(data: &[f32]) -> Vec<f32> {
        data.iter().map(|&x| x.max(0.0)).collect()
    }

    /// Sigmoid: 1 / (1 + exp(-x))
    fn kernel_sigmoid_fp32(data: &[f32]) -> Vec<f32> {
        data.iter().map(|&x| 1.0 / (1.0 + (-x).exp())).collect()
    }

    /// Softmax (FP32, numerically stable)
    fn kernel_softmax_fp32(data: &[f32]) -> Vec<f32> {
        if data.is_empty() {
            return vec![];
        }
        let max = data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let exps: Vec<f32> = data.iter().map(|&x| (x - max).exp()).collect();
        let sum: f32 = exps.iter().sum();
        exps.iter().map(|&e| e / sum).collect()
    }

    /// Tanh activation
    fn kernel_tanh_fp32(data: &[f32]) -> Vec<f32> {
        data.iter().map(|&x| x.tanh()).collect()
    }

    /// LeakyReLU: x if x >= 0, else alpha * x
    fn kernel_leaky_relu_fp32(data: &[f32], alpha: f32) -> Vec<f32> {
        data.iter()
            .map(|&x| if x >= 0.0 { x } else { alpha * x })
            .collect()
    }

    /// ReLU6: min(max(0, x), 6)
    fn kernel_relu6_fp32(data: &[f32]) -> Vec<f32> {
        data.iter().map(|&x| x.max(0.0).min(6.0)).collect()
    }

    /// Vector scale: a * scalar
    fn kernel_scale_fp32(a: &[f32], scalar: f32) -> Vec<f32> {
        a.iter().map(|&x| x * scalar).collect()
    }

    /// Direct Conv2d FP32.
    ///
    /// input: [C_in * H * W] (flattened, row-major per channel)
    /// filter: [C_out * C_in * KH * KW] (flattened)
    /// output: [C_out * H_out * W_out] (flattened)
    fn kernel_conv2d_fp32(
        input: &[f32],
        filter: &[f32],
        c_in: usize,
        c_out: usize,
        in_h: usize,
        in_w: usize,
        kh: usize,
        kw: usize,
        stride_h: usize,
        stride_w: usize,
        pad_h: usize,
        pad_w: usize,
    ) -> Vec<f32> {
        let out_h = (in_h + 2 * pad_h - kh) / stride_h + 1;
        let out_w = (in_w + 2 * pad_w - kw) / stride_w + 1;
        let mut output = vec![0.0f32; c_out * out_h * out_w];

        for co in 0..c_out {
            for oh in 0..out_h {
                for ow in 0..out_w {
                    let mut sum = 0.0f32;
                    for ci in 0..c_in {
                        for fh in 0..kh {
                            for fw in 0..kw {
                                let ih = oh * stride_h + fh;
                                let iw = ow * stride_w + fw;
                                // Apply padding (out-of-bounds = 0)
                                if ih >= pad_h
                                    && iw >= pad_w
                                    && ih < in_h + pad_h
                                    && iw < in_w + pad_w
                                {
                                    let real_ih = ih - pad_h;
                                    let real_iw = iw - pad_w;
                                    let in_val = input[ci * in_h * in_w + real_ih * in_w + real_iw];
                                    let f_val =
                                        filter[co * c_in * kh * kw + ci * kh * kw + fh * kw + fw];
                                    sum += in_val * f_val;
                                }
                            }
                        }
                    }
                    output[co * out_h * out_w + oh * out_w + ow] = sum;
                }
            }
        }
        output
    }

    /// MaxPool2d FP32.
    ///
    /// input: [C * H * W], output: [C * H_out * W_out]
    fn kernel_pool2d_max_fp32(
        input: &[f32],
        channels: usize,
        in_h: usize,
        in_w: usize,
        kh: usize,
        kw: usize,
        stride_h: usize,
        stride_w: usize,
        pad_h: usize,
        pad_w: usize,
    ) -> Vec<f32> {
        let out_h = (in_h + 2 * pad_h - kh) / stride_h + 1;
        let out_w = (in_w + 2 * pad_w - kw) / stride_w + 1;
        let mut output = vec![f32::NEG_INFINITY; channels * out_h * out_w];

        for c in 0..channels {
            for oh in 0..out_h {
                for ow in 0..out_w {
                    let mut max_val = f32::NEG_INFINITY;
                    for fh in 0..kh {
                        for fw in 0..kw {
                            let ih = oh * stride_h + fh;
                            let iw = ow * stride_w + fw;
                            if ih >= pad_h && iw >= pad_w && ih < in_h + pad_h && iw < in_w + pad_w
                            {
                                let real_ih = ih - pad_h;
                                let real_iw = iw - pad_w;
                                let val = input[c * in_h * in_w + real_ih * in_w + real_iw];
                                if val > max_val {
                                    max_val = val;
                                }
                            }
                        }
                    }
                    output[c * out_h * out_w + oh * out_w + ow] = max_val;
                }
            }
        }
        output
    }

    /// AvgPool2d FP32.
    fn kernel_pool2d_avg_fp32(
        input: &[f32],
        channels: usize,
        in_h: usize,
        in_w: usize,
        kh: usize,
        kw: usize,
        stride_h: usize,
        stride_w: usize,
        pad_h: usize,
        pad_w: usize,
    ) -> Vec<f32> {
        let out_h = (in_h + 2 * pad_h - kh) / stride_h + 1;
        let out_w = (in_w + 2 * pad_w - kw) / stride_w + 1;
        let mut output = vec![0.0f32; channels * out_h * out_w];

        for c in 0..channels {
            for oh in 0..out_h {
                for ow in 0..out_w {
                    let mut sum = 0.0f32;
                    let mut count = 0;
                    for fh in 0..kh {
                        for fw in 0..kw {
                            let ih = oh * stride_h + fh;
                            let iw = ow * stride_w + fw;
                            if ih >= pad_h && iw >= pad_w && ih < in_h + pad_h && iw < in_w + pad_w
                            {
                                let real_ih = ih - pad_h;
                                let real_iw = iw - pad_w;
                                sum += input[c * in_h * in_w + real_ih * in_w + real_iw];
                                count += 1;
                            }
                        }
                    }
                    output[c * out_h * out_w + oh * out_w + ow] =
                        if count > 0 { sum / count as f32 } else { 0.0 };
                }
            }
        }
        output
    }

    // ── Execute path ───────────────────────────────────────────────────

    /// Execute the current kernel using register-configured tensor descriptors.
    fn execute_once_with_memory(&mut self, ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>) {
        self.status |= status_bits::BUSY;
        self.status &= !(status_bits::DONE | status_bits::ERROR);

        let kernel = KernelType::from_u32(self.kernel_type);
        let prec = Precision::from_u32(self.precision);

        let result = match kernel {
            KernelType::MatMul => {
                if prec == Precision::Fp32 {
                    self.execute_matmul_fp32(ram_regions)
                } else {
                    self.execute_matmul_int8(ram_regions)
                }
            }
            KernelType::MatAdd => self.execute_binop_fp32(ram_regions, Self::kernel_add_fp32),
            KernelType::VectorAdd => self.execute_binop_fp32(ram_regions, Self::kernel_add_fp32),
            KernelType::VectorMul => self.execute_binop_fp32(ram_regions, Self::kernel_mul_fp32),
            KernelType::VectorDot => self.execute_dot_fp32(ram_regions),
            KernelType::Relu => self.execute_unary_fp32(ram_regions, Self::kernel_relu_fp32),
            KernelType::Sigmoid => self.execute_unary_fp32(ram_regions, Self::kernel_sigmoid_fp32),
            KernelType::Softmax => self.execute_unary_fp32(ram_regions, Self::kernel_softmax_fp32),
            KernelType::Tanh => self.execute_unary_fp32(ram_regions, Self::kernel_tanh_fp32),
            KernelType::LeakyRelu => self.execute_leaky_relu_fp32(ram_regions),
            KernelType::Relu6 => self.execute_unary_fp32(ram_regions, Self::kernel_relu6_fp32),
            KernelType::VectorScale => self.execute_scale_fp32(ram_regions),
            KernelType::Conv2d => self.execute_conv2d_fp32(ram_regions),
            KernelType::Pool2dMax => self.execute_pool2d_fp32(ram_regions, true),
            KernelType::Pool2dAvg => self.execute_pool2d_fp32(ram_regions, false),
            _ => {
                self.error_code = 1; // unsupported kernel
                self.tasks_error = self.tasks_error.wrapping_add(1);
                self.finish_op(true);
                return;
            }
        };

        match result {
            Ok(ops) => {
                self.ops_count = self.ops_count.wrapping_add(ops);
                self.kernels_executed = self.kernels_executed.wrapping_add(1);
                self.tasks_done = self.tasks_done.wrapping_add(1);
                self.finish_op(false);
            }
            Err(_) => {
                self.error_code = 2; // execution error
                self.tasks_error = self.tasks_error.wrapping_add(1);
                self.finish_op(true);
            }
        }
    }

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

    // ── Kernel dispatchers ─────────────────────────────────────────────

    /// MatMul FP32: input0=[M,K], input1=[K,N] → output0=[M,N]
    fn execute_matmul_fp32(
        &mut self,
        ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
    ) -> Result<u64> {
        let (a_addr, _a_cnt, a_shape) =
            Self::read_tensor_desc(ram_regions, self.input_desc_addrs[0])?;
        let (b_addr, _b_cnt, b_shape) =
            Self::read_tensor_desc(ram_regions, self.input_desc_addrs[1])?;
        let (c_addr, _c_cnt, _c_shape) =
            Self::read_tensor_desc(ram_regions, self.output_desc_addrs[0])?;

        let m = a_shape[0] as usize;
        let k = a_shape[1] as usize;
        let n = b_shape[1] as usize;
        if m == 0 || k == 0 || n == 0 {
            return Ok(0);
        }

        let a = Self::read_guest_f32_slice(ram_regions, a_addr, m * k)?;
        let b = Self::read_guest_f32_slice(ram_regions, b_addr, k * n)?;
        let c = Self::kernel_matmul_fp32(&a, &b, m, n, k);
        Self::write_guest_f32_slice(ram_regions, c_addr, &c)?;

        let bytes = (m * k + k * n + m * n) as u64 * 4;
        self.bytes_transferred = self.bytes_transferred.wrapping_add(bytes);
        self.cycles = self.cycles.wrapping_add((m * n * k) as u64);
        Ok((m * n * k * 2) as u64)
    }

    /// MatMul INT8: uses i8 inputs, accumulates to i32, writes back as i32
    fn execute_matmul_int8(
        &mut self,
        ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
    ) -> Result<u64> {
        let (a_addr, _a_cnt, a_shape) =
            Self::read_tensor_desc(ram_regions, self.input_desc_addrs[0])?;
        let (b_addr, _b_cnt, b_shape) =
            Self::read_tensor_desc(ram_regions, self.input_desc_addrs[1])?;
        let (c_addr, _c_cnt, _c_shape) =
            Self::read_tensor_desc(ram_regions, self.output_desc_addrs[0])?;

        let m = a_shape[0] as usize;
        let k = a_shape[1] as usize;
        let n = b_shape[1] as usize;
        if m == 0 || k == 0 || n == 0 {
            return Ok(0);
        }

        let a = Self::read_guest_i8_slice(ram_regions, a_addr, m * k)?;
        let b = Self::read_guest_i8_slice(ram_regions, b_addr, k * n)?;

        let mut c = vec![0i32; m * n];
        for i in 0..m {
            for j in 0..n {
                let mut sum: i32 = 0;
                for p in 0..k {
                    sum += (a[i * k + p] as i32) * (b[p * n + j] as i32);
                }
                c[i * n + j] = sum;
            }
        }
        // write i32 results
        for (i, &v) in c.iter().enumerate() {
            Self::write_guest_u32(ram_regions, c_addr + (i as u64) * 4, v as u32)?;
        }

        let bytes = (m * k + k * n) as u64 + (m * n) as u64 * 4;
        self.bytes_transferred = self.bytes_transferred.wrapping_add(bytes);
        self.cycles = self.cycles.wrapping_add((m * n * k) as u64);
        Ok((m * n * k * 2) as u64)
    }

    /// Binary element-wise FP32 (add / mul)
    fn execute_binop_fp32<F>(
        &mut self,
        ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
        op: F,
    ) -> Result<u64>
    where
        F: Fn(&[f32], &[f32]) -> Vec<f32>,
    {
        let (a_addr, a_cnt, _) = Self::read_tensor_desc(ram_regions, self.input_desc_addrs[0])?;
        let (b_addr, b_cnt, _) = Self::read_tensor_desc(ram_regions, self.input_desc_addrs[1])?;
        let (c_addr, _c_cnt, _) = Self::read_tensor_desc(ram_regions, self.output_desc_addrs[0])?;

        let n = a_cnt.min(b_cnt);
        if n == 0 {
            return Ok(0);
        }

        let a = Self::read_guest_f32_slice(ram_regions, a_addr, n)?;
        let b = Self::read_guest_f32_slice(ram_regions, b_addr, n)?;
        let c = op(&a, &b);
        Self::write_guest_f32_slice(ram_regions, c_addr, &c)?;

        let bytes = (n * 3) as u64 * 4;
        self.bytes_transferred = self.bytes_transferred.wrapping_add(bytes);
        self.cycles = self.cycles.wrapping_add(n as u64);
        Ok(n as u64)
    }

    /// Dot product FP32: result is a single f32
    fn execute_dot_fp32(
        &mut self,
        ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
    ) -> Result<u64> {
        let (a_addr, a_cnt, _) = Self::read_tensor_desc(ram_regions, self.input_desc_addrs[0])?;
        let (b_addr, b_cnt, _) = Self::read_tensor_desc(ram_regions, self.input_desc_addrs[1])?;
        let (c_addr, _c_cnt, _) = Self::read_tensor_desc(ram_regions, self.output_desc_addrs[0])?;

        let n = a_cnt.min(b_cnt);
        if n == 0 {
            return Ok(0);
        }

        let a = Self::read_guest_f32_slice(ram_regions, a_addr, n)?;
        let b = Self::read_guest_f32_slice(ram_regions, b_addr, n)?;
        let dot = Self::kernel_dot_fp32(&a, &b);
        Self::write_guest_u32(ram_regions, c_addr, dot.to_bits())?;

        self.bytes_transferred = self.bytes_transferred.wrapping_add((n * 2 + 1) as u64 * 4);
        self.cycles = self.cycles.wrapping_add(n as u64);
        Ok(n as u64)
    }

    /// Unary FP32 (relu / sigmoid / softmax)
    fn execute_unary_fp32<F>(
        &mut self,
        ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
        op: F,
    ) -> Result<u64>
    where
        F: Fn(&[f32]) -> Vec<f32>,
    {
        let (a_addr, a_cnt, _) = Self::read_tensor_desc(ram_regions, self.input_desc_addrs[0])?;
        let (c_addr, _c_cnt, _) = Self::read_tensor_desc(ram_regions, self.output_desc_addrs[0])?;

        if a_cnt == 0 {
            return Ok(0);
        }

        let a = Self::read_guest_f32_slice(ram_regions, a_addr, a_cnt)?;
        let c = op(&a);
        Self::write_guest_f32_slice(ram_regions, c_addr, &c)?;

        let bytes = (a_cnt * 2) as u64 * 4;
        self.bytes_transferred = self.bytes_transferred.wrapping_add(bytes);
        self.cycles = self.cycles.wrapping_add(a_cnt as u64);
        Ok(a_cnt as u64)
    }

    /// LeakyReLU FP32 (uses CONV_PADDING low byte as alpha parameter).
    fn execute_leaky_relu_fp32(
        &mut self,
        ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
    ) -> Result<u64> {
        let alpha_bits = self.conv_padding & 0xFFFF;
        let alpha = if alpha_bits != 0 {
            f32::from_bits(alpha_bits)
        } else {
            0.01 // default LeakyReLU alpha
        };

        let (a_addr, a_cnt, _) = Self::read_tensor_desc(ram_regions, self.input_desc_addrs[0])?;
        let (c_addr, _c_cnt, _) = Self::read_tensor_desc(ram_regions, self.output_desc_addrs[0])?;

        if a_cnt == 0 {
            return Ok(0);
        }

        let a = Self::read_guest_f32_slice(ram_regions, a_addr, a_cnt)?;
        let c = Self::kernel_leaky_relu_fp32(&a, alpha);
        Self::write_guest_f32_slice(ram_regions, c_addr, &c)?;

        let bytes = (a_cnt * 2) as u64 * 4;
        self.bytes_transferred = self.bytes_transferred.wrapping_add(bytes);
        self.cycles = self.cycles.wrapping_add(a_cnt as u64);
        Ok(a_cnt as u64)
    }

    /// VectorScale FP32: input0 * scalar.
    /// Uses CONV_PADDING low half as scalar (f16 bits) or fallback to input1 as scalar tensor.
    fn execute_scale_fp32(
        &mut self,
        ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
    ) -> Result<u64> {
        let (a_addr, a_cnt, _) = Self::read_tensor_desc(ram_regions, self.input_desc_addrs[0])?;
        let (c_addr, _c_cnt, _) = Self::read_tensor_desc(ram_regions, self.output_desc_addrs[0])?;

        if a_cnt == 0 {
            return Ok(0);
        }

        // Use input1 descriptor as scalar if available, else use conv_padding as f32
        let scalar = if self.input_desc_addrs[1] != 0 {
            let (s_addr, _s_cnt, _) =
                Self::read_tensor_desc(ram_regions, self.input_desc_addrs[1])?;
            let s_bits = Self::read_guest_u32(ram_regions, s_addr)?;
            f32::from_bits(s_bits)
        } else {
            f32::from_bits(self.conv_padding)
        };

        let a = Self::read_guest_f32_slice(ram_regions, a_addr, a_cnt)?;
        let c = Self::kernel_scale_fp32(&a, scalar);
        Self::write_guest_f32_slice(ram_regions, c_addr, &c)?;

        let bytes = (a_cnt * 2) as u64 * 4;
        self.bytes_transferred = self.bytes_transferred.wrapping_add(bytes);
        self.cycles = self.cycles.wrapping_add(a_cnt as u64);
        Ok(a_cnt as u64)
    }

    /// Conv2d FP32 dispatcher.
    fn execute_conv2d_fp32(
        &mut self,
        ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
    ) -> Result<u64> {
        let kh = ((self.conv_kernel_size >> 16) & 0xFFFF) as usize;
        let kw = (self.conv_kernel_size & 0xFFFF) as usize;
        let stride_h = ((self.conv_stride >> 16) & 0xFFFF) as usize;
        let stride_w = (self.conv_stride & 0xFFFF) as usize;
        let pad_h = ((self.conv_padding >> 16) & 0xFFFF) as usize;
        let pad_w = (self.conv_padding & 0xFFFF) as usize;
        let in_h = ((self.conv_input_dims >> 16) & 0xFFFF) as usize;
        let in_w = (self.conv_input_dims & 0xFFFF) as usize;
        let c_in = ((self.conv_channels >> 16) & 0xFFFF) as usize;
        let c_out = (self.conv_channels & 0xFFFF) as usize;

        if kh == 0
            || kw == 0
            || stride_h == 0
            || stride_w == 0
            || in_h == 0
            || in_w == 0
            || c_in == 0
            || c_out == 0
        {
            self.error_code = 3; // invalid conv params
            return Ok(0);
        }

        let (input_addr, _, _) = Self::read_tensor_desc(ram_regions, self.input_desc_addrs[0])?;
        let (filter_addr, _, _) = Self::read_tensor_desc(ram_regions, self.input_desc_addrs[1])?;
        let (output_addr, _, _) = Self::read_tensor_desc(ram_regions, self.output_desc_addrs[0])?;

        let input = Self::read_guest_f32_slice(ram_regions, input_addr, c_in * in_h * in_w)?;
        let filter = Self::read_guest_f32_slice(ram_regions, filter_addr, c_out * c_in * kh * kw)?;
        let output = Self::kernel_conv2d_fp32(
            &input, &filter, c_in, c_out, in_h, in_w, kh, kw, stride_h, stride_w, pad_h, pad_w,
        );
        Self::write_guest_f32_slice(ram_regions, output_addr, &output)?;

        let out_h = (in_h + 2 * pad_h - kh) / stride_h + 1;
        let out_w = (in_w + 2 * pad_w - kw) / stride_w + 1;
        let ops = (c_out * out_h * out_w * c_in * kh * kw * 2) as u64;
        let bytes =
            ((c_in * in_h * in_w) + (c_out * c_in * kh * kw) + (c_out * out_h * out_w)) as u64 * 4;
        self.bytes_transferred = self.bytes_transferred.wrapping_add(bytes);
        self.cycles = self.cycles.wrapping_add(ops / 2);
        Ok(ops)
    }

    /// Pool2d FP32 dispatcher (max or avg).
    fn execute_pool2d_fp32(
        &mut self,
        ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
        is_max: bool,
    ) -> Result<u64> {
        let kh = ((self.conv_kernel_size >> 16) & 0xFFFF) as usize;
        let kw = (self.conv_kernel_size & 0xFFFF) as usize;
        let stride_h = ((self.conv_stride >> 16) & 0xFFFF) as usize;
        let stride_w = (self.conv_stride & 0xFFFF) as usize;
        let pad_h = ((self.conv_padding >> 16) & 0xFFFF) as usize;
        let pad_w = (self.conv_padding & 0xFFFF) as usize;
        let in_h = ((self.conv_input_dims >> 16) & 0xFFFF) as usize;
        let in_w = (self.conv_input_dims & 0xFFFF) as usize;
        let channels = ((self.conv_channels >> 16) & 0xFFFF) as usize;

        if kh == 0
            || kw == 0
            || stride_h == 0
            || stride_w == 0
            || in_h == 0
            || in_w == 0
            || channels == 0
        {
            self.error_code = 3;
            return Ok(0);
        }

        let (input_addr, _, _) = Self::read_tensor_desc(ram_regions, self.input_desc_addrs[0])?;
        let (output_addr, _, _) = Self::read_tensor_desc(ram_regions, self.output_desc_addrs[0])?;

        let input = Self::read_guest_f32_slice(ram_regions, input_addr, channels * in_h * in_w)?;
        let output = if is_max {
            Self::kernel_pool2d_max_fp32(
                &input, channels, in_h, in_w, kh, kw, stride_h, stride_w, pad_h, pad_w,
            )
        } else {
            Self::kernel_pool2d_avg_fp32(
                &input, channels, in_h, in_w, kh, kw, stride_h, stride_w, pad_h, pad_w,
            )
        };
        Self::write_guest_f32_slice(ram_regions, output_addr, &output)?;

        let out_h = (in_h + 2 * pad_h - kh) / stride_h + 1;
        let out_w = (in_w + 2 * pad_w - kw) / stride_w + 1;
        let ops = (channels * out_h * out_w * kh * kw) as u64;
        let bytes = ((channels * in_h * in_w) + (channels * out_h * out_w)) as u64 * 4;
        self.bytes_transferred = self.bytes_transferred.wrapping_add(bytes);
        self.cycles = self.cycles.wrapping_add(ops);
        Ok(ops)
    }

    /// Process pending descriptor notification.
    ///
    /// Each command descriptor (16 bytes):
    ///   [0..4]  kernel_type (u32)
    ///   [4..8]  precision   (u32)
    ///   [8..12] input0_desc_addr (u32)
    ///   [12..16] output0_desc_addr (u32)
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
            let desc_base = self.cmd_queue_addr + (idx as u64) * 16;
            let kt = Self::read_guest_u32(ram_regions, desc_base)?;
            let prec = Self::read_guest_u32(ram_regions, desc_base + 4)?;
            let in0_desc = Self::read_guest_u32(ram_regions, desc_base + 8)? as u64;
            let out0_desc = Self::read_guest_u32(ram_regions, desc_base + 12)? as u64;

            // Temporarily set registers for this kernel
            self.kernel_type = kt;
            self.precision = prec;
            self.input_desc_addrs[0] = in0_desc;
            self.output_desc_addrs[0] = out0_desc;

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

impl Peripheral for Gpu {
    fn read(&self, offset: Addr) -> Result<u8> {
        let off = offset.raw();
        if off >= GPU_SIZE as u32 {
            return Err(SimError::InvalidAddress(offset));
        }
        Ok(self.read_u8(off))
    }

    fn write(&mut self, offset: Addr, value: u8) -> Result<()> {
        let off = offset.raw();
        if off >= GPU_SIZE as u32 {
            return Err(SimError::InvalidAddress(offset));
        }
        self.write_u8(off, value);
        Ok(())
    }

    fn base_addr(&self) -> Addr {
        self.base
    }

    fn size(&self) -> usize {
        GPU_SIZE
    }

    fn name(&self) -> &str {
        "GPU"
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

impl Accelerator for Gpu {
    fn accelerator_type(&self) -> AcceleratorType {
        AcceleratorType::Gpu
    }

    fn supported_precisions(&self) -> Vec<Precision> {
        vec![Precision::Fp32, Precision::Int8]
    }

    fn supported_kernels(&self) -> Vec<KernelType> {
        vec![
            KernelType::MatMul,
            KernelType::MatAdd,
            KernelType::VectorAdd,
            KernelType::VectorMul,
            KernelType::VectorScale,
            KernelType::VectorDot,
            KernelType::Relu,
            KernelType::Relu6,
            KernelType::LeakyRelu,
            KernelType::Sigmoid,
            KernelType::Tanh,
            KernelType::Softmax,
            KernelType::Conv2d,
            KernelType::Pool2dMax,
            KernelType::Pool2dAvg,
        ]
    }

    fn performance_counters(&self) -> AcceleratorPerfCounters {
        AcceleratorPerfCounters {
            kernels_executed: self.kernels_executed,
            cycles_elapsed: self.cycles,
            bytes_transferred: self.bytes_transferred,
            operations_count: self.ops_count,
            errors_count: self.tasks_error as u64,
        }
    }

    fn reset(&mut self) {
        self.control = 0;
        self.status = status_bits::IDLE;
        self.kernel_type = KernelType::MatMul as u32;
        self.precision = Precision::Fp32 as u32;
        self.input_desc_addrs = [0; 3];
        self.output_desc_addrs = [0; 2];
        self.cmd_queue_addr = 0;
        self.cmd_queue_len = 0;
        self.pending_notify = false;
        self.conv_kernel_size = 0x0001_0001;
        self.conv_stride = 0x0001_0001;
        self.conv_padding = 0;
        self.conv_input_dims = 0;
        self.conv_channels = 0x0001_0001;
        self.kernels_executed = 0;
        self.cycles = 0;
        self.ops_count = 0;
        self.bytes_transferred = 0;
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
    use crate::memory::Ram;

    const RAM_BASE: u32 = 0x8000_0000;
    const RAM_SIZE: usize = 0x4000;

    fn make_ram() -> Vec<(Addr, usize, Box<dyn Memory>)> {
        vec![(Addr::new(RAM_BASE), RAM_SIZE, Box::new(Ram::new(RAM_SIZE)))]
    }

    fn write_u32(gpu: &mut Gpu, reg: u32, value: u32) {
        for i in 0..4 {
            gpu.write(Addr::new(reg + i), ((value >> (i * 8)) & 0xFF) as u8)
                .unwrap();
        }
    }

    fn read_u32(gpu: &Gpu, reg: u32) -> u32 {
        let mut value = 0u32;
        for i in 0..4 {
            value |= (gpu.read(Addr::new(reg + i)).unwrap() as u32) << (i * 8);
        }
        value
    }

    #[test]
    fn test_gpu_basic() {
        let gpu = Gpu::new();
        assert_eq!(gpu.name(), "GPU");
        assert_eq!(gpu.size(), GPU_SIZE);
        assert!(!gpu.is_busy());
    }

    #[test]
    fn test_gpu_control_start_triggers_pending() {
        let mut gpu = Gpu::new();
        write_u32(&mut gpu, regs::CONTROL, control_bits::START);
        assert!(gpu.has_pending_start());
    }

    #[test]
    fn test_gpu_execute_with_memory_start() {
        let mut gpu = Gpu::new();
        let mut ram = make_ram();
        write_u32(
            &mut gpu,
            regs::CONTROL,
            control_bits::START | control_bits::IRQ_EN,
        );
        gpu.execute_with_memory(&mut ram);
        assert!(!gpu.has_pending_start());
        let status = read_u32(&gpu, regs::STATUS);
        assert_ne!(status & status_bits::DONE, 0);
        assert!(gpu.has_interrupt());
        gpu.acknowledge_interrupt();
        assert!(!gpu.has_interrupt());
    }

    #[test]
    fn test_gpu_reset() {
        let mut gpu = Gpu::new();
        write_u32(&mut gpu, regs::CONTROL, control_bits::START);
        <Gpu as Accelerator>::reset(&mut gpu);
        assert_eq!(read_u32(&gpu, regs::KERNELS_EXECUTED_LOW), 0);
        assert_eq!(read_u32(&gpu, regs::STATUS), status_bits::IDLE);
    }

    // ── Compute tests ──────────────────────────────────────────────────

    /// Helper: write a tensor descriptor into guest RAM.
    /// Layout: [data_addr(4)] [element_count(4)] [shape0(4)] [shape1(4)] [shape2(4)]
    fn write_tensor_desc(
        ram: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
        desc_addr: u64,
        data_addr: u32,
        element_count: u32,
        shape: [u32; 3],
    ) {
        dma::write_tensor_desc(ram, desc_addr, data_addr, element_count, shape).unwrap();
    }

    /// Helper: write f32 array into guest RAM.
    fn write_f32_array(ram: &mut Vec<(Addr, usize, Box<dyn Memory>)>, base: u64, data: &[f32]) {
        dma::write_f32_slice(ram, base, data).unwrap();
    }

    /// Helper: read f32 array from guest RAM.
    fn read_f32_array(
        ram: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
        base: u64,
        count: usize,
    ) -> Vec<f32> {
        dma::read_f32_slice(ram, base, count).unwrap()
    }

    #[test]
    fn test_gpu_matmul_2x2_fp32() {
        let mut gpu = Gpu::new();
        let mut ram = make_ram();

        // A = [[1,2],[3,4]], B = [[5,6],[7,8]]
        let a_addr = RAM_BASE + 0x100;
        let b_addr = RAM_BASE + 0x120;
        let c_addr = RAM_BASE + 0x140;
        let desc_a = RAM_BASE + 0x200;
        let desc_b = RAM_BASE + 0x220;
        let desc_c = RAM_BASE + 0x240;

        write_f32_array(&mut ram, a_addr as u64, &[1.0, 2.0, 3.0, 4.0]);
        write_f32_array(&mut ram, b_addr as u64, &[5.0, 6.0, 7.0, 8.0]);

        write_tensor_desc(&mut ram, desc_a as u64, a_addr, 4, [2, 2, 0]);
        write_tensor_desc(&mut ram, desc_b as u64, b_addr, 4, [2, 2, 0]);
        write_tensor_desc(&mut ram, desc_c as u64, c_addr, 4, [2, 2, 0]);

        write_u32(&mut gpu, regs::KERNEL_TYPE, KernelType::MatMul as u32);
        write_u32(&mut gpu, regs::PRECISION, Precision::Fp32 as u32);
        write_u32(&mut gpu, regs::INPUT0_DESC_LOW, desc_a);
        write_u32(&mut gpu, regs::INPUT1_DESC_LOW, desc_b);
        write_u32(&mut gpu, regs::OUTPUT0_DESC_LOW, desc_c);

        gpu.execute_with_memory(&mut ram);

        // C = A*B = [[19,22],[43,50]]
        let c = read_f32_array(&mut ram, c_addr as u64, 4);
        assert!((c[0] - 19.0).abs() < 1e-5);
        assert!((c[1] - 22.0).abs() < 1e-5);
        assert!((c[2] - 43.0).abs() < 1e-5);
        assert!((c[3] - 50.0).abs() < 1e-5);

        let status = read_u32(&gpu, regs::STATUS);
        assert_ne!(status & status_bits::DONE, 0);
        assert_eq!(status & status_bits::ERROR, 0);
    }

    #[test]
    fn test_gpu_vector_add_fp32() {
        let mut gpu = Gpu::new();
        let mut ram = make_ram();

        let a_addr = RAM_BASE + 0x100;
        let b_addr = RAM_BASE + 0x120;
        let c_addr = RAM_BASE + 0x140;
        let desc_a = RAM_BASE + 0x200;
        let desc_b = RAM_BASE + 0x220;
        let desc_c = RAM_BASE + 0x240;

        write_f32_array(&mut ram, a_addr as u64, &[1.0, 2.0, 3.0]);
        write_f32_array(&mut ram, b_addr as u64, &[10.0, 20.0, 30.0]);

        write_tensor_desc(&mut ram, desc_a as u64, a_addr, 3, [3, 0, 0]);
        write_tensor_desc(&mut ram, desc_b as u64, b_addr, 3, [3, 0, 0]);
        write_tensor_desc(&mut ram, desc_c as u64, c_addr, 3, [3, 0, 0]);

        write_u32(&mut gpu, regs::KERNEL_TYPE, KernelType::VectorAdd as u32);
        write_u32(&mut gpu, regs::PRECISION, Precision::Fp32 as u32);
        write_u32(&mut gpu, regs::INPUT0_DESC_LOW, desc_a);
        write_u32(&mut gpu, regs::INPUT1_DESC_LOW, desc_b);
        write_u32(&mut gpu, regs::OUTPUT0_DESC_LOW, desc_c);

        gpu.execute_with_memory(&mut ram);

        let c = read_f32_array(&mut ram, c_addr as u64, 3);
        assert!((c[0] - 11.0).abs() < 1e-5);
        assert!((c[1] - 22.0).abs() < 1e-5);
        assert!((c[2] - 33.0).abs() < 1e-5);
    }

    #[test]
    fn test_gpu_relu_fp32() {
        let mut gpu = Gpu::new();
        let mut ram = make_ram();

        let a_addr = RAM_BASE + 0x100;
        let c_addr = RAM_BASE + 0x120;
        let desc_a = RAM_BASE + 0x200;
        let desc_c = RAM_BASE + 0x220;

        write_f32_array(&mut ram, a_addr as u64, &[-1.0, 2.0, -3.0, 4.0]);
        write_tensor_desc(&mut ram, desc_a as u64, a_addr, 4, [4, 0, 0]);
        write_tensor_desc(&mut ram, desc_c as u64, c_addr, 4, [4, 0, 0]);

        write_u32(&mut gpu, regs::KERNEL_TYPE, KernelType::Relu as u32);
        write_u32(&mut gpu, regs::INPUT0_DESC_LOW, desc_a);
        write_u32(&mut gpu, regs::OUTPUT0_DESC_LOW, desc_c);

        gpu.execute_with_memory(&mut ram);

        let c = read_f32_array(&mut ram, c_addr as u64, 4);
        assert!((c[0] - 0.0).abs() < 1e-5);
        assert!((c[1] - 2.0).abs() < 1e-5);
        assert!((c[2] - 0.0).abs() < 1e-5);
        assert!((c[3] - 4.0).abs() < 1e-5);
    }

    #[test]
    fn test_gpu_softmax_fp32() {
        let mut gpu = Gpu::new();
        let mut ram = make_ram();

        let a_addr = RAM_BASE + 0x100;
        let c_addr = RAM_BASE + 0x120;
        let desc_a = RAM_BASE + 0x200;
        let desc_c = RAM_BASE + 0x220;

        write_f32_array(&mut ram, a_addr as u64, &[1.0, 2.0, 3.0]);
        write_tensor_desc(&mut ram, desc_a as u64, a_addr, 3, [3, 0, 0]);
        write_tensor_desc(&mut ram, desc_c as u64, c_addr, 3, [3, 0, 0]);

        write_u32(&mut gpu, regs::KERNEL_TYPE, KernelType::Softmax as u32);
        write_u32(&mut gpu, regs::INPUT0_DESC_LOW, desc_a);
        write_u32(&mut gpu, regs::OUTPUT0_DESC_LOW, desc_c);

        gpu.execute_with_memory(&mut ram);

        let c = read_f32_array(&mut ram, c_addr as u64, 3);
        let sum: f32 = c.iter().sum();
        assert!((sum - 1.0).abs() < 1e-4);
        // softmax(1,2,3) = [0.0900, 0.2447, 0.6652]
        assert!(c[2] > c[1]);
        assert!(c[1] > c[0]);
    }

    // ── Phase 3: Conv2d / Pool2d / Activation tests ────────────────────

    #[test]
    fn test_gpu_conv2d_1x1_kernel() {
        // 1 channel, 2x2 input, 1x1 kernel (1 filter), stride 1, no padding
        // This is equivalent to element-wise multiply
        let mut gpu = Gpu::new();
        let mut ram = make_ram();

        // Input: 1 channel, 2x2 = [[1,2],[3,4]]
        let input_addr = RAM_BASE + 0x100;
        // Filter: 1 out, 1 in, 1x1 = [[2]]
        let filter_addr = RAM_BASE + 0x120;
        // Output: 1 out, 2x2
        let output_addr = RAM_BASE + 0x140;
        let desc_in = RAM_BASE + 0x200;
        let desc_f = RAM_BASE + 0x220;
        let desc_out = RAM_BASE + 0x240;

        write_f32_array(&mut ram, input_addr as u64, &[1.0, 2.0, 3.0, 4.0]);
        write_f32_array(&mut ram, filter_addr as u64, &[2.0]);

        write_tensor_desc(&mut ram, desc_in as u64, input_addr, 4, [1, 2, 2]);
        write_tensor_desc(&mut ram, desc_f as u64, filter_addr, 1, [1, 1, 1]);
        write_tensor_desc(&mut ram, desc_out as u64, output_addr, 4, [1, 2, 2]);

        write_u32(&mut gpu, regs::KERNEL_TYPE, KernelType::Conv2d as u32);
        write_u32(&mut gpu, regs::INPUT0_DESC_LOW, desc_in);
        write_u32(&mut gpu, regs::INPUT1_DESC_LOW, desc_f);
        write_u32(&mut gpu, regs::OUTPUT0_DESC_LOW, desc_out);
        // kernel 1x1, stride 1x1, pad 0x0, input 2x2, channels 1in/1out
        write_u32(&mut gpu, regs::CONV_KERNEL_SIZE, (1 << 16) | 1);
        write_u32(&mut gpu, regs::CONV_STRIDE, (1 << 16) | 1);
        write_u32(&mut gpu, regs::CONV_PADDING, 0);
        write_u32(&mut gpu, regs::CONV_INPUT_DIMS, (2 << 16) | 2);
        write_u32(&mut gpu, regs::CONV_CHANNELS, (1 << 16) | 1);

        gpu.execute_with_memory(&mut ram);

        // Each element * 2: [2,4,6,8]
        let out = read_f32_array(&mut ram, output_addr as u64, 4);
        assert!((out[0] - 2.0).abs() < 1e-5);
        assert!((out[1] - 4.0).abs() < 1e-5);
        assert!((out[2] - 6.0).abs() < 1e-5);
        assert!((out[3] - 8.0).abs() < 1e-5);
    }

    #[test]
    fn test_gpu_conv2d_3x3_with_padding() {
        // 1 channel, 3x3 input, 3x3 kernel, stride 1, pad 1
        // Output should be 3x3
        let mut gpu = Gpu::new();
        let mut ram = make_ram();

        // Input: all 1s (3x3)
        let input_addr = RAM_BASE + 0x100;
        // Filter: all 1s (1x1x3x3 = 9)
        let filter_addr = RAM_BASE + 0x130;
        // Output: 3x3
        let output_addr = RAM_BASE + 0x170;
        let desc_in = RAM_BASE + 0x200;
        let desc_f = RAM_BASE + 0x220;
        let desc_out = RAM_BASE + 0x240;

        write_f32_array(&mut ram, input_addr as u64, &[1.0; 9]);
        write_f32_array(&mut ram, filter_addr as u64, &[1.0; 9]);

        write_tensor_desc(&mut ram, desc_in as u64, input_addr, 9, [1, 3, 3]);
        write_tensor_desc(&mut ram, desc_f as u64, filter_addr, 9, [1, 3, 3]);
        write_tensor_desc(&mut ram, desc_out as u64, output_addr, 9, [1, 3, 3]);

        write_u32(&mut gpu, regs::KERNEL_TYPE, KernelType::Conv2d as u32);
        write_u32(&mut gpu, regs::INPUT0_DESC_LOW, desc_in);
        write_u32(&mut gpu, regs::INPUT1_DESC_LOW, desc_f);
        write_u32(&mut gpu, regs::OUTPUT0_DESC_LOW, desc_out);
        write_u32(&mut gpu, regs::CONV_KERNEL_SIZE, (3 << 16) | 3);
        write_u32(&mut gpu, regs::CONV_STRIDE, (1 << 16) | 1);
        write_u32(&mut gpu, regs::CONV_PADDING, (1 << 16) | 1);
        write_u32(&mut gpu, regs::CONV_INPUT_DIMS, (3 << 16) | 3);
        write_u32(&mut gpu, regs::CONV_CHANNELS, (1 << 16) | 1);

        gpu.execute_with_memory(&mut ram);

        let out = read_f32_array(&mut ram, output_addr as u64, 9);
        // With all-1 input and all-1 3x3 kernel with pad=1:
        // corners see 4 values, edges see 6, center sees 9
        assert!(
            (out[4] - 9.0).abs() < 1e-4,
            "center should be 9, got {}",
            out[4]
        );
        // Corners: (0,0) sees (0,0)(0,1)(1,0)(1,1) = 4
        assert!(
            (out[0] - 4.0).abs() < 1e-4,
            "corner should be 4, got {}",
            out[0]
        );
        // Top edge (0,1): sees 6 values
        assert!(
            (out[1] - 6.0).abs() < 1e-4,
            "edge should be 6, got {}",
            out[1]
        );
    }

    #[test]
    fn test_gpu_conv2d_2channels() {
        // 2 input channels, 2x2 input, 1x1 kernel, 1 output channel
        // Output = input[0] * filter[0] + input[1] * filter[1]
        let mut gpu = Gpu::new();
        let mut ram = make_ram();

        // Channel 0: [[1,2],[3,4]], Channel 1: [[5,6],[7,8]]
        let input_addr = RAM_BASE + 0x100;
        // Filter: [out0/in0/1x1=10, out0/in1/1x1=1] → output = ch0*10 + ch1*1
        let filter_addr = RAM_BASE + 0x130;
        let output_addr = RAM_BASE + 0x150;
        let desc_in = RAM_BASE + 0x200;
        let desc_f = RAM_BASE + 0x220;
        let desc_out = RAM_BASE + 0x240;

        write_f32_array(
            &mut ram,
            input_addr as u64,
            &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
        );
        write_f32_array(&mut ram, filter_addr as u64, &[10.0, 1.0]);

        write_tensor_desc(&mut ram, desc_in as u64, input_addr, 8, [2, 2, 2]);
        write_tensor_desc(&mut ram, desc_f as u64, filter_addr, 2, [1, 2, 1]);
        write_tensor_desc(&mut ram, desc_out as u64, output_addr, 4, [1, 2, 2]);

        write_u32(&mut gpu, regs::KERNEL_TYPE, KernelType::Conv2d as u32);
        write_u32(&mut gpu, regs::INPUT0_DESC_LOW, desc_in);
        write_u32(&mut gpu, regs::INPUT1_DESC_LOW, desc_f);
        write_u32(&mut gpu, regs::OUTPUT0_DESC_LOW, desc_out);
        write_u32(&mut gpu, regs::CONV_KERNEL_SIZE, (1 << 16) | 1);
        write_u32(&mut gpu, regs::CONV_STRIDE, (1 << 16) | 1);
        write_u32(&mut gpu, regs::CONV_PADDING, 0);
        write_u32(&mut gpu, regs::CONV_INPUT_DIMS, (2 << 16) | 2);
        write_u32(&mut gpu, regs::CONV_CHANNELS, (2 << 16) | 1); // 2 in, 1 out

        gpu.execute_with_memory(&mut ram);

        // output[0] = 1*10 + 5*1 = 15
        // output[1] = 2*10 + 6*1 = 26
        // output[2] = 3*10 + 7*1 = 37
        // output[3] = 4*10 + 8*1 = 48
        let out = read_f32_array(&mut ram, output_addr as u64, 4);
        assert!((out[0] - 15.0).abs() < 1e-4);
        assert!((out[1] - 26.0).abs() < 1e-4);
        assert!((out[2] - 37.0).abs() < 1e-4);
        assert!((out[3] - 48.0).abs() < 1e-4);
    }

    #[test]
    fn test_gpu_pool2d_max_2x2() {
        let mut gpu = Gpu::new();
        let mut ram = make_ram();

        // Input: 1 channel, 4x4 = [[1,2,3,4],[5,6,7,8],[9,10,11,12],[13,14,15,16]]
        let input_addr = RAM_BASE + 0x100;
        let output_addr = RAM_BASE + 0x140;
        let desc_in = RAM_BASE + 0x200;
        let desc_out = RAM_BASE + 0x220;

        write_f32_array(
            &mut ram,
            input_addr as u64,
            &[
                1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0,
                16.0,
            ],
        );

        write_tensor_desc(&mut ram, desc_in as u64, input_addr, 16, [1, 4, 4]);
        write_tensor_desc(&mut ram, desc_out as u64, output_addr, 4, [1, 2, 2]);

        write_u32(&mut gpu, regs::KERNEL_TYPE, KernelType::Pool2dMax as u32);
        write_u32(&mut gpu, regs::INPUT0_DESC_LOW, desc_in);
        write_u32(&mut gpu, regs::OUTPUT0_DESC_LOW, desc_out);
        write_u32(&mut gpu, regs::CONV_KERNEL_SIZE, (2 << 16) | 2);
        write_u32(&mut gpu, regs::CONV_STRIDE, (2 << 16) | 2);
        write_u32(&mut gpu, regs::CONV_PADDING, 0);
        write_u32(&mut gpu, regs::CONV_INPUT_DIMS, (4 << 16) | 4);
        write_u32(&mut gpu, regs::CONV_CHANNELS, 1 << 16); // 1 channel

        gpu.execute_with_memory(&mut ram);

        // Max pool 2x2 stride 2 on 4x4:
        // [[6,8],[14,16]]
        let out = read_f32_array(&mut ram, output_addr as u64, 4);
        assert!((out[0] - 6.0).abs() < 1e-5, "got {}", out[0]);
        assert!((out[1] - 8.0).abs() < 1e-5, "got {}", out[1]);
        assert!((out[2] - 14.0).abs() < 1e-5, "got {}", out[2]);
        assert!((out[3] - 16.0).abs() < 1e-5, "got {}", out[3]);
    }

    #[test]
    fn test_gpu_pool2d_avg_2x2() {
        let mut gpu = Gpu::new();
        let mut ram = make_ram();

        // Input: 1 channel, 2x2 = [[1,3],[5,7]]
        let input_addr = RAM_BASE + 0x100;
        let output_addr = RAM_BASE + 0x120;
        let desc_in = RAM_BASE + 0x200;
        let desc_out = RAM_BASE + 0x220;

        write_f32_array(&mut ram, input_addr as u64, &[1.0, 3.0, 5.0, 7.0]);

        write_tensor_desc(&mut ram, desc_in as u64, input_addr, 4, [1, 2, 2]);
        write_tensor_desc(&mut ram, desc_out as u64, output_addr, 1, [1, 1, 1]);

        write_u32(&mut gpu, regs::KERNEL_TYPE, KernelType::Pool2dAvg as u32);
        write_u32(&mut gpu, regs::INPUT0_DESC_LOW, desc_in);
        write_u32(&mut gpu, regs::OUTPUT0_DESC_LOW, desc_out);
        write_u32(&mut gpu, regs::CONV_KERNEL_SIZE, (2 << 16) | 2);
        write_u32(&mut gpu, regs::CONV_STRIDE, (2 << 16) | 2);
        write_u32(&mut gpu, regs::CONV_PADDING, 0);
        write_u32(&mut gpu, regs::CONV_INPUT_DIMS, (2 << 16) | 2);
        write_u32(&mut gpu, regs::CONV_CHANNELS, 1 << 16);

        gpu.execute_with_memory(&mut ram);

        // Avg of [1,3,5,7] = 4.0
        let out = read_f32_array(&mut ram, output_addr as u64, 1);
        assert!((out[0] - 4.0).abs() < 1e-5, "got {}", out[0]);
    }

    #[test]
    fn test_gpu_tanh_fp32() {
        let mut gpu = Gpu::new();
        let mut ram = make_ram();

        let a_addr = RAM_BASE + 0x100;
        let c_addr = RAM_BASE + 0x120;
        let desc_a = RAM_BASE + 0x200;
        let desc_c = RAM_BASE + 0x220;

        write_f32_array(&mut ram, a_addr as u64, &[0.0, 1.0, -1.0]);
        write_tensor_desc(&mut ram, desc_a as u64, a_addr, 3, [3, 0, 0]);
        write_tensor_desc(&mut ram, desc_c as u64, c_addr, 3, [3, 0, 0]);

        write_u32(&mut gpu, regs::KERNEL_TYPE, KernelType::Tanh as u32);
        write_u32(&mut gpu, regs::INPUT0_DESC_LOW, desc_a);
        write_u32(&mut gpu, regs::OUTPUT0_DESC_LOW, desc_c);

        gpu.execute_with_memory(&mut ram);

        let c = read_f32_array(&mut ram, c_addr as u64, 3);
        assert!((c[0] - 0.0).abs() < 1e-5);
        assert!((c[1] - 1.0f32.tanh()).abs() < 1e-5);
        assert!((c[2] - (-1.0f32).tanh()).abs() < 1e-5);
    }

    #[test]
    fn test_gpu_leaky_relu_fp32() {
        let mut gpu = Gpu::new();
        let mut ram = make_ram();

        let a_addr = RAM_BASE + 0x100;
        let c_addr = RAM_BASE + 0x120;
        let desc_a = RAM_BASE + 0x200;
        let desc_c = RAM_BASE + 0x220;

        write_f32_array(&mut ram, a_addr as u64, &[-2.0, -1.0, 0.0, 1.0, 2.0]);
        write_tensor_desc(&mut ram, desc_a as u64, a_addr, 5, [5, 0, 0]);
        write_tensor_desc(&mut ram, desc_c as u64, c_addr, 5, [5, 0, 0]);

        write_u32(&mut gpu, regs::KERNEL_TYPE, KernelType::LeakyRelu as u32);
        write_u32(&mut gpu, regs::INPUT0_DESC_LOW, desc_a);
        write_u32(&mut gpu, regs::OUTPUT0_DESC_LOW, desc_c);
        // Default alpha = 0.01 (conv_padding = 0 means use default)

        gpu.execute_with_memory(&mut ram);

        let c = read_f32_array(&mut ram, c_addr as u64, 5);
        assert!((c[0] - (-2.0 * 0.01)).abs() < 1e-5, "got {}", c[0]);
        assert!((c[1] - (-1.0 * 0.01)).abs() < 1e-5);
        assert!((c[2] - 0.0).abs() < 1e-5);
        assert!((c[3] - 1.0).abs() < 1e-5);
        assert!((c[4] - 2.0).abs() < 1e-5);
    }

    #[test]
    fn test_gpu_relu6_fp32() {
        let mut gpu = Gpu::new();
        let mut ram = make_ram();

        let a_addr = RAM_BASE + 0x100;
        let c_addr = RAM_BASE + 0x120;
        let desc_a = RAM_BASE + 0x200;
        let desc_c = RAM_BASE + 0x220;

        write_f32_array(&mut ram, a_addr as u64, &[-1.0, 0.0, 3.0, 6.0, 10.0]);
        write_tensor_desc(&mut ram, desc_a as u64, a_addr, 5, [5, 0, 0]);
        write_tensor_desc(&mut ram, desc_c as u64, c_addr, 5, [5, 0, 0]);

        write_u32(&mut gpu, regs::KERNEL_TYPE, KernelType::Relu6 as u32);
        write_u32(&mut gpu, regs::INPUT0_DESC_LOW, desc_a);
        write_u32(&mut gpu, regs::OUTPUT0_DESC_LOW, desc_c);

        gpu.execute_with_memory(&mut ram);

        let c = read_f32_array(&mut ram, c_addr as u64, 5);
        assert!((c[0] - 0.0).abs() < 1e-5);
        assert!((c[1] - 0.0).abs() < 1e-5);
        assert!((c[2] - 3.0).abs() < 1e-5);
        assert!((c[3] - 6.0).abs() < 1e-5);
        assert!(
            (c[4] - 6.0).abs() < 1e-5,
            "relu6(10) should clamp to 6, got {}",
            c[4]
        );
    }

    #[test]
    fn test_gpu_vector_scale_fp32() {
        let mut gpu = Gpu::new();
        let mut ram = make_ram();

        let a_addr = RAM_BASE + 0x100;
        let scalar_addr = RAM_BASE + 0x120;
        let c_addr = RAM_BASE + 0x130;
        let desc_a = RAM_BASE + 0x200;
        let desc_s = RAM_BASE + 0x220;
        let desc_c = RAM_BASE + 0x240;

        write_f32_array(&mut ram, a_addr as u64, &[1.0, 2.0, 3.0]);
        // scalar = 3.0
        write_f32_array(&mut ram, scalar_addr as u64, &[3.0]);

        write_tensor_desc(&mut ram, desc_a as u64, a_addr, 3, [3, 0, 0]);
        write_tensor_desc(&mut ram, desc_s as u64, scalar_addr, 1, [1, 0, 0]);
        write_tensor_desc(&mut ram, desc_c as u64, c_addr, 3, [3, 0, 0]);

        write_u32(&mut gpu, regs::KERNEL_TYPE, KernelType::VectorScale as u32);
        write_u32(&mut gpu, regs::INPUT0_DESC_LOW, desc_a);
        write_u32(&mut gpu, regs::INPUT1_DESC_LOW, desc_s);
        write_u32(&mut gpu, regs::OUTPUT0_DESC_LOW, desc_c);

        gpu.execute_with_memory(&mut ram);

        let c = read_f32_array(&mut ram, c_addr as u64, 3);
        assert!((c[0] - 3.0).abs() < 1e-5);
        assert!((c[1] - 6.0).abs() < 1e-5);
        assert!((c[2] - 9.0).abs() < 1e-5);
    }

    #[test]
    fn test_gpu_conv2d_stride2() {
        // 1 channel, 4x4 input, 2x2 kernel, stride 2, no padding → 2x2 output
        let mut gpu = Gpu::new();
        let mut ram = make_ram();

        let input_addr = RAM_BASE + 0x100;
        let filter_addr = RAM_BASE + 0x140;
        let output_addr = RAM_BASE + 0x160;
        let desc_in = RAM_BASE + 0x200;
        let desc_f = RAM_BASE + 0x220;
        let desc_out = RAM_BASE + 0x240;

        // Input: [[1,2,3,4],[5,6,7,8],[9,10,11,12],[13,14,15,16]]
        write_f32_array(
            &mut ram,
            input_addr as u64,
            &[
                1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0,
                16.0,
            ],
        );
        // Filter: [[1,0],[0,0]] → picks top-left element only
        write_f32_array(&mut ram, filter_addr as u64, &[1.0, 0.0, 0.0, 0.0]);

        write_tensor_desc(&mut ram, desc_in as u64, input_addr, 16, [1, 4, 4]);
        write_tensor_desc(&mut ram, desc_f as u64, filter_addr, 4, [1, 2, 2]);
        write_tensor_desc(&mut ram, desc_out as u64, output_addr, 4, [1, 2, 2]);

        write_u32(&mut gpu, regs::KERNEL_TYPE, KernelType::Conv2d as u32);
        write_u32(&mut gpu, regs::INPUT0_DESC_LOW, desc_in);
        write_u32(&mut gpu, regs::INPUT1_DESC_LOW, desc_f);
        write_u32(&mut gpu, regs::OUTPUT0_DESC_LOW, desc_out);
        write_u32(&mut gpu, regs::CONV_KERNEL_SIZE, (2 << 16) | 2);
        write_u32(&mut gpu, regs::CONV_STRIDE, (2 << 16) | 2);
        write_u32(&mut gpu, regs::CONV_PADDING, 0);
        write_u32(&mut gpu, regs::CONV_INPUT_DIMS, (4 << 16) | 4);
        write_u32(&mut gpu, regs::CONV_CHANNELS, (1 << 16) | 1);

        gpu.execute_with_memory(&mut ram);

        // With filter [[1,0],[0,0]], stride 2:
        // output[0,0] = input[0,0]*1 = 1
        // output[0,1] = input[0,2]*1 = 3
        // output[1,0] = input[2,0]*1 = 9
        // output[1,1] = input[2,2]*1 = 11
        let out = read_f32_array(&mut ram, output_addr as u64, 4);
        assert!((out[0] - 1.0).abs() < 1e-4, "got {}", out[0]);
        assert!((out[1] - 3.0).abs() < 1e-4, "got {}", out[1]);
        assert!((out[2] - 9.0).abs() < 1e-4, "got {}", out[2]);
        assert!((out[3] - 11.0).abs() < 1e-4, "got {}", out[3]);
    }
}
