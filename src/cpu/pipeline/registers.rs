//! Pipeline register definitions.
//!
//! This module defines the pipeline registers that hold data between stages.

use crate::cpu::pipeline::control::{ExControlSignals, MemControlSignals, WbControlSignals};
use crate::types::{Addr, RegIdx, Word};

/// IF/ID Pipeline Register.
///
/// Holds data between Instruction Fetch and Instruction Decode stages.
#[derive(Debug, Clone)]
pub struct IfIdRegister {
    /// PC of the instruction in this stage.
    pub pc: Addr,
    /// Raw instruction word (32 bits).
    pub instruction: u32,
    /// Whether this register contains valid data (for bubble handling).
    pub valid: bool,
}

impl Default for IfIdRegister {
    fn default() -> Self {
        Self {
            pc: Addr::new(0),
            instruction: 0,
            valid: false,
        }
    }
}

impl IfIdRegister {
    /// Create a new IF/ID register with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Flush this register (invalidate for bubble/branch).
    pub fn flush(&mut self) {
        self.valid = false;
        self.instruction = 0;
    }
}

/// ID/EX Pipeline Register.
///
/// Holds data between Instruction Decode and Execute stages.
#[derive(Debug, Clone)]
pub struct IdExRegister {
    /// PC of the instruction.
    pub pc: Addr,
    /// PC + 4 (next sequential PC).
    pub pc_plus_4: Addr,

    // Register values read in ID stage
    /// Value of source register 1.
    pub rs1_val: Word,
    /// Value of source register 2.
    pub rs2_val: Word,

    // Register indices (for hazard detection)
    /// Source register 1 index.
    pub rs1: RegIdx,
    /// Source register 2 index.
    pub rs2: RegIdx,
    /// Destination register index.
    pub rd: RegIdx,

    /// Immediate value (sign-extended).
    pub imm: i32,

    /// Control signals for EX stage.
    pub ctrl: ExControlSignals,
    /// Control signals for MEM stage.
    pub mem_ctrl: MemControlSignals,

    /// Whether this register contains valid data.
    pub valid: bool,
}

impl Default for IdExRegister {
    fn default() -> Self {
        Self {
            pc: Addr::new(0),
            pc_plus_4: Addr::new(4),
            rs1_val: Word::ZERO,
            rs2_val: Word::ZERO,
            rs1: RegIdx::new(0),
            rs2: RegIdx::new(0),
            rd: RegIdx::new(0),
            imm: 0,
            ctrl: ExControlSignals::default(),
            mem_ctrl: MemControlSignals::default(),
            valid: false,
        }
    }
}

impl IdExRegister {
    /// Create a new ID/EX register with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Flush this register (invalidate for bubble/branch).
    pub fn flush(&mut self) {
        self.valid = false;
        self.ctrl.reg_write = false;
        self.mem_ctrl.mem_read = false;
        self.mem_ctrl.mem_write = false;
    }
}

/// EX/MEM Pipeline Register.
///
/// Holds data between Execute and Memory stages.
#[derive(Debug, Clone)]
pub struct ExMemRegister {
    /// PC of the instruction.
    pub pc: Addr,
    /// PC + 4.
    pub pc_plus_4: Addr,

    /// ALU result (address for loads/stores, result for ALU ops).
    pub alu_result: Word,

    /// Value to store (for S-type instructions).
    pub store_data: Word,

    /// Destination register.
    pub rd: RegIdx,

    /// Control signals for MEM and WB stages.
    pub ctrl: MemControlSignals,

    /// Whether a branch was taken.
    pub branch_taken: bool,

    /// Branch/jump target address.
    pub branch_target: Addr,

    /// Whether this register contains valid data.
    pub valid: bool,
}

impl Default for ExMemRegister {
    fn default() -> Self {
        Self {
            pc: Addr::new(0),
            pc_plus_4: Addr::new(4),
            alu_result: Word::ZERO,
            store_data: Word::ZERO,
            rd: RegIdx::new(0),
            ctrl: MemControlSignals::default(),
            branch_taken: false,
            branch_target: Addr::new(0),
            valid: false,
        }
    }
}

impl ExMemRegister {
    /// Create a new EX/MEM register with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Flush this register (invalidate for branch misprediction).
    pub fn flush(&mut self) {
        self.valid = false;
        self.ctrl.reg_write = false;
        self.ctrl.mem_read = false;
        self.ctrl.mem_write = false;
    }
}

/// MEM/WB Pipeline Register.
///
/// Holds data between Memory and Write Back stages.
#[derive(Debug, Clone)]
pub struct MemWbRegister {
    /// PC of the instruction.
    pub pc: Addr,

    /// Value to write back (from ALU or memory).
    pub write_data: Word,

    /// Destination register.
    pub rd: RegIdx,

    /// Control signal for WB.
    pub ctrl: WbControlSignals,

    /// Whether this register contains valid data.
    pub valid: bool,
}

impl Default for MemWbRegister {
    fn default() -> Self {
        Self {
            pc: Addr::new(0),
            write_data: Word::ZERO,
            rd: RegIdx::new(0),
            ctrl: WbControlSignals::default(),
            valid: false,
        }
    }
}

impl MemWbRegister {
    /// Create a new MEM/WB register with default values.
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_if_id_default() {
        let reg = IfIdRegister::default();
        assert!(!reg.valid);
        assert_eq!(reg.pc, Addr::new(0));
    }

    #[test]
    fn test_if_id_flush() {
        let mut reg = IfIdRegister {
            pc: Addr::new(0x1000),
            instruction: 0x12345678,
            valid: true,
        };
        reg.flush();
        assert!(!reg.valid);
        assert_eq!(reg.instruction, 0);
    }

    #[test]
    fn test_id_ex_default() {
        let reg = IdExRegister::default();
        assert!(!reg.valid);
        assert!(!reg.ctrl.reg_write);
    }
}
