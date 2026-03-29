//! Instruction Decode (ID) Stage.
//!
//! This module implements the ID stage of the pipeline.

use crate::cpu::pipeline::control::{ExControlSignals, MemControlSignals};
use crate::cpu::pipeline::registers::{IdExRegister, IfIdRegister};
use crate::cpu::Registers;
use crate::error::Result;
use crate::instruction::{opcode::opcode, DecodedInstr, Decoder};
use crate::types::{Addr, RegIdx, Word};

/// Instruction Decode stage.
#[derive(Debug, Clone, Default)]
pub struct DecodeStage;

impl DecodeStage {
    /// Create a new decode stage.
    pub const fn new() -> Self {
        Self
    }

    /// Execute the ID stage.
    ///
    /// # Arguments
    /// * `if_id` - IF/ID pipeline register input
    /// * `regs` - Register file
    /// * `flush` - Whether to flush this stage (insert bubble)
    ///
    /// # Returns
    /// The ID/EX pipeline register output.
    pub fn execute(
        &mut self,
        if_id: &IfIdRegister,
        regs: &Registers,
        flush: bool,
    ) -> Result<IdExRegister> {
        // Handle flush (insert bubble)
        if flush || !if_id.valid {
            return Ok(IdExRegister {
                pc: if_id.pc,
                pc_plus_4: if_id.pc + Addr::new(4),
                rs1_val: Word::ZERO,
                rs2_val: Word::ZERO,
                rs1: RegIdx::new(0),
                rs2: RegIdx::new(0),
                rd: RegIdx::new(0),
                imm: 0,
                ctrl: ExControlSignals::default(),
                mem_ctrl: MemControlSignals::default(),
                valid: false,
            });
        }

        // Decode the instruction
        let decoded = Decoder::decode(if_id.instruction, if_id.pc)?;

        // Extract register indices using Decoder helper
        let rs1 = Decoder::rs1(if_id.instruction);
        let rs2 = Decoder::rs2(if_id.instruction);
        let rd = Decoder::rd(if_id.instruction);

        // Read register values
        let rs1_val = regs.read(rs1);
        let rs2_val = regs.read(rs2);

        // Generate control signals
        let (ctrl, mem_ctrl) = Self::generate_control_signals(&decoded, if_id.instruction);

        // Debug output
        #[cfg(test)]
        {
            println!("    DecodeStage: instr={:08x}, decoded={:?}, funct3={}, ctrl.alu_op={:?}",
                if_id.instruction, std::mem::discriminant(&decoded),
                match &decoded {
                    DecodedInstr::I(i) => i.funct3,
                    DecodedInstr::Load(i) => i.funct3,
                    _ => 0,
                },
                ctrl.alu_op);
        }

        // Extract immediate
        let imm = Self::extract_immediate(&decoded, if_id.instruction);

        Ok(IdExRegister {
            pc: if_id.pc,
            pc_plus_4: if_id.pc + Addr::new(4),
            rs1_val,
            rs2_val,
            rs1,
            rs2,
            rd,
            imm,
            ctrl,
            mem_ctrl,
            valid: true,
        })
    }

    /// Extract immediate value based on instruction type.
    fn extract_immediate(decoded: &DecodedInstr, _instr: u32) -> i32 {
        match decoded {
            DecodedInstr::I(i) => i.imm,
            DecodedInstr::S(s) => s.imm,
            DecodedInstr::B(b) => b.imm,
            DecodedInstr::U(u) => u.imm as i32,
            DecodedInstr::J(j) => j.imm,
            DecodedInstr::Load(i) => i.imm,
            DecodedInstr::Jalr(i) => i.imm,
            DecodedInstr::R(_) | DecodedInstr::System { .. } => 0,
        }
    }

