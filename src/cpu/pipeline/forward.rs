//! Forwarding unit implementation.
//!
//! This module provides forwarding (bypassing) logic to resolve data hazards
//! without stalling the pipeline.

use crate::cpu::pipeline::registers::{ExMemRegister, IdExRegister, MemWbRegister};
use crate::types::Word;

/// Forwarding source selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ForwardSource {
    /// Use value from register file (no forwarding).
    #[default]
    None,
    /// Forward from EX/MEM pipeline register.
    ExMem,
    /// Forward from MEM/WB pipeline register.
    MemWb,
}

/// Forwarding unit.
#[derive(Debug, Clone, Copy, Default)]
pub struct ForwardUnit {
    /// Forwarding control for rs1.
    pub forward_rs1: ForwardSource,
    /// Forwarding control for rs2.
    pub forward_rs2: ForwardSource,
}

impl ForwardUnit {
    /// Create a new forwarding unit.
    pub fn new() -> Self {
        Self::default()
    }

    /// Determine forwarding source for rs1.
    ///
    /// Forwarding priority:
    /// 1. EX/MEM (most recent result) - EX hazard
    /// 2. MEM/WB (previous result) - MEM hazard
    pub fn forward_rs1_source(
        id_ex: &IdExRegister,
        ex_mem: &ExMemRegister,
        mem_wb: &MemWbRegister,
    ) -> ForwardSource {
        // EX hazard: previous instruction writes to rs1's source
        if ex_mem.ctrl.reg_write
            && !ex_mem.rd.is_zero()
            && ex_mem.rd == id_ex.rs1
        {
            return ForwardSource::ExMem;
        }

        // MEM hazard: instruction before previous writes to rs1's source
        // Note: We only reach here if EX hazard check failed (early return above)
        if mem_wb.ctrl.reg_write
            && !mem_wb.rd.is_zero()
            && mem_wb.rd == id_ex.rs1
        {
            return ForwardSource::MemWb;
        }

        ForwardSource::None
    }

    /// Determine forwarding source for rs2.
    pub fn forward_rs2_source(
        id_ex: &IdExRegister,
        ex_mem: &ExMemRegister,
        mem_wb: &MemWbRegister,
    ) -> ForwardSource {
        // EX hazard
        if ex_mem.ctrl.reg_write
            && !ex_mem.rd.is_zero()
            && ex_mem.rd == id_ex.rs2
        {
            return ForwardSource::ExMem;
        }

        // MEM hazard
        if mem_wb.ctrl.reg_write
            && !mem_wb.rd.is_zero()
            && mem_wb.rd == id_ex.rs2
        {
            return ForwardSource::MemWb;
        }

        ForwardSource::None
    }

    /// Update forwarding control signals.
    pub fn update(
        &mut self,
        id_ex: &IdExRegister,
        ex_mem: &ExMemRegister,
        mem_wb: &MemWbRegister,
    ) {
        self.forward_rs1 = Self::forward_rs1_source(id_ex, ex_mem, mem_wb);
        self.forward_rs2 = Self::forward_rs2_source(id_ex, ex_mem, mem_wb);
    }

    /// Apply forwarding to get the actual operand values.
    pub fn apply_forwarding(
        id_ex: &IdExRegister,
        ex_mem: &ExMemRegister,
        mem_wb: &MemWbRegister,
    ) -> (Word, Word) {
        let rs1_source = Self::forward_rs1_source(id_ex, ex_mem, mem_wb);
        let rs2_source = Self::forward_rs2_source(id_ex, ex_mem, mem_wb);

        let rs1_val = match rs1_source {
            ForwardSource::ExMem => ex_mem.alu_result,
            ForwardSource::MemWb => mem_wb.write_data,
            ForwardSource::None => id_ex.rs1_val,
        };

        let rs2_val = match rs2_source {
            ForwardSource::ExMem => ex_mem.alu_result,
            ForwardSource::MemWb => mem_wb.write_data,
            ForwardSource::None => id_ex.rs2_val,
        };

        (rs1_val, rs2_val)
    }

    /// Reset the forwarding unit state.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RegIdx;

    #[test]
    fn test_forward_unit_default() {
        let unit = ForwardUnit::default();
        assert_eq!(unit.forward_rs1, ForwardSource::None);
        assert_eq!(unit.forward_rs2, ForwardSource::None);
    }

