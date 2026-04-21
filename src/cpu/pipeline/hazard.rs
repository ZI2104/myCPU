//! Hazard detection unit.
//!
//! This module detects data hazards (load-use, branch-data) and manages
//! stall/flush signals for the pipeline.

use crate::cpu::pipeline::registers::{ExMemRegister, IdExRegister, IfIdRegister};
use crate::instruction::opcode::opcode;
use crate::instruction::Decoder;
use crate::types::RegIdx;

/// Hazard detection unit.
#[derive(Debug, Clone, Copy, Default)]
pub struct HazardUnit {
    /// Whether to stall the pipeline (IF and ID frozen).
    pub stall: bool,
    /// Whether to flush IF/ID (control hazard).
    pub flush_if_id: bool,
    /// Whether to flush ID/EX (load-use hazard).
    pub flush_id_ex: bool,
    /// Whether to stall the PC (don't increment).
    pub stall_pc: bool,
}

impl HazardUnit {
    /// Create a new hazard unit.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if the instruction is a branch or jump.
    fn is_branch_or_jump(instr: u32) -> bool {
        let op = (instr & 0x7F) as u8;
        matches!(op, opcode::BRANCH | opcode::JAL | opcode::JALR)
    }

    /// Detect load-use hazard.
    ///
    /// Load-use hazard occurs when:
    /// - ID/EX contains a load instruction (mem_read = true)
    /// - The load's destination register matches rs1 or rs2 of the instruction in ID
    pub fn detect_load_use(&self, id_ex: &IdExRegister, id_rs1: RegIdx, id_rs2: RegIdx) -> bool {
        id_ex.mem_ctrl.mem_read && !id_ex.rd.is_zero() && (id_ex.rd == id_rs1 || id_ex.rd == id_rs2)
    }

    /// Detect ID-stage branch data hazard: id_ex writes to a source register
    /// used by a branch/jump in ID, and the result is not yet available.
    pub fn detect_id_ex_branch_hazard(
        &self,
        id_ex: &IdExRegister,
        id_rs1: RegIdx,
        id_rs2: RegIdx,
        is_branch_or_jump: bool,
    ) -> bool {
        if !is_branch_or_jump {
            return false;
        }
        id_ex.ctrl.reg_write && !id_ex.rd.is_zero() && (id_ex.rd == id_rs1 || id_ex.rd == id_rs2)
    }

    /// Detect ex_mem load-to-branch hazard: ex_mem is a load that writes
    /// to a source register used by a branch/jump in ID.
    /// (Load data is not available in ex_mem for forwarding.)
    pub fn detect_ex_mem_load_branch_hazard(
        &self,
        ex_mem: &ExMemRegister,
        id_rs1: RegIdx,
        id_rs2: RegIdx,
        is_branch_or_jump: bool,
    ) -> bool {
        if !is_branch_or_jump {
            return false;
        }
        ex_mem.ctrl.mem_read
            && ex_mem.ctrl.reg_write
            && !ex_mem.rd.is_zero()
            && (ex_mem.rd == id_rs1 || ex_mem.rd == id_rs2)
    }

    /// Update stall and flush signals based on pipeline state.
    ///
    /// Control hazard flushing is NOT handled here — it is handled in
    /// `clock()` using `new_id_ex.branch_taken` (which is only available
    /// after the ID stage executes).
    pub fn update(&mut self, id_ex: &IdExRegister, if_id: &IfIdRegister, ex_mem: &ExMemRegister) {
        let id_rs1 = Decoder::rs1(if_id.instruction);
        let id_rs2 = Decoder::rs2(if_id.instruction);
        let is_branch = Self::is_branch_or_jump(if_id.instruction);

        // Existing load-use hazard
        let load_use = self.detect_load_use(id_ex, id_rs1, id_rs2);

        // New: ID-stage branch data hazards
        let id_ex_branch = self.detect_id_ex_branch_hazard(id_ex, id_rs1, id_rs2, is_branch);
        let ex_mem_load_branch =
            self.detect_ex_mem_load_branch_hazard(ex_mem, id_rs1, id_rs2, is_branch);

        let data_stall = load_use || id_ex_branch || ex_mem_load_branch;

        self.stall = data_stall;
        self.stall_pc = data_stall;
        // Control hazard flush is handled in clock() after ID stage executes
        self.flush_if_id = false;
        // Flush ID/EX only for load-use (insert bubble between ID and EX)
        self.flush_id_ex = load_use;
    }

