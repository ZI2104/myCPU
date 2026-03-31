//! NPU MMIO coprocessor (milestone skeleton).
//!
//! The NPU is modeled as a simple memory-mapped accelerator with a tiny command
//! interface to keep integration, visualization, and testing straightforward.

use crate::error::{Result, SimError};
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

    fn execute_once(&mut self) {
        self.status |= status_bits::BUSY;
        self.status &= !status_bits::DONE;

        let op = NpuOp::from_u32(self.opcode);
        self.result = match op {
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
        };

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
}
