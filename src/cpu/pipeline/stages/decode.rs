//! Instruction Decode (ID) Stage.
//!
//! This module implements the ID stage of the pipeline, including
//! early branch/jump resolution (branch decision made in ID stage
//! to reduce control hazard penalty from 3 to 1 cycle).

use crate::cpu::csr::CsrOp;
use crate::cpu::pipeline::control::{BranchType, ExControlSignals, MemControlSignals};
use crate::cpu::pipeline::forward::ForwardUnit;
use crate::cpu::pipeline::registers::{ExMemRegister, IdExRegister, IfIdRegister, MemWbRegister};
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
    /// Decodes the instruction, reads registers, applies forwarding for
    /// branch/jump resolution, and produces the ID/EX pipeline register.
    ///
    /// # Arguments
    /// * `if_id` - IF/ID pipeline register input
    /// * `regs` - Register file
    /// * `ex_mem` - EX/MEM pipeline register (for forwarding to ID)
    /// * `mem_wb` - MEM/WB pipeline register (for forwarding to ID)
    /// * `flush` - Whether to flush this stage (insert bubble)
    pub fn execute(
        &mut self,
        if_id: &IfIdRegister,
        regs: &Registers,
        ex_mem: &ExMemRegister,
        mem_wb: &MemWbRegister,
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
                branch_taken: false,
                branch_target: Addr::new(0),
                prediction: None,
                id_forward_rs1: crate::cpu::pipeline::forward::ForwardSource::None,
                id_forward_rs2: crate::cpu::pipeline::forward::ForwardSource::None,
                valid: false,
            });
        }

        // Decode the instruction
        let decoded = Decoder::decode(if_id.instruction, if_id.pc)?;

        // Extract register indices using Decoder helper
        let rs1 = Decoder::rs1(if_id.instruction);
        let rs2 = Decoder::rs2(if_id.instruction);
        let rd = Decoder::rd(if_id.instruction);

        // Capture ID-stage forwarding sources for visualization.
        let id_forward_rs1 = ForwardUnit::forward_source_for_decode(rs1, ex_mem, mem_wb);
        let id_forward_rs2 = ForwardUnit::forward_source_for_decode(rs2, ex_mem, mem_wb);

        // Read register values from register file
        let rs1_val = regs.read(rs1);
        let rs2_val = regs.read(rs2);

        // Generate control signals
        let (ctrl, mem_ctrl) = Self::generate_control_signals(&decoded, if_id.instruction);

        // Debug output
        #[cfg(test)]
        {
            println!(
                "    DecodeStage: instr={:08x}, decoded={:?}, funct3={}, ctrl.alu_op={:?}",
                if_id.instruction,
                std::mem::discriminant(&decoded),
                match &decoded {
                    DecodedInstr::I(i) => i.funct3,
                    DecodedInstr::Load(i) => i.funct3,
                    _ => 0,
                },
                ctrl.alu_op
            );
        }

        // Extract immediate
        let imm = Self::extract_immediate(&decoded, if_id.instruction);

        // Resolve branch/jump in ID stage for early redirect.
        // Apply forwarding from ex_mem (non-load) and mem_wb to get correct operand values.
        let (branch_taken, branch_target) = if ctrl.trap_return {
            // trap_return targets come from CSR (mepc/sepc), resolved in mod.rs
            (false, Addr::new(0))
        } else if ctrl.branch {
            // Conditional branch: forward operands, evaluate condition
            let (fwd_rs1, fwd_rs2) = ForwardUnit::apply_forwarding_for_decode(
                rs1, rs2, rs1_val, rs2_val, ex_mem, mem_wb,
            );
            let taken = Self::evaluate_branch(ctrl.branch_type, fwd_rs1, fwd_rs2);
            let target = if_id.pc + Addr::new(imm as u32);
            (taken, target)
        } else if ctrl.jump {
            // Distinguish JAL vs JALR by decoded instruction type
            if matches!(decoded, DecodedInstr::Jalr(_)) {
                // JALR: target = (rs1 + imm) & ~1
                let (fwd_rs1, _) = ForwardUnit::apply_forwarding_for_decode(
                    rs1,
                    RegIdx::new(0),
                    rs1_val,
                    rs2_val,
                    ex_mem,
                    mem_wb,
                );
                let target = Addr::new(fwd_rs1.raw().wrapping_add(imm as u32) & !1);
                (true, target)
            } else {
                // JAL: target = PC + imm
                let target = if_id.pc + Addr::new(imm as u32);
                (true, target)
            }
        } else {
            (false, Addr::new(0))
        };

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
            branch_taken,
            branch_target,
            prediction: None,
            id_forward_rs1,
            id_forward_rs2,
            valid: true,
        })
    }

    /// Evaluate branch condition.
    pub fn evaluate_branch(branch_type: BranchType, rs1: Word, rs2: Word) -> bool {
        match branch_type {
            BranchType::Beq => rs1.raw() == rs2.raw(),
            BranchType::Bne => rs1.raw() != rs2.raw(),
            BranchType::Blt => rs1.as_signed() < rs2.as_signed(),
            BranchType::Bge => rs1.as_signed() >= rs2.as_signed(),
            BranchType::Bltu => rs1.raw() < rs2.raw(),
            BranchType::Bgeu => rs1.raw() >= rs2.raw(),
            BranchType::None => false,
        }
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
                (ExControlSignals::r_type(alu_op), MemControlSignals::alu())
            }
            DecodedInstr::I(i) => {
                let alu_op = Self::get_i_type_alu_op(i.funct3);
                (ExControlSignals::i_type(alu_op), MemControlSignals::alu())
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
                    0 => BranchType::Beq,
                    1 => BranchType::Bne,
                    4 => BranchType::Blt,
                    5 => BranchType::Bge,
                    6 => BranchType::Bltu,
                    7 => BranchType::Bgeu,
                    _ => BranchType::None,
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
            DecodedInstr::System { funct3, imm, .. } => match funct3 {
                0 => {
                    let funct12 = *imm;
                    match funct12 {
                        0x000 | 0x001 => (ExControlSignals::default(), MemControlSignals::none()),
                        0x302 | 0x102 | 0x002 => {
                            (ExControlSignals::trap_return(), MemControlSignals::none())
                        }
                        _ => (ExControlSignals::default(), MemControlSignals::none()),
                    }
                }
                1 | 2 | 3 | 5 | 6 | 7 => {
                    let csr_addr = *imm as u16;
                    let csr_op = match funct3 {
                        1 => CsrOp::ReadWrite,
                        2 => CsrOp::ReadSet,
                        3 => CsrOp::ReadClear,
                        5 => CsrOp::ReadWriteImm,
                        6 => CsrOp::ReadSetImm,
                        7 => CsrOp::ReadClearImm,
                        _ => CsrOp::ReadWrite,
                    };
                    (
                        ExControlSignals::csr(csr_op, csr_addr),
                        MemControlSignals::alu(),
                    )
                }
                _ => (ExControlSignals::default(), MemControlSignals::none()),
            },
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
            5 => AluOp::Srl,
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
        let ex_mem = ExMemRegister::default();
        let mem_wb = MemWbRegister::default();

        let if_id = IfIdRegister {
            pc: Addr::new(0),
            instruction: 0x03208193, // ADDI x3, x1, 50
            valid: true,
            ..Default::default()
        };

        let id_ex = stage
            .execute(&if_id, &regs, &ex_mem, &mem_wb, false)
            .unwrap();

        assert!(id_ex.valid);
        assert_eq!(id_ex.rs1, RegIdx::new(1));
        assert_eq!(id_ex.rs1_val.raw(), 100);
        assert_eq!(id_ex.rd, RegIdx::new(3));
        assert_eq!(id_ex.imm, 50);
        assert!(id_ex.ctrl.reg_write);
        assert!(!id_ex.branch_taken);
    }

    #[test]
    fn test_decode_flush() {
        let mut stage = DecodeStage::new();
        let regs = create_test_regs();
        let ex_mem = ExMemRegister::default();
        let mem_wb = MemWbRegister::default();

        let if_id = IfIdRegister {
            pc: Addr::new(0),
            instruction: 0x03208193,
            valid: true,
            ..Default::default()
        };

        let id_ex = stage
            .execute(&if_id, &regs, &ex_mem, &mem_wb, true)
            .unwrap();
        assert!(!id_ex.valid);
        assert!(!id_ex.ctrl.reg_write);
    }

    #[test]
    fn test_decode_branch() {
        let mut stage = DecodeStage::new();
        let regs = create_test_regs();
        let ex_mem = ExMemRegister::default();
        let mem_wb = MemWbRegister::default();

        // BEQ x1, x2, +8  (x1=100, x2=200 → not equal → not taken)
        let if_id = IfIdRegister {
            pc: Addr::new(0),
            instruction: 0x00208463, // BEQ x1, x2, +8
            valid: true,
            ..Default::default()
        };

        let id_ex = stage
            .execute(&if_id, &regs, &ex_mem, &mem_wb, false)
            .unwrap();

        assert!(id_ex.ctrl.branch);
        assert!(!id_ex.ctrl.reg_write);
        assert!(!id_ex.branch_taken); // 100 != 200
    }

    #[test]
    fn test_decode_branch_taken() {
        let mut stage = DecodeStage::new();
        let regs = create_test_regs();
        let ex_mem = ExMemRegister::default();
        let mem_wb = MemWbRegister::default();

        // BEQ x1, x1, +0  (x1=100, x1=100 → equal → taken)
        // B-type: 0000000_00001_00001_000_00000_1100011 = 0x00108063
        let if_id = IfIdRegister {
            pc: Addr::new(0x100),
            instruction: 0x00108063, // BEQ x1, x1, +0
            valid: true,
            ..Default::default()
        };

        let id_ex = stage
            .execute(&if_id, &regs, &ex_mem, &mem_wb, false)
            .unwrap();

        assert!(id_ex.ctrl.branch);
        assert!(id_ex.branch_taken); // x1 == x1
    }

    #[test]
    fn test_decode_jal_target() {
        let mut stage = DecodeStage::new();
        let regs = create_test_regs();
        let ex_mem = ExMemRegister::default();
        let mem_wb = MemWbRegister::default();

        // JAL x1, +8  → target should be PC + 8 = 0x108
        let if_id = IfIdRegister {
            pc: Addr::new(0x100),
            instruction: 0x008000EF, // JAL x1, +8
            valid: true,
            ..Default::default()
        };

        let id_ex = stage
            .execute(&if_id, &regs, &ex_mem, &mem_wb, false)
            .unwrap();

        assert!(id_ex.ctrl.jump);
        assert!(id_ex.branch_taken);
        assert_eq!(id_ex.branch_target, Addr::new(0x108));
    }

    #[test]
    fn test_decode_branch_with_forwarding() {
        let mut stage = DecodeStage::new();
        let regs = create_test_regs(); // x1=100, x2=200
        let ex_mem = ExMemRegister {
            rd: RegIdx::new(2),
            alu_result: Word::new(100), // ex_mem will write 100 to x2
            ctrl: crate::cpu::pipeline::control::MemControlSignals {
                reg_write: true,
                mem_read: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let mem_wb = MemWbRegister::default();

        // BEQ x1, x2, +0 → with forwarding x2=100, x1=100 → equal → taken
        let if_id = IfIdRegister {
            pc: Addr::new(0x100),
            instruction: 0x00008463, // BEQ x1, x1, +0 → but we want BEQ x1, x2
            valid: true,
            ..Default::default()
        };

        // Use BEQ x1, x2 encoding: 0000000_00010_00001_000_00000_1100011
        let if_id = IfIdRegister {
            pc: Addr::new(0x100),
            instruction: 0x00208463, // BEQ x1, x2, +8
            valid: true,
            ..Default::default()
        };

        let id_ex = stage
            .execute(&if_id, &regs, &ex_mem, &mem_wb, false)
            .unwrap();

        assert!(id_ex.ctrl.branch);
        // With forwarding: x2 forwarded from ex_mem = 100, x1 = 100 → taken!
        assert!(id_ex.branch_taken);
    }
}
