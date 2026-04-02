//! NPU MMIO coprocessor (milestone skeleton).
//!
//! The NPU is modeled as a simple memory-mapped accelerator with a tiny command
//! interface to keep integration, visualization, and testing straightforward.

use crate::error::{Result, SimError};
use crate::traits::Memory;
use crate::traits::Peripheral;
use crate::types::Addr;

/// NPU base address.
pub const NPU_BASE: u32 = 0x2000_0000;
/// NPU MMIO region size.
pub const NPU_SIZE: usize = 0x100;

const REG_CONTROL: u32 = 0x00;
const REG_STATUS: u32 = 0x04;
const REG_OP_A: u32 = 0x08;
const REG_OP_B: u32 = 0x0C;
const REG_RESULT: u32 = 0x10;
const REG_OPCODE: u32 = 0x14;
const REG_CYCLES: u32 = 0x18;
const REG_DESC_ADDR_LOW: u32 = 0x20;
const REG_DESC_ADDR_HIGH: u32 = 0x24;
const REG_DESC_LEN: u32 = 0x28;
const REG_DESC_NOTIFY: u32 = 0x2C;
const REG_TASKS_DONE: u32 = 0x30;
const REG_TASKS_ERROR: u32 = 0x34;
const REG_DESC_NOTIFY_COUNT: u32 = 0x38;

const NPU_DESC_STRIDE: u64 = 16;

mod control_bits {
    pub const START: u32 = 1 << 0;
    pub const IRQ_EN: u32 = 1 << 1;
}

mod status_bits {
    pub const BUSY: u32 = 1 << 0;
    pub const DONE: u32 = 1 << 1;
    pub const IRQ_PENDING: u32 = 1 << 2;
}

/// Supported NPU operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NpuOp {
    Add = 0,
    Mul = 1,
    Max = 2,
    Relu = 3,
}

#[derive(Debug, Clone, Copy)]
pub struct NpuSnapshot {
    pub control: u32,
    pub status: u32,
    pub opcode: u32,
    pub cycles: u32,
    pub desc_addr: u64,
    pub desc_len: u32,
    pub tasks_done: u32,
    pub tasks_error: u32,
    pub desc_notify_count: u64,
    pub pending_desc_notify: bool,
}

impl NpuOp {
    fn from_u32(v: u32) -> Self {
        match v {
            0 => Self::Add,
            1 => Self::Mul,
            2 => Self::Max,
            3 => Self::Relu,
            _ => Self::Add,
        }
    }
}

/// Minimal NPU model for MMIO coprocessor integration.
#[derive(Debug, Clone)]
pub struct Npu {
    base: Addr,
    control: u32,
    status: u32,
    op_a: u32,
    op_b: u32,
    result: u32,
    opcode: u32,
    cycles: u32,
    desc_addr: u64,
    desc_len: u32,
    tasks_done: u32,
    tasks_error: u32,
    desc_notify_count: u64,
    pending_desc_notify: bool,
}

impl Default for Npu {
    fn default() -> Self {
        Self::new()
    }
}

impl Npu {
    pub fn new() -> Self {
        Self::with_base(Addr::new(NPU_BASE))
    }

    pub fn with_base(base: Addr) -> Self {
        Self {
            base,
            control: 0,
            status: 0,
            op_a: 0,
            op_b: 0,
            result: 0,
            opcode: 0,
            cycles: 0,
            desc_addr: 0,
            desc_len: 0,
            tasks_done: 0,
            tasks_error: 0,
            desc_notify_count: 0,
            pending_desc_notify: false,
        }
    }

    pub fn snapshot(&self) -> NpuSnapshot {
        NpuSnapshot {
            control: self.control,
            status: self.status,
            opcode: self.opcode,
            cycles: self.cycles,
            desc_addr: self.desc_addr,
            desc_len: self.desc_len,
            tasks_done: self.tasks_done,
            tasks_error: self.tasks_error,
            desc_notify_count: self.desc_notify_count,
            pending_desc_notify: self.pending_desc_notify,
        }
    }

