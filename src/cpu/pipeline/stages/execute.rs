//! Execute (EX) Stage.
//!
//! This module implements the EX stage of the pipeline.

use crate::cpu::pipeline::control::{AluOp, BranchType, MemControlSignals};
use crate::cpu::pipeline::forward::ForwardUnit;
use crate::cpu::pipeline::registers::{ExMemRegister, IdExRegister, MemWbRegister};
use crate::error::Result;
use crate::types::{Addr, RegIdx, Word};

/// Execute stage.
#[derive(Debug, Clone, Default)]
pub struct ExecuteStage {
    /// ALU result (for debugging).
    pub alu_result: Word,
    /// Whether branch was taken (for debugging).
    pub branch_taken: bool,
    /// Branch target (for debugging).
    pub branch_target: Addr,
}

impl ExecuteStage {
    /// Create a new execute stage.
    pub fn new() -> Self {
        Self::default()
    }

    /// Execute the EX stage.
    ///
    /// # Arguments
    /// * `id_ex` - ID/EX pipeline register input
    /// * `ex_mem` - EX/MEM pipeline register (for forwarding)
    /// * `mem_wb` - MEM/WB pipeline register (for forwarding)
    /// * `flush` - Whether to flush this stage
    ///
    /// # Returns
    /// The EX/MEM pipeline register output.
    pub fn execute(
        &mut self,
        id_ex: &IdExRegister,
        ex_mem: &ExMemRegister,
        mem_wb: &MemWbRegister,
        flush: bool,
    ) -> Result<ExMemRegister> {
        // Debug output
        #[cfg(test)]
        {
            println!("    ExecuteStage: id_ex.rs1={}, id_ex.rs1_val={}, ex_mem.rd={}, ex_mem.alu_result={}",
                id_ex.rs1.raw(), id_ex.rs1_val.raw(), ex_mem.rd.raw(), ex_mem.alu_result.raw());
        }
        // Handle flush (insert bubble)
        if flush || !id_ex.valid {
            return Ok(ExMemRegister {
                pc: id_ex.pc,
                pc_plus_4: id_ex.pc_plus_4,
                alu_result: Word::ZERO,
                store_data: Word::ZERO,
                rd: RegIdx::new(0),
                ctrl: MemControlSignals::default(),
                branch_taken: false,
                branch_target: Addr::new(0),
                valid: false,
            });
        }

        // Apply forwarding to get actual operand values
        let (rs1_val, rs2_val) = ForwardUnit::apply_forwarding(id_ex, ex_mem, mem_wb);

        // Debug output after forwarding
        #[cfg(test)]
        {
            println!(
                "    ExecuteStage after forwarding: rs1_val={}, rs2_val={}",
                rs1_val.raw(),
                rs2_val.raw()
            );
        }

        // Determine ALU operands
        let operand1 = rs1_val;
        let operand2 = if id_ex.ctrl.alu_src == crate::cpu::pipeline::control::AluSrc::Immediate {
            Word::new(id_ex.imm as u32)
        } else {
            rs2_val
        };

        // Execute ALU operation
        let alu_result = self.execute_alu(id_ex.ctrl.alu_op, operand1, operand2);
        self.alu_result = alu_result;

        // Debug output for ALU
        #[cfg(test)]
        {
            println!(
                "    ExecuteStage ALU: alu_op={:?}, operand1={}, operand2={}, result={}",
                id_ex.ctrl.alu_op,
                operand1.raw(),
                operand2.raw(),
                alu_result.raw()
            );
        }

        // Branch/jump decision already resolved in ID stage.
        // Pass through id_ex.branch_taken and id_ex.branch_target.
        let branch_taken = id_ex.branch_taken;
        let branch_target = id_ex.branch_target;

        self.branch_taken = branch_taken;
        self.branch_target = branch_target;

        // Create EX/MEM register output
        Ok(ExMemRegister {
            pc: id_ex.pc,
            pc_plus_4: id_ex.pc_plus_4,
            alu_result,
            store_data: rs2_val, // For stores
            rd: id_ex.rd,
            ctrl: id_ex.mem_ctrl,
            branch_taken,
            branch_target,
            valid: true,
        })
    }

    /// Execute ALU operation.
    fn execute_alu(&self, op: AluOp, a: Word, b: Word) -> Word {
        match op {
            AluOp::Add => Word::new(a.raw().wrapping_add(b.raw())),
            AluOp::Sub => Word::new(a.raw().wrapping_sub(b.raw())),
            AluOp::And => Word::new(a.raw() & b.raw()),
            AluOp::Or => Word::new(a.raw() | b.raw()),
            AluOp::Xor => Word::new(a.raw() ^ b.raw()),
            AluOp::Sll => Word::new(a.raw() << (b.raw() & 0x1F)),
            AluOp::Srl => Word::new(a.raw() >> (b.raw() & 0x1F)),
            AluOp::Sra => {
                let shift = b.raw() & 0x1F;
                Word::new((a.as_signed() >> shift) as u32)
            }
            AluOp::Slt => Word::new(if a.as_signed() < b.as_signed() { 1 } else { 0 }),
            AluOp::Sltu => Word::new(if a.raw() < b.raw() { 1 } else { 0 }),
            AluOp::Lui => b, // Immediate is already in b
            AluOp::Pass => a,
            AluOp::Nop => Word::ZERO,
            AluOp::Csr => a, // For CSR instructions, pass rs1 value through
        }
    }

    /// Reset the execute stage.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::csr::CsrOp;
    use crate::cpu::pipeline::control::{AluSrc, ExControlSignals};

