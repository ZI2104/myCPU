//! LPU MMIO coprocessor (logic processing unit skeleton).

use crate::error::{Result, SimError};
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

        let op = LpuOp::from_u32(self.opcode);
        let shamt = self.op_b & 0x1F;
        self.result = match op {
            LpuOp::And => self.op_a & self.op_b,
            LpuOp::Or => self.op_a | self.op_b,
            LpuOp::Xor => self.op_a ^ self.op_b,
            LpuOp::Shl => self.op_a.wrapping_shl(shamt),
            LpuOp::Shr => self.op_a.wrapping_shr(shamt),
            LpuOp::Sar => ((self.op_a as i32) >> shamt) as u32,
        };

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
}