    fn read_reg(&self, reg: u32) -> u32 {
        match reg {
            REG_CONTROL => self.control,
            REG_STATUS => self.status,
            REG_OP_A => self.op_a,
            REG_OP_B => self.op_b,
            REG_RESULT => self.result,
            REG_OPCODE => self.opcode,
            REG_CYCLES => self.cycles,
            REG_DESC_ADDR_LOW => self.desc_addr as u32,
            REG_DESC_ADDR_HIGH => (self.desc_addr >> 32) as u32,
            REG_DESC_LEN => self.desc_len,
            REG_TASKS_DONE => self.tasks_done,
            REG_TASKS_ERROR => self.tasks_error,
            REG_DESC_NOTIFY_COUNT => self.desc_notify_count as u32,
            _ => 0,
        }
    }

    fn write_reg(&mut self, reg: u32, value: u32) {
        match reg {
            REG_CONTROL => {
                self.control = value;
                if (self.control & control_bits::START) != 0 {
                    self.execute_once();
                    self.control &= !control_bits::START;
                }
            }
            REG_STATUS => {
                // W1C for DONE/IRQ_PENDING
                if (value & status_bits::DONE) != 0 {
                    self.status &= !status_bits::DONE;
                }
                if (value & status_bits::IRQ_PENDING) != 0 {
                    self.status &= !status_bits::IRQ_PENDING;
                }
            }
            REG_OP_A => self.op_a = value,
            REG_OP_B => self.op_b = value,
            REG_RESULT => self.result = value,
            REG_OPCODE => self.opcode = value,
            REG_CYCLES => self.cycles = value,
            REG_DESC_ADDR_LOW => {
                self.desc_addr = (self.desc_addr & 0xFFFF_FFFF_0000_0000) | value as u64;
            }
            REG_DESC_ADDR_HIGH => {
                self.desc_addr = (self.desc_addr & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
            }
            REG_DESC_LEN => self.desc_len = value,
            REG_DESC_NOTIFY => {
                if value != 0 {
                    self.pending_desc_notify = true;
                    self.desc_notify_count = self.desc_notify_count.wrapping_add(1);
                }
            }
            REG_TASKS_DONE => self.tasks_done = value,
            REG_TASKS_ERROR => self.tasks_error = value,
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

        if reg == REG_DESC_NOTIFY {
            if shift == 0 {
                self.write_reg(reg, value as u32);
            }
            return;
        }

        let mut current = self.read_reg(reg);
        current &= !(0xFF << shift);
        current |= (value as u32) << shift;
        self.write_reg(reg, current);
    }

    fn compute_result(&self, op: NpuOp) -> u32 {
        match op {
            NpuOp::Add => self.op_a.wrapping_add(self.op_b),
            NpuOp::Mul => self.op_a.wrapping_mul(self.op_b),
            NpuOp::Max => self.op_a.max(self.op_b),
            NpuOp::Relu => {
                if (self.op_a as i32) < 0 {
                    0
                } else {
                    self.op_a
                }
            }
        }
    }

    fn guest_addr(raw: u64) -> Result<Addr> {
        if raw > u32::MAX as u64 {
            return Err(SimError::Peripheral(
                "npu guest address out of rv32 range".to_string(),
            ));
        }
        Ok(Addr::new(raw as u32))
    }

    fn read_guest_u8(
        ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
        addr: u64,
    ) -> Result<u8> {
        let addr = Self::guest_addr(addr)?;
        let target = addr.raw() as usize;

        for (base, size, memory) in ram_regions.iter_mut() {
            let base_addr = base.raw() as usize;
            if target >= base_addr && target < base_addr + *size {
                let relative = Addr::new((target - base_addr) as u32);
                return memory.read_byte(relative).map(|b| b.raw());
            }
        }

        Err(SimError::MemoryOutOfBounds { addr, size: 1 })
    }

    fn write_guest_u8(
        ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
        addr: u64,
        value: u8,
    ) -> Result<()> {
        let addr = Self::guest_addr(addr)?;
        let target = addr.raw() as usize;

        for (base, size, memory) in ram_regions.iter_mut() {
            let base_addr = base.raw() as usize;
            if target >= base_addr && target < base_addr + *size {
                let relative = Addr::new((target - base_addr) as u32);
                return memory.write_byte(relative, crate::types::Byte::new(value));
            }
        }

        Err(SimError::MemoryOutOfBounds { addr, size: 1 })
    }

    fn read_guest_u32(
        ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
        addr: u64,
    ) -> Result<u32> {
        let mut value = 0u32;
        for i in 0..4 {
            value |= (Self::read_guest_u8(ram_regions, addr + i)? as u32) << (i * 8);
        }
        Ok(value)
    }

    fn write_guest_u32(
        ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
        addr: u64,
        value: u32,
    ) -> Result<()> {
        for i in 0..4 {
            Self::write_guest_u8(ram_regions, addr + i, ((value >> (i * 8)) & 0xFF) as u8)?;
        }
        Ok(())
    }

    fn execute_descriptor_entry(
        &mut self,
        ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
        index: u32,
    ) -> Result<()> {
        let desc_base = self.desc_addr + (index as u64) * NPU_DESC_STRIDE;
        self.opcode = Self::read_guest_u32(ram_regions, desc_base)?;

        let op_a_addr = Self::read_guest_u32(ram_regions, desc_base + 4)? as u64;
        let op_b_addr = Self::read_guest_u32(ram_regions, desc_base + 8)? as u64;
        let result_addr = Self::read_guest_u32(ram_regions, desc_base + 12)? as u64;

        self.op_a = Self::read_guest_u32(ram_regions, op_a_addr)?;
        self.op_b = Self::read_guest_u32(ram_regions, op_b_addr)?;

        let op = NpuOp::from_u32(self.opcode);
        self.result = self.compute_result(op);
        self.cycles = self.cycles.wrapping_add(1);
        self.tasks_done = self.tasks_done.wrapping_add(1);

        Self::write_guest_u32(ram_regions, result_addr, self.result)
    }

    pub fn has_pending_descriptor_notify(&self) -> bool {
        self.pending_desc_notify
    }

    pub fn process_pending_descriptor_notify(
        &mut self,
        ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
    ) -> Result<()> {
        if !self.pending_desc_notify {
            return Ok(());
        }

        self.pending_desc_notify = false;
        self.status |= status_bits::BUSY;
        self.status &= !status_bits::DONE;

        for index in 0..self.desc_len {
            if self.execute_descriptor_entry(ram_regions, index).is_err() {
                self.tasks_error = self.tasks_error.wrapping_add(1);
            }
        }

        self.status &= !status_bits::BUSY;
        self.status |= status_bits::DONE;

        if (self.control & control_bits::IRQ_EN) != 0 {
            self.status |= status_bits::IRQ_PENDING;
        }

        Ok(())
    }

    fn execute_once(&mut self) {
        self.status |= status_bits::BUSY;
        self.status &= !status_bits::DONE;

        let op = NpuOp::from_u32(self.opcode);
        self.result = self.compute_result(op);

        self.cycles = self.cycles.wrapping_add(1);
        self.status &= !status_bits::BUSY;
        self.status |= status_bits::DONE;

        if (self.control & control_bits::IRQ_EN) != 0 {
            self.status |= status_bits::IRQ_PENDING;
        }
    }
}

impl Peripheral for Npu {
    fn read(&self, offset: Addr) -> Result<u8> {
        let off = offset.raw();
        if off >= NPU_SIZE as u32 {
            return Err(SimError::InvalidAddress(offset));
        }
        Ok(self.read_u8(off))
    }

    fn write(&mut self, offset: Addr, value: u8) -> Result<()> {
        let off = offset.raw();
        if off >= NPU_SIZE as u32 {
            return Err(SimError::InvalidAddress(offset));
        }
        self.write_u8(off, value);
        Ok(())
    }

    fn base_addr(&self) -> Addr {
        self.base
    }

    fn size(&self) -> usize {
        NPU_SIZE
    }

    fn name(&self) -> &str {
        "NPU"
    }

    fn has_interrupt(&self) -> bool {
        (self.status & status_bits::IRQ_PENDING) != 0
    }

    fn acknowledge_interrupt(&mut self) {
        self.status &= !status_bits::IRQ_PENDING;
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_u32(npu: &mut Npu, reg: u32, value: u32) {
        for i in 0..4 {
            npu.write(Addr::new(reg + i), ((value >> (i * 8)) & 0xFF) as u8)
                .unwrap();
        }
    }

    fn read_u32(npu: &Npu, reg: u32) -> u32 {
        let mut value = 0u32;
        for i in 0..4 {
            value |= (npu.read(Addr::new(reg + i)).unwrap() as u32) << (i * 8);
        }
        value
    }

    #[test]
    fn test_npu_add_op() {
        let mut npu = Npu::new();
        write_u32(&mut npu, REG_OP_A, 10);
        write_u32(&mut npu, REG_OP_B, 32);
        write_u32(&mut npu, REG_OPCODE, NpuOp::Add as u32);
        write_u32(&mut npu, REG_CONTROL, control_bits::START);

        assert_eq!(read_u32(&npu, REG_RESULT), 42);
        assert_ne!(read_u32(&npu, REG_STATUS) & status_bits::DONE, 0);
    }

    #[test]
    fn test_npu_mul_op() {
        let mut npu = Npu::new();
        write_u32(&mut npu, REG_OP_A, 7);
        write_u32(&mut npu, REG_OP_B, 9);
        write_u32(&mut npu, REG_OPCODE, NpuOp::Mul as u32);
        write_u32(&mut npu, REG_CONTROL, control_bits::START);

        assert_eq!(read_u32(&npu, REG_RESULT), 63);
        assert_eq!(read_u32(&npu, REG_CYCLES), 1);
    }

    #[test]
    fn test_npu_interrupt_ack() {
        let mut npu = Npu::new();
        write_u32(&mut npu, REG_OP_A, 1);
        write_u32(&mut npu, REG_OP_B, 2);
        write_u32(&mut npu, REG_OPCODE, NpuOp::Add as u32);
        write_u32(
            &mut npu,
            REG_CONTROL,
            control_bits::START | control_bits::IRQ_EN,
        );

        assert!(npu.has_interrupt());
        npu.acknowledge_interrupt();
        assert!(!npu.has_interrupt());
    }

    #[test]
    fn test_npu_descriptor_dma_batch() {
        use crate::memory::Ram;

        const RAM_BASE: u32 = 0x8000_0000;
        let mut regions: Vec<(Addr, usize, Box<dyn Memory>)> =
            vec![(Addr::new(RAM_BASE), 0x4000, Box::new(Ram::new(0x4000)))];

        let mut npu = Npu::new();

        let desc_addr = RAM_BASE + 0x100;
        let op_a0 = RAM_BASE + 0x200;
        let op_b0 = RAM_BASE + 0x204;
        let out0 = RAM_BASE + 0x208;
        let op_a1 = RAM_BASE + 0x20C;
        let op_b1 = RAM_BASE + 0x210;
        let out1 = RAM_BASE + 0x214;

        Npu::write_guest_u32(&mut regions, desc_addr as u64, NpuOp::Add as u32).unwrap();
        Npu::write_guest_u32(&mut regions, (desc_addr + 4) as u64, op_a0).unwrap();
        Npu::write_guest_u32(&mut regions, (desc_addr + 8) as u64, op_b0).unwrap();
        Npu::write_guest_u32(&mut regions, (desc_addr + 12) as u64, out0).unwrap();

        Npu::write_guest_u32(&mut regions, (desc_addr + 16) as u64, NpuOp::Max as u32).unwrap();
        Npu::write_guest_u32(&mut regions, (desc_addr + 20) as u64, op_a1).unwrap();
        Npu::write_guest_u32(&mut regions, (desc_addr + 24) as u64, op_b1).unwrap();
        Npu::write_guest_u32(&mut regions, (desc_addr + 28) as u64, out1).unwrap();

        Npu::write_guest_u32(&mut regions, op_a0 as u64, 10).unwrap();
        Npu::write_guest_u32(&mut regions, op_b0 as u64, 32).unwrap();
        Npu::write_guest_u32(&mut regions, op_a1 as u64, 7).unwrap();
        Npu::write_guest_u32(&mut regions, op_b1 as u64, 9).unwrap();

        write_u32(&mut npu, REG_DESC_ADDR_LOW, desc_addr);
        write_u32(&mut npu, REG_DESC_LEN, 2);
        write_u32(&mut npu, REG_CONTROL, control_bits::IRQ_EN);
        write_u32(&mut npu, REG_DESC_NOTIFY, 1);

        assert!(npu.has_pending_descriptor_notify());
        npu.process_pending_descriptor_notify(&mut regions).unwrap();

        assert_eq!(Npu::read_guest_u32(&mut regions, out0 as u64).unwrap(), 42);
        assert_eq!(Npu::read_guest_u32(&mut regions, out1 as u64).unwrap(), 9);
        assert_eq!(read_u32(&npu, REG_TASKS_DONE), 2);
        assert_eq!(read_u32(&npu, REG_TASKS_ERROR), 0);
        assert_ne!(read_u32(&npu, REG_STATUS) & status_bits::DONE, 0);
        assert!(npu.has_interrupt());
    }
}
