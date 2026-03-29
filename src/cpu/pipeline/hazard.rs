//! Hazard detection unit.
//!
//! This module detects data hazards (load-use) and control hazards.

use crate::cpu::pipeline::registers::{ExMemRegister, IdExRegister, IfIdRegister};
use crate::instruction::Decoder;
use crate::types::RegIdx;

/// Hazard detection unit.
#[derive(Debug, Clone, Copy, Default)]
pub struct HazardUnit {
    /// Whether to stall the pipeline.
    pub stall: bool,
    /// Whether to flush IF/ID (control hazard).
    pub flush_if_id: bool,
    /// Whether to flush ID/EX (control hazard or load-use).
    pub flush_id_ex: bool,
    /// Whether to stall the PC (don't increment).
    pub stall_pc: bool,
}

impl HazardUnit {
    /// Create a new hazard unit.
    pub fn new() -> Self {
        Self::default()
    }

    /// Detect load-use hazard.
    ///
    /// Load-use hazard occurs when:
    /// - ID/EX contains a load instruction (mem_read = true)
    /// - The load's destination register matches rs1 or rs2 of the instruction in ID
    pub fn detect_load_use(
        &self,
        id_ex: &IdExRegister,
        id_rs1: RegIdx,
        id_rs2: RegIdx,
    ) -> bool {
        id_ex.mem_ctrl.mem_read
            && !id_ex.rd.is_zero()
            && (id_ex.rd == id_rs1 || id_ex.rd == id_rs2)
    }

    /// Detect control hazard (branch/jump in EX stage).
    pub fn detect_control_hazard(&self, ex_mem: &ExMemRegister) -> bool {
        ex_mem.branch_taken
    }

    /// Update stall and flush signals based on pipeline state.
    pub fn update(
        &mut self,
        id_ex: &IdExRegister,
        if_id: &IfIdRegister,
        ex_mem: &ExMemRegister,
    ) {
        // Extract rs1, rs2 from IF/ID instruction using Decoder helper
        let id_rs1 = Decoder::rs1(if_id.instruction);
        let id_rs2 = Decoder::rs2(if_id.instruction);

        // Load-use hazard: stall IF and ID, flush ID/EX
        let load_use = self.detect_load_use(id_ex, id_rs1, id_rs2);

        // Control hazard: flush IF/ID and ID/EX
        let control = self.detect_control_hazard(ex_mem);

        self.stall = load_use;
        self.stall_pc = load_use;
        self.flush_if_id = control;
        self.flush_id_ex = control || load_use;
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
}