    fn create_id_ex_for_alu(
        alu_op: AluOp,
        rs1: u32,
        rs2: u32,
        imm: i32,
        alu_src: AluSrc,
    ) -> IdExRegister {
        IdExRegister {
            pc: Addr::new(0),
            pc_plus_4: Addr::new(4),
            rs1_val: Word::new(rs1),
            rs2_val: Word::new(rs2),
            rs1: RegIdx::new(1),
            rs2: RegIdx::new(2),
            rd: RegIdx::new(3),
            imm,
            ctrl: ExControlSignals {
                alu_op,
                alu_src,
                branch: false,
                jump: false,
                branch_type: BranchType::None,
                reg_write: true,
                csr_op: false,
                csr_op_type: CsrOp::ReadWrite,
                csr_addr: 0,
                trap_return: false,
            },
            mem_ctrl: MemControlSignals::default(),
            branch_taken: false,
            branch_target: Addr::new(0),
            valid: true,
        }
    }

    #[test]
    fn test_alu_add() {
        let mut stage = ExecuteStage::new();
        let id_ex = create_id_ex_for_alu(AluOp::Add, 100, 50, 0, AluSrc::Register);

        let ex_mem = stage
            .execute(
                &id_ex,
                &ExMemRegister::default(),
                &MemWbRegister::default(),
                false,
            )
            .unwrap();

        assert_eq!(ex_mem.alu_result.raw(), 150);
    }

    #[test]
    fn test_alu_sub() {
        let mut stage = ExecuteStage::new();
        let id_ex = create_id_ex_for_alu(AluOp::Sub, 100, 30, 0, AluSrc::Register);

        let ex_mem = stage
            .execute(
                &id_ex,
                &ExMemRegister::default(),
                &MemWbRegister::default(),
                false,
            )
            .unwrap();

        assert_eq!(ex_mem.alu_result.raw(), 70);
    }

    #[test]
    fn test_alu_immediate() {
        let mut stage = ExecuteStage::new();
        let id_ex = create_id_ex_for_alu(AluOp::Add, 100, 0, 50, AluSrc::Immediate);

        let ex_mem = stage
            .execute(
                &id_ex,
                &ExMemRegister::default(),
                &MemWbRegister::default(),
                false,
            )
            .unwrap();

        assert_eq!(ex_mem.alu_result.raw(), 150);
    }

    #[test]
    fn test_alu_and() {
        let mut stage = ExecuteStage::new();
        let id_ex = create_id_ex_for_alu(AluOp::And, 0xFF, 0x0F, 0, AluSrc::Register);

        let ex_mem = stage
            .execute(
                &id_ex,
                &ExMemRegister::default(),
                &MemWbRegister::default(),
                false,
            )
            .unwrap();

        assert_eq!(ex_mem.alu_result.raw(), 0x0F);
    }

    #[test]
    fn test_alu_slt() {
        let mut stage = ExecuteStage::new();

        // 5 < 10 = true
        let id_ex = create_id_ex_for_alu(AluOp::Slt, 5, 10, 0, AluSrc::Register);
        let ex_mem = stage
            .execute(
                &id_ex,
                &ExMemRegister::default(),
                &MemWbRegister::default(),
                false,
            )
            .unwrap();
        assert_eq!(ex_mem.alu_result.raw(), 1);

        // 10 < 5 = false
        let id_ex = create_id_ex_for_alu(AluOp::Slt, 10, 5, 0, AluSrc::Register);
        let ex_mem = stage
            .execute(
                &id_ex,
                &ExMemRegister::default(),
                &MemWbRegister::default(),
                false,
            )
            .unwrap();
        assert_eq!(ex_mem.alu_result.raw(), 0);
    }

    #[test]
    fn test_branch_beq() {
        let mut stage = ExecuteStage::new();
        let mut id_ex = create_id_ex_for_alu(AluOp::Sub, 100, 100, 8, AluSrc::Register);
        id_ex.ctrl.branch = true;
        id_ex.ctrl.branch_type = BranchType::Beq;
        // Branch decision is now made in ID stage
        id_ex.branch_taken = true;
        id_ex.branch_target = Addr::new(8);

        let ex_mem = stage
            .execute(
                &id_ex,
                &ExMemRegister::default(),
                &MemWbRegister::default(),
                false,
            )
            .unwrap();

        assert!(ex_mem.branch_taken);
        assert_eq!(ex_mem.branch_target, Addr::new(8));
    }

    #[test]
    fn test_branch_bne() {
        let mut stage = ExecuteStage::new();
        let mut id_ex = create_id_ex_for_alu(AluOp::Sub, 100, 200, 8, AluSrc::Register);
        id_ex.ctrl.branch = true;
        id_ex.ctrl.branch_type = BranchType::Bne;
        // Branch decision is now made in ID stage
        id_ex.branch_taken = true;
        id_ex.branch_target = Addr::new(8);

        let ex_mem = stage
            .execute(
                &id_ex,
                &ExMemRegister::default(),
                &MemWbRegister::default(),
                false,
            )
            .unwrap();

        assert!(ex_mem.branch_taken);
    }

    #[test]
    fn test_flush() {
        let mut stage = ExecuteStage::new();
        let id_ex = create_id_ex_for_alu(AluOp::Add, 100, 50, 0, AluSrc::Register);

        let ex_mem = stage
            .execute(
                &id_ex,
                &ExMemRegister::default(),
                &MemWbRegister::default(),
                true,
            )
            .unwrap();

        assert!(!ex_mem.valid);
    }
}
