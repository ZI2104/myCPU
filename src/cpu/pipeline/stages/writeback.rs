//! Write Back (WB) Stage.
//!
//! This module implements the WB stage of the pipeline.

use crate::cpu::pipeline::registers::MemWbRegister;
use crate::cpu::Registers;
use crate::types::RegIdx;
use crate::error::Result;

/// Write Back stage.
#[derive(Debug, Clone, Default)]
pub struct WritebackStage {
    /// Value written back (for debugging).
    pub write_value: crate::types::Word,
    /// Destination register (for debugging).
    pub write_rd: RegIdx,
    /// Whether a write occurred (for debugging).
    pub did_write: bool,
}

impl WritebackStage {
    /// Create a new writeback stage.
    pub fn new() -> Self {
        Self::default()
    }

    /// Execute the WB stage.
    ///
    /// # Arguments
    /// * `mem_wb` - MEM/WB pipeline register input
    /// * `regs` - Register file to write back to
    ///
    /// # Returns
    /// Whether an instruction completed.
    pub fn execute(
        &mut self,
        mem_wb: &MemWbRegister,
        regs: &mut Registers,
    ) -> Result<bool> {
        self.write_value = mem_wb.write_data;
        self.write_rd = mem_wb.rd;
        self.did_write = false;

        // Check if this is a valid instruction with register write enabled
        if !mem_wb.valid || !mem_wb.ctrl.reg_write {
            return Ok(false);
        }

        // Don't write to x0 (hardwired to zero)
        if mem_wb.rd.is_zero() {
            return Ok(true);
        }

        // Write back the result
        regs.write(mem_wb.rd, mem_wb.write_data);
        self.did_write = true;

        Ok(true)
    }

    /// Reset the writeback stage.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::pipeline::control::WbControlSignals;
    use crate::types::{Addr, Word};

    fn create_mem_wb(rd: u8, value: u32, reg_write: bool) -> MemWbRegister {
        MemWbRegister {
            pc: Addr::new(0),
            write_data: Word::new(value),
            rd: RegIdx::new(rd),
            ctrl: WbControlSignals {
                reg_write,
                mem_to_reg: false,
            },
            valid: true,
        }
    }

    #[test]
    fn test_writeback_to_register() {
        let mut stage = WritebackStage::new();
        let mut regs = Registers::new();

        let mem_wb = create_mem_wb(5, 0x12345678, true);
        let completed = stage.execute(&mem_wb, &mut regs).unwrap();

        assert!(completed);
        assert!(stage.did_write);
        assert_eq!(stage.write_rd, RegIdx::new(5));
        assert_eq!(regs.read(RegIdx::new(5)).raw(), 0x12345678);
    }

    #[test]
    fn test_writeback_no_write_signal() {
        let mut stage = WritebackStage::new();
        let mut regs = Registers::new();

        // Pre-set register value
        regs.write(RegIdx::new(5), Word::new(0xDEADBEEF));

        let mem_wb = create_mem_wb(5, 0x12345678, false);
        let completed = stage.execute(&mem_wb, &mut regs).unwrap();

        assert!(!completed);
        assert!(!stage.did_write);
        // Register should NOT be changed
        assert_eq!(regs.read(RegIdx::new(5)).raw(), 0xDEADBEEF);
    }

    #[test]
    fn test_writeback_to_x0_ignored() {
        let mut stage = WritebackStage::new();
        let mut regs = Registers::new();

        let mem_wb = create_mem_wb(0, 0x12345678, true);
        let completed = stage.execute(&mem_wb, &mut regs).unwrap();

        // Instruction completed, but x0 should still be 0
        assert!(completed);
        assert!(!stage.did_write); // We set did_write = false for x0
        assert_eq!(regs.read(RegIdx::new(0)).raw(), 0);
    }

    #[test]
    fn test_writeback_invalid_instruction() {
        let mut stage = WritebackStage::new();
        let mut regs = Registers::new();

        let mem_wb = MemWbRegister {
            pc: Addr::new(0),
            write_data: Word::new(0x12345678),
            rd: RegIdx::new(5),
            ctrl: WbControlSignals {
                reg_write: true,
                mem_to_reg: false,
            },
            valid: false,
        };

        let completed = stage.execute(&mem_wb, &mut regs).unwrap();

        assert!(!completed);
        assert!(!stage.did_write);
        assert_eq!(regs.read(RegIdx::new(5)).raw(), 0);
    }
}
