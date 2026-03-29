//! Control signal definitions for the pipeline.
//!
//! This module defines control signals used in each pipeline stage.

/// Memory access width constants.
pub mod mem_width {
    /// Byte access (8 bits).
    pub const BYTE: u32 = 1;
    /// Half-word access (16 bits).
    pub const HALF: u32 = 2;
    /// Word access (32 bits).
    pub const WORD: u32 = 4;
}

/// ALU operation types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AluOp {
    /// No operation / pass through.
    #[default]
    Nop,
    /// Addition.
    Add,
    /// Subtraction.
    Sub,
    /// Bitwise AND.
    And,
    /// Bitwise OR.
    Or,
    /// Bitwise XOR.
    Xor,
    /// Shift left logical.
    Sll,
    /// Shift right logical.
    Srl,
    /// Shift right arithmetic.
    Sra,
    /// Set less than (signed).
    Slt,
    /// Set less than (unsigned).
    Sltu,
    /// Load upper immediate (pass immediate).
    Lui,
    /// Pass rs1 value through (for jumps, loads).
    Pass,
}

impl AluOp {
    /// Check if this is a comparison operation.
    pub fn is_comparison(&self) -> bool {
        matches!(self, AluOp::Slt | AluOp::Sltu)
    }
}

/// Source for ALU operand 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AluSrc {
    /// Use value from register file (rs2).
    #[default]
    Register,
    /// Use immediate value.
    Immediate,
}

/// Branch condition types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BranchType {
    /// Not a branch instruction.
    #[default]
    None,
    /// Branch if equal.
    Beq,
    /// Branch if not equal.
    Bne,
    /// Branch if less than (signed).
    Blt,
    /// Branch if greater or equal (signed).
    Bge,
    /// Branch if less than (unsigned).
    Bltu,
    /// Branch if greater or equal (unsigned).
    Bgeu,
}

impl BranchType {
    /// Check if this is a branch instruction.
    pub fn is_branch(&self) -> bool {
        !matches!(self, BranchType::None)
    }
}

/// Control signals for Execute stage (computed in ID, used in EX).
#[derive(Debug, Clone, Copy, Default)]
pub struct ExControlSignals {
    /// ALU operation to perform.
    pub alu_op: AluOp,
    /// Source for ALU operand 2 (register or immediate).
    pub alu_src: AluSrc,
    /// Whether this is a branch instruction.
    pub branch: bool,
    /// Whether this is a jump (JAL/JALR).
    pub jump: bool,
    /// Branch condition type.
    pub branch_type: BranchType,
    /// Whether this instruction writes to a register.
    pub reg_write: bool,
}

impl ExControlSignals {
    /// Create control signals for R-type instructions.
    pub fn r_type(alu_op: AluOp) -> Self {
        Self {
            alu_op,
            alu_src: AluSrc::Register,
            branch: false,
            jump: false,
            branch_type: BranchType::None,
            reg_write: true,
        }
    }

    /// Create control signals for I-type ALU instructions.
    pub fn i_type(alu_op: AluOp) -> Self {
        Self {
            alu_op,
            alu_src: AluSrc::Immediate,
            branch: false,
            jump: false,
            branch_type: BranchType::None,
            reg_write: true,
        }
    }

    /// Create control signals for load instructions.
    pub fn load() -> Self {
        Self {
            alu_op: AluOp::Add, // Address calculation: rs1 + offset
            alu_src: AluSrc::Immediate,
            branch: false,
            jump: false,
            branch_type: BranchType::None,
            reg_write: true,
        }
    }

    /// Create control signals for store instructions.
    pub fn store() -> Self {
        Self {
            alu_op: AluOp::Add, // Address calculation: rs1 + offset
            alu_src: AluSrc::Immediate,
            branch: false,
            jump: false,
            branch_type: BranchType::None,
            reg_write: false,
        }
    }

    /// Create control signals for branch instructions.
    pub fn branch(branch_type: BranchType) -> Self {
        Self {
            alu_op: AluOp::Sub, // For comparison
            alu_src: AluSrc::Register,
            branch: true,
            jump: false,
            branch_type,
            reg_write: false,
        }
    }

    /// Create control signals for JAL instruction.
    pub fn jal() -> Self {
        Self {
            alu_op: AluOp::Pass,
            alu_src: AluSrc::Immediate,
            branch: false,
            jump: true,
            branch_type: BranchType::None,
            reg_write: true,
        }
    }

    /// Create control signals for JALR instruction.
    pub fn jalr() -> Self {
        Self {
            alu_op: AluOp::Add, // Target = rs1 + offset
            alu_src: AluSrc::Immediate,
            branch: false,
            jump: true,
            branch_type: BranchType::None,
            reg_write: true,
        }
    }

    /// Create control signals for LUI instruction.
    pub fn lui() -> Self {
        Self {
            alu_op: AluOp::Lui,
            alu_src: AluSrc::Immediate,
            branch: false,
            jump: false,
            branch_type: BranchType::None,
            reg_write: true,
        }
    }

