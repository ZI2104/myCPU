//! LPU MMIO coprocessor (logic processing unit skeleton).

use crate::error::{Result, SimError};
use crate::peripheral::dma;
use crate::traits::Peripheral;
use crate::types::Addr;

/// LPU base address.
pub const LPU_BASE: u32 = 0x2000_1000;
/// LPU MMIO region size.
pub const LPU_SIZE: usize = 0x100;

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

const LPU_DESC_STRIDE: u64 = 16;

mod control_bits {
    pub const START: u32 = 1 << 0;
    pub const IRQ_EN: u32 = 1 << 1;
}

mod status_bits {
    pub const BUSY: u32 = 1 << 0;
    pub const DONE: u32 = 1 << 1;
    pub const IRQ_PENDING: u32 = 1 << 2;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LpuOp {
    And = 0,
    Or = 1,
    Xor = 2,
    Shl = 3,
    Shr = 4,
    Sar = 5,
}

#[derive(Debug, Clone, Copy)]
pub struct LpuSnapshot {
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

impl LpuOp {
    fn from_u32(v: u32) -> Self {
        match v {
            0 => Self::And,
            1 => Self::Or,
            2 => Self::Xor,
            3 => Self::Shl,
            4 => Self::Shr,
            5 => Self::Sar,
            _ => Self::And,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Lpu {
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

impl Default for Lpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Lpu {
    pub fn new() -> Self {
        Self::with_base(Addr::new(LPU_BASE))
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

    pub fn snapshot(&self) -> LpuSnapshot {
        LpuSnapshot {
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

    fn compute_result(&self, op: LpuOp) -> u32 {
        let shamt = self.op_b & 0x1F;
        match op {
            LpuOp::And => self.op_a & self.op_b,
            LpuOp::Or => self.op_a | self.op_b,
            LpuOp::Xor => self.op_a ^ self.op_b,
            LpuOp::Shl => self.op_a.wrapping_shl(shamt),
            LpuOp::Shr => self.op_a.wrapping_shr(shamt),
            LpuOp::Sar => ((self.op_a as i32) >> shamt) as u32,
        }
    }

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

    fn execute_descriptor_entry(
        &mut self,
        ram_regions: &mut dma::RamRegions,
        index: u32,
    ) -> Result<()> {
        let desc_base = self.desc_addr + (index as u64) * LPU_DESC_STRIDE;
        self.opcode = Self::read_guest_u32(ram_regions, desc_base)?;

        let op_a_addr = Self::read_guest_u32(ram_regions, desc_base + 4)? as u64;
        let op_b_addr = Self::read_guest_u32(ram_regions, desc_base + 8)? as u64;
        let result_addr = Self::read_guest_u32(ram_regions, desc_base + 12)? as u64;

        self.op_a = Self::read_guest_u32(ram_regions, op_a_addr)?;
        self.op_b = Self::read_guest_u32(ram_regions, op_b_addr)?;

        let op = LpuOp::from_u32(self.opcode);
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
        ram_regions: &mut dma::RamRegions,
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

        let op = LpuOp::from_u32(self.opcode);
        self.result = self.compute_result(op);

        self.cycles = self.cycles.wrapping_add(1);
        self.status &= !status_bits::BUSY;
        self.status |= status_bits::DONE;

        if (self.control & control_bits::IRQ_EN) != 0 {
            self.status |= status_bits::IRQ_PENDING;
        }
    }
}

impl Peripheral for Lpu {
    fn read(&self, offset: Addr) -> Result<u8> {
        let off = offset.raw();
        if off >= LPU_SIZE as u32 {
            return Err(SimError::InvalidAddress(offset));
        }
        Ok(self.read_u8(off))
    }

    fn write(&mut self, offset: Addr, value: u8) -> Result<()> {
        let off = offset.raw();
        if off >= LPU_SIZE as u32 {
            return Err(SimError::InvalidAddress(offset));
        }
        self.write_u8(off, value);
        Ok(())
    }

    fn base_addr(&self) -> Addr {
        self.base
    }

    fn size(&self) -> usize {
        LPU_SIZE
    }

    fn name(&self) -> &str {
        "LPU"
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

    fn try_execute_pending(
        &mut self,
        ram_regions: &mut dma::RamRegions,
    ) -> Result<bool> {
        if !self.pending_desc_notify {
            return Ok(false);
        }
        self.process_pending_descriptor_notify(ram_regions)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_u32(lpu: &mut Lpu, reg: u32, value: u32) {
        for i in 0..4 {
            lpu.write(Addr::new(reg + i), ((value >> (i * 8)) & 0xFF) as u8)
                .unwrap();
        }
    }

    fn read_u32(lpu: &Lpu, reg: u32) -> u32 {
        let mut value = 0u32;
        for i in 0..4 {
            value |= (lpu.read(Addr::new(reg + i)).unwrap() as u32) << (i * 8);
        }
        value
    }

    #[test]
    fn test_lpu_logic_ops() {
        let mut lpu = Lpu::new();
        write_u32(&mut lpu, REG_OP_A, 0b1010);
        write_u32(&mut lpu, REG_OP_B, 0b1100);
        write_u32(&mut lpu, REG_OPCODE, LpuOp::And as u32);
        write_u32(&mut lpu, REG_CONTROL, control_bits::START);
        assert_eq!(read_u32(&lpu, REG_RESULT), 0b1000);

        write_u32(&mut lpu, REG_OPCODE, LpuOp::Xor as u32);
        write_u32(&mut lpu, REG_CONTROL, control_bits::START);
        assert_eq!(read_u32(&lpu, REG_RESULT), 0b0110);
    }

    #[test]
    fn test_lpu_shift_ops() {
        let mut lpu = Lpu::new();
        write_u32(&mut lpu, REG_OP_A, 0x8000_0000);
        write_u32(&mut lpu, REG_OP_B, 1);
        write_u32(&mut lpu, REG_OPCODE, LpuOp::Sar as u32);
        write_u32(&mut lpu, REG_CONTROL, control_bits::START);

        assert_eq!(read_u32(&lpu, REG_RESULT), 0xC000_0000);
    }

    #[test]
    fn test_lpu_interrupt_ack() {
        let mut lpu = Lpu::new();
        write_u32(&mut lpu, REG_OP_A, 1);
        write_u32(&mut lpu, REG_OP_B, 1);
        write_u32(&mut lpu, REG_OPCODE, LpuOp::Or as u32);
        write_u32(
            &mut lpu,
            REG_CONTROL,
            control_bits::START | control_bits::IRQ_EN,
        );

        assert!(lpu.has_interrupt());
        lpu.acknowledge_interrupt();
        assert!(!lpu.has_interrupt());
    }

    #[test]
    fn test_lpu_descriptor_dma_batch() {
        use crate::memory::Ram;

        const RAM_BASE: u32 = 0x8000_0000;
        let mut regions: dma::RamRegions =
            vec![(Addr::new(RAM_BASE), 0x4000, Box::new(Ram::new(0x4000)))];

        let mut lpu = Lpu::new();

        let desc_addr = RAM_BASE + 0x300;
        let op_a0 = RAM_BASE + 0x400;
        let op_b0 = RAM_BASE + 0x404;
        let out0 = RAM_BASE + 0x408;
        let op_a1 = RAM_BASE + 0x40C;
        let op_b1 = RAM_BASE + 0x410;
        let out1 = RAM_BASE + 0x414;

        Lpu::write_guest_u32(&mut regions, desc_addr as u64, LpuOp::Xor as u32).unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 4) as u64, op_a0).unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 8) as u64, op_b0).unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 12) as u64, out0).unwrap();

        Lpu::write_guest_u32(&mut regions, (desc_addr + 16) as u64, LpuOp::Shl as u32).unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 20) as u64, op_a1).unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 24) as u64, op_b1).unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 28) as u64, out1).unwrap();