    /// Generate control signals based on decoded instruction.
    fn generate_control_signals(
        decoded: &DecodedInstr,
        instr: u32,
    ) -> (ExControlSignals, MemControlSignals) {
        let opcode_val = (instr & 0x7F) as u8;

        match decoded {
            DecodedInstr::R(r) => {
                let alu_op = Self::get_r_type_alu_op(r.funct3, r.funct7);
                (
                    ExControlSignals::r_type(alu_op),
                    MemControlSignals::alu(), // R-type instructions write to register
                )
            }
            DecodedInstr::I(i) => {
                let alu_op = Self::get_i_type_alu_op(i.funct3);
                (
                    ExControlSignals::i_type(alu_op),
                    MemControlSignals::alu(), // ALU instructions still write to register
                )
            }
            DecodedInstr::Load(i) => {
                let mem_ctrl = match i.funct3 {
                    0 => MemControlSignals::lb(),
                    1 => MemControlSignals::lh(),
                    2 => MemControlSignals::lw(),
                    4 => MemControlSignals::lbu(),
                    5 => MemControlSignals::lhu(),
                    _ => MemControlSignals::none(),
                };
                (ExControlSignals::load(), mem_ctrl)
            }
            DecodedInstr::S(s) => {
                let mem_ctrl = match s.funct3 {
                    0 => MemControlSignals::sb(),
                    1 => MemControlSignals::sh(),
                    2 => MemControlSignals::sw(),
                    _ => MemControlSignals::none(),
                };
                (ExControlSignals::store(), mem_ctrl)
            }
            DecodedInstr::B(b) => {
                let branch_type = match b.funct3 {
                    0 => crate::cpu::pipeline::control::BranchType::Beq,
                    1 => crate::cpu::pipeline::control::BranchType::Bne,
                    4 => crate::cpu::pipeline::control::BranchType::Blt,
                    5 => crate::cpu::pipeline::control::BranchType::Bge,
                    6 => crate::cpu::pipeline::control::BranchType::Bltu,
                    7 => crate::cpu::pipeline::control::BranchType::Bgeu,
                    _ => crate::cpu::pipeline::control::BranchType::None,
                };
                (
                    ExControlSignals::branch(branch_type),
                    MemControlSignals::none(),
                )
            }
            DecodedInstr::U(_u) => {
                if opcode_val == opcode::LUI {
                    (ExControlSignals::lui(), MemControlSignals::alu())
                } else {
                    (ExControlSignals::auipc(), MemControlSignals::alu())
                }
            }
            DecodedInstr::J(_) => (ExControlSignals::jal(), MemControlSignals::alu()),
            DecodedInstr::Jalr(_) => (ExControlSignals::jalr(), MemControlSignals::alu()),
            DecodedInstr::System { .. } => (
                ExControlSignals::default(),
                MemControlSignals::none(),
            ),
        }
    }

    /// Get ALU operation for R-type instructions.
    fn get_r_type_alu_op(funct3: u8, funct7: u8) -> crate::cpu::pipeline::control::AluOp {
        use crate::cpu::pipeline::control::AluOp;
        match (funct3, funct7) {
            (0, 0) => AluOp::Add,
            (0, 0x20) => AluOp::Sub,
            (1, _) => AluOp::Sll,
            (2, _) => AluOp::Slt,
            (3, _) => AluOp::Sltu,
            (4, _) => AluOp::Xor,
            (5, 0) => AluOp::Srl,
            (5, 0x20) => AluOp::Sra,
            (6, _) => AluOp::Or,
            (7, _) => AluOp::And,
            _ => AluOp::Nop,
        }
    }

    /// Get ALU operation for I-type ALU instructions.
    fn get_i_type_alu_op(funct3: u8) -> crate::cpu::pipeline::control::AluOp {
        use crate::cpu::pipeline::control::AluOp;
        match funct3 {
            0 => AluOp::Add,
            1 => AluOp::Sll,
            2 => AluOp::Slt,
            3 => AluOp::Sltu,
            4 => AluOp::Xor,
            5 => AluOp::Srl, // SRAI handled by checking bit 10 in execute stage
            6 => AluOp::Or,
            7 => AluOp::And,
            _ => AluOp::Nop,
        }
    }

    /// Reset the decode stage.
    pub fn reset(&mut self) {
        *self = Self;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_regs() -> Registers {
        let mut regs = Registers::new();
        regs.write(RegIdx::new(1), Word::new(100));
        regs.write(RegIdx::new(2), Word::new(200));
        regs
    }

    #[test]
    fn test_decode_addi() {
        let mut stage = DecodeStage::new();
        let regs = create_test_regs();

        // ADDI x3, x1, 50 (x3 = x1 + 50)
        let if_id = IfIdRegister {
            pc: Addr::new(0),
            instruction: 0x03208193, // ADDI x3, x1, 50
            valid: true,
        };

        let id_ex = stage.execute(&if_id, &regs, false).unwrap();

        assert!(id_ex.valid);
        assert_eq!(id_ex.rs1, RegIdx::new(1));
        assert_eq!(id_ex.rs1_val.raw(), 100);
        assert_eq!(id_ex.rd, RegIdx::new(3));
        assert_eq!(id_ex.imm, 50);
        assert!(id_ex.ctrl.reg_write);
    }

    #[test]
    fn test_decode_flush() {
        let mut stage = DecodeStage::new();
        let regs = create_test_regs();

        let if_id = IfIdRegister {
            pc: Addr::new(0),
            instruction: 0x03208193,
            valid: true,
        };

        let id_ex = stage.execute(&if_id, &regs, true).unwrap();
        assert!(!id_ex.valid);
        assert!(!id_ex.ctrl.reg_write);
    }

    #[test]
    fn test_decode_branch() {
        let mut stage = DecodeStage::new();
        let regs = create_test_regs();

        // BEQ x1, x2, +8
        let if_id = IfIdRegister {
            pc: Addr::new(0),
            instruction: 0x00208463, // BEQ x1, x2, +8
            valid: true,
        };

        let id_ex = stage.execute(&if_id, &regs, false).unwrap();

        assert!(id_ex.ctrl.branch);
        assert!(!id_ex.ctrl.reg_write);
    }
}
