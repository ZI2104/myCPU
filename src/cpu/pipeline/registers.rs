//! Pipeline register definitions.
//!
//! This module defines the pipeline registers that hold data between stages,
//! as well as the synchronous RAM latches used in the pre-IF/IF two-beat
//! fetch design and the EX/MEM data path.

use crate::cpu::pipeline::control::{ExControlSignals, MemControlSignals, WbControlSignals};
use crate::types::{Addr, RegIdx, Word};

/// Instruction fetch latch between pre-IF and IF stages.
///
/// Models the synchronous instruction RAM output register. The read address
/// is presented in the pre-IF phase (combinational), and the instruction
/// data becomes available in the IF phase (one cycle later).
#[derive(Debug, Clone)]
pub struct InstrFetchLatch {
    /// PC corresponding to the fetched instruction.
    pub pc: Addr,
    /// Raw instruction word.
    pub instruction: u32,
    /// Whether this latch contains valid data.
    pub valid: bool,
}

impl Default for InstrFetchLatch {
    fn default() -> Self {
        Self {
            pc: Addr::new(0),
            instruction: 0,
            valid: false,
        }
    }
}

impl InstrFetchLatch {
    /// Create a new instruction fetch latch with default values.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Data read latch between EX and MEM stages.
///
/// Models the synchronous data RAM output register. The read address is
/// presented when the instruction is in the EX stage, and the data becomes
/// available when the instruction reaches the MEM stage (one cycle later).
#[derive(Debug, Clone)]
pub struct DataReadLatch {
    /// Physical address used for the read.
    pub paddr: Addr,
    /// Raw data read from memory (always word-aligned).
    pub raw_data: Word,
    /// Access width (1 = byte, 2 = half, 4 = word).
    pub width: u32,
    /// Whether to sign-extend the result.
    pub sign_extend: bool,
    /// Whether this latch contains valid data.
    pub valid: bool,
}

impl Default for DataReadLatch {
    fn default() -> Self {
        Self {
            paddr: Addr::new(0),
            raw_data: Word::ZERO,
            width: 0,
            sign_extend: false,
            valid: false,
        }
    }
}

impl DataReadLatch {
    /// Create a new data read latch with default values.
    pub fn new() -> Self {
        Self::default()
    }
}

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

    /// Whether a branch/jump was resolved as taken in the ID stage.
    pub branch_taken: bool,
    /// Branch/jump target address computed in the ID stage.
    pub branch_target: Addr,

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
            branch_taken: false,
            branch_target: Addr::new(0),
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
        self.branch_taken = false;
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

    #[test]
    fn test_instr_fetch_latch_default() {
        let latch = InstrFetchLatch::default();
        assert!(!latch.valid);
        assert_eq!(latch.pc, Addr::new(0));
        assert_eq!(latch.instruction, 0);
    }

    #[test]
    fn test_data_read_latch_default() {
        let latch = DataReadLatch::default();
        assert!(!latch.valid);
        assert_eq!(latch.paddr, Addr::new(0));
        assert_eq!(latch.raw_data, Word::ZERO);
        assert_eq!(latch.width, 0);
        assert!(!latch.sign_extend);
    }
}