    /// Create control signals for AUIPC instruction.
    pub fn auipc() -> Self {
        Self {
            alu_op: AluOp::Add,
            alu_src: AluSrc::Immediate,
            branch: false,
            jump: false,
            branch_type: BranchType::None,
            reg_write: true,
        }
    }
}

/// Control signals for Memory stage (computed in ID, passed through EX).
#[derive(Debug, Clone, Copy, Default)]
pub struct MemControlSignals {
    /// Memory read enable.
    pub mem_read: bool,
    /// Memory write enable.
    pub mem_write: bool,
    /// Memory access width (1, 2, or 4 bytes).
    pub mem_width: u32,
    /// Whether to sign-extend load result.
    pub mem_sign_extend: bool,
    /// Whether this instruction writes to a register.
    pub reg_write: bool,
}

impl MemControlSignals {
    /// Create control signals for no memory access (but may still write back to register).
    pub fn none() -> Self {
        Self::default()
    }

    /// Create control signals for ALU instructions that write to register.
    pub fn alu() -> Self {
        Self {
            mem_read: false,
            mem_write: false,
            mem_width: 4,
            mem_sign_extend: false,
            reg_write: true,
        }
    }

    /// Create control signals for load byte.
    pub fn lb() -> Self {
        Self {
            mem_read: true,
            mem_write: false,
            mem_width: 1,
            mem_sign_extend: true,
            reg_write: true,
        }
    }

    /// Create control signals for load halfword.
    pub fn lh() -> Self {
        Self {
            mem_read: true,
            mem_write: false,
            mem_width: 2,
            mem_sign_extend: true,
            reg_write: true,
        }
    }

    /// Create control signals for load word.
    pub fn lw() -> Self {
        Self {
            mem_read: true,
            mem_write: false,
            mem_width: 4,
            mem_sign_extend: false,
            reg_write: true,
        }
    }

    /// Create control signals for load byte unsigned.
    pub fn lbu() -> Self {
        Self {
            mem_read: true,
            mem_write: false,
            mem_width: 1,
            mem_sign_extend: false,
            reg_write: true,
        }
    }

    /// Create control signals for load halfword unsigned.
    pub fn lhu() -> Self {
        Self {
            mem_read: true,
            mem_write: false,
            mem_width: 2,
            mem_sign_extend: false,
            reg_write: true,
        }
    }

    /// Create control signals for store byte.
    pub fn sb() -> Self {
        Self {
            mem_read: false,
            mem_write: true,
            mem_width: 1,
            mem_sign_extend: false,
            reg_write: false,
        }
    }

    /// Create control signals for store halfword.
    pub fn sh() -> Self {
        Self {
            mem_read: false,
            mem_write: true,
            mem_width: 2,
            mem_sign_extend: false,
            reg_write: false,
        }
    }

    /// Create control signals for store word.
    pub fn sw() -> Self {
        Self {
            mem_read: false,
            mem_write: true,
            mem_width: 4,
            mem_sign_extend: false,
            reg_write: false,
        }
    }
}

/// Control signals for Write Back stage.
#[derive(Debug, Clone, Copy, Default)]
pub struct WbControlSignals {
    /// Whether to write to register file.
    pub reg_write: bool,
    /// Source for write data (ALU result or memory data).
    pub mem_to_reg: bool,
}

impl WbControlSignals {
    /// Create control signals for ALU result write-back.
    pub fn alu_result() -> Self {
        Self {
            reg_write: true,
            mem_to_reg: false,
        }
    }

    /// Create control signals for memory data write-back.
    pub fn mem_data() -> Self {
        Self {
            reg_write: true,
            mem_to_reg: true,
        }
    }

    /// Create control signals for no write-back.
    pub fn none() -> Self {
        Self {
            reg_write: false,
            mem_to_reg: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alu_op_default() {
        let op = AluOp::default();
        assert_eq!(op, AluOp::Nop);
    }

    #[test]
    fn test_ex_control_r_type() {
        let ctrl = ExControlSignals::r_type(AluOp::Add);
        assert!(ctrl.reg_write);
        assert_eq!(ctrl.alu_src, AluSrc::Register);
        assert!(!ctrl.branch);
    }

    #[test]
    fn test_ex_control_i_type() {
        let ctrl = ExControlSignals::i_type(AluOp::Add);
        assert!(ctrl.reg_write);
        assert_eq!(ctrl.alu_src, AluSrc::Immediate);
    }

    #[test]
    fn test_mem_control_lw() {
        let ctrl = MemControlSignals::lw();
        assert!(ctrl.mem_read);
        assert!(!ctrl.mem_write);
        assert_eq!(ctrl.mem_width, 4);
    }

    #[test]
    fn test_mem_control_sw() {
        let ctrl = MemControlSignals::sw();
        assert!(!ctrl.mem_read);
        assert!(ctrl.mem_write);
        assert_eq!(ctrl.mem_width, 4);
    }
}