    /// Reset the hazard unit state.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hazard_unit_default() {
        let unit = HazardUnit::default();
        assert!(!unit.stall);
        assert!(!unit.flush_if_id);
        assert!(!unit.flush_id_ex);
    }

    #[test]
    fn test_load_use_detection() {
        let mut id_ex = IdExRegister::default();
        id_ex.mem_ctrl.mem_read = true;
        id_ex.rd = RegIdx::new(5);

        let unit = HazardUnit::new();
        assert!(unit.detect_load_use(&id_ex, RegIdx::new(5), RegIdx::new(0)));
        assert!(!unit.detect_load_use(&id_ex, RegIdx::new(6), RegIdx::new(7)));
    }

    #[test]
    fn test_load_use_no_hazard_for_x0() {
        let mut id_ex = IdExRegister::default();
        id_ex.mem_ctrl.mem_read = true;
        id_ex.rd = RegIdx::new(0); // x0 is always 0, no hazard

        let unit = HazardUnit::new();
        assert!(!unit.detect_load_use(&id_ex, RegIdx::new(0), RegIdx::new(0)));
    }

    #[test]
    fn test_id_ex_branch_hazard() {
        let mut id_ex = IdExRegister::default();
        id_ex.ctrl.reg_write = true;
        id_ex.rd = RegIdx::new(5);

        let unit = HazardUnit::new();
        // Hazard when branch uses x5
        assert!(unit.detect_id_ex_branch_hazard(&id_ex, RegIdx::new(5), RegIdx::new(0), true));
        // No hazard for non-branch
        assert!(!unit.detect_id_ex_branch_hazard(&id_ex, RegIdx::new(5), RegIdx::new(0), false));
        // No hazard when branch doesn't use x5
        assert!(!unit.detect_id_ex_branch_hazard(&id_ex, RegIdx::new(6), RegIdx::new(7), true));
    }

    #[test]
    fn test_ex_mem_load_branch_hazard() {
        let mut ex_mem = ExMemRegister::default();
        ex_mem.ctrl.mem_read = true;
        ex_mem.ctrl.reg_write = true;
        ex_mem.rd = RegIdx::new(3);

        let unit = HazardUnit::new();
        // Hazard when branch uses x3
        assert!(unit.detect_ex_mem_load_branch_hazard(
            &ex_mem,
            RegIdx::new(3),
            RegIdx::new(0),
            true
        ));
        // No hazard for non-branch
        assert!(!unit.detect_ex_mem_load_branch_hazard(
            &ex_mem,
            RegIdx::new(3),
            RegIdx::new(0),
            false
        ));
    }

    #[test]
    fn test_is_branch_or_jump() {
        // BEQ opcode: 0b1100011
        let beq: u32 = 0b0000000_00000_00000_000_00000_1100011;
        assert!(HazardUnit::is_branch_or_jump(beq));

        // JAL opcode: 0b1101111
        let jal: u32 = 0b00000000000000000000_00000_1101111;
        assert!(HazardUnit::is_branch_or_jump(jal));

        // JALR opcode: 0b1100111
        let jalr: u32 = 0b000000000000_00000_000_00000_1100111;
        assert!(HazardUnit::is_branch_or_jump(jalr));

        // ADDI opcode: 0b0010011 — not a branch
        let addi: u32 = 0b000000000000_00000_000_00000_0010011;
        assert!(!HazardUnit::is_branch_or_jump(addi));
    }
}