    #[test]
    fn test_forward_ex_hazard_rs1() {
        // Setup: ADD x1, x2, x3 in EX/MEM, SUB x4, x1, x5 in ID/EX
        let mut id_ex = IdExRegister::default();
        id_ex.rs1 = RegIdx::new(1);
        id_ex.rs1_val = Word::new(0); // Old value

        let mut ex_mem = ExMemRegister::default();
        ex_mem.rd = RegIdx::new(1);
        ex_mem.alu_result = Word::new(42); // New value
        ex_mem.ctrl.reg_write = true;

        let mem_wb = MemWbRegister::default();

        let source = ForwardUnit::forward_rs1_source(&id_ex, &ex_mem, &mem_wb);
        assert_eq!(source, ForwardSource::ExMem);

        let (rs1, _) = ForwardUnit::apply_forwarding(&id_ex, &ex_mem, &mem_wb);
        assert_eq!(rs1.raw(), 42);
    }

    #[test]
    fn test_forward_ex_hazard_rs2() {
        // Setup: ADD x1, x2, x3 in EX/MEM, SUB x4, x5, x1 in ID/EX
        let mut id_ex = IdExRegister::default();
        id_ex.rs2 = RegIdx::new(1);
        id_ex.rs2_val = Word::new(0);

        let mut ex_mem = ExMemRegister::default();
        ex_mem.rd = RegIdx::new(1);
        ex_mem.alu_result = Word::new(100);
        ex_mem.ctrl.reg_write = true;

        let mem_wb = MemWbRegister::default();

        let source = ForwardUnit::forward_rs2_source(&id_ex, &ex_mem, &mem_wb);
        assert_eq!(source, ForwardSource::ExMem);
    }

    #[test]
    fn test_forward_mem_hazard() {
        // Setup: ADD x1 in MEM/WB, SUB x4, x1, x5 in ID/EX, no EX hazard
        let mut id_ex = IdExRegister::default();
        id_ex.rs1 = RegIdx::new(1);
        id_ex.rs1_val = Word::new(0);

        let ex_mem = ExMemRegister::default(); // No hazard from EX

        let mut mem_wb = MemWbRegister::default();
        mem_wb.rd = RegIdx::new(1);
        mem_wb.write_data = Word::new(55);
        mem_wb.ctrl.reg_write = true;

        let source = ForwardUnit::forward_rs1_source(&id_ex, &ex_mem, &mem_wb);
        assert_eq!(source, ForwardSource::MemWb);

        let (rs1, _) = ForwardUnit::apply_forwarding(&id_ex, &ex_mem, &mem_wb);
        assert_eq!(rs1.raw(), 55);
    }

    #[test]
    fn test_forward_ex_priority_over_mem() {
        // Setup: EX/MEM and MEM/WB both write to x1, EX should win
        let mut id_ex = IdExRegister::default();
        id_ex.rs1 = RegIdx::new(1);

        let mut ex_mem = ExMemRegister::default();
        ex_mem.rd = RegIdx::new(1);
        ex_mem.alu_result = Word::new(10); // EX result
        ex_mem.ctrl.reg_write = true;

        let mut mem_wb = MemWbRegister::default();
        mem_wb.rd = RegIdx::new(1);
        mem_wb.write_data = Word::new(20); // MEM result
        mem_wb.ctrl.reg_write = true;

        let source = ForwardUnit::forward_rs1_source(&id_ex, &ex_mem, &mem_wb);
        assert_eq!(source, ForwardSource::ExMem);

        let (rs1, _) = ForwardUnit::apply_forwarding(&id_ex, &ex_mem, &mem_wb);
        assert_eq!(rs1.raw(), 10); // Should get EX result, not MEM
    }

    #[test]
    fn test_no_forward_for_x0() {
        // x0 is always 0, no forwarding needed
        let mut id_ex = IdExRegister::default();
        id_ex.rs1 = RegIdx::new(5);

        let mut ex_mem = ExMemRegister::default();
        ex_mem.rd = RegIdx::new(0); // Writing to x0
        ex_mem.ctrl.reg_write = true;

        let mem_wb = MemWbRegister::default();

        let source = ForwardUnit::forward_rs1_source(&id_ex, &ex_mem, &mem_wb);
        assert_eq!(source, ForwardSource::None);
    }
}