        Lpu::write_guest_u32(&mut regions, op_a0 as u64, 0b1010).unwrap();
        Lpu::write_guest_u32(&mut regions, op_b0 as u64, 0b1100).unwrap();
        Lpu::write_guest_u32(&mut regions, op_a1 as u64, 3).unwrap();
        Lpu::write_guest_u32(&mut regions, op_b1 as u64, 4).unwrap();

        write_u32(&mut lpu, REG_DESC_ADDR_LOW, desc_addr);
        write_u32(&mut lpu, REG_DESC_LEN, 2);
        write_u32(&mut lpu, REG_CONTROL, control_bits::IRQ_EN);
        write_u32(&mut lpu, REG_DESC_NOTIFY, 1);

        assert!(lpu.has_pending_descriptor_notify());
        lpu.process_pending_descriptor_notify(&mut regions).unwrap();

        assert_eq!(
            Lpu::read_guest_u32(&mut regions, out0 as u64).unwrap(),
            0b0110
        );
        assert_eq!(Lpu::read_guest_u32(&mut regions, out1 as u64).unwrap(), 48);
        assert_eq!(read_u32(&lpu, REG_TASKS_DONE), 2);
        assert_eq!(read_u32(&lpu, REG_TASKS_ERROR), 0);
        assert_ne!(read_u32(&lpu, REG_STATUS) & status_bits::DONE, 0);
        assert!(lpu.has_interrupt());
    }
}
