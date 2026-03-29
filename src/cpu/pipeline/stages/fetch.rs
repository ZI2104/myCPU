//! Instruction Fetch (IF) Stage.
//!
//! This module implements the IF stage of the pipeline.

use crate::cpu::pipeline::registers::IfIdRegister;
use crate::error::Result;
use crate::memory::Bus;
use crate::types::Addr;

/// Instruction Fetch stage.
#[derive(Debug, Clone, Default)]
pub struct FetchStage {
    /// Current PC value.
    pub pc: Addr,
    /// Next PC value (PC + 4).
    pub pc_plus_4: Addr,
}

impl FetchStage {
    /// Create a new fetch stage.
    pub fn new() -> Self {
        Self::default()
    }

    /// Execute the IF stage.
    ///
    /// # Arguments
    /// * `bus` - Memory bus for instruction fetch
    /// * `stall` - Whether to stall this stage
    /// * `branch_target` - Branch target address (if branch taken)
    /// * `branch_taken` - Whether a branch is being taken
    ///
    /// # Returns
    /// The IF/ID pipeline register output.
    pub fn execute(
        &mut self,
        bus: &Bus,
        stall: bool,
        branch_target: Addr,
        branch_taken: bool,
    ) -> Result<IfIdRegister> {
        if stall {
            // Don't update anything on stall
            return Ok(IfIdRegister {
                pc: self.pc,
                instruction: 0,
                valid: false,
            });
        }

        // Update PC if branch taken
        if branch_taken {
            self.pc = branch_target;
        }

        // Fetch instruction
        let instruction = bus.read_word(self.pc)?;
        self.pc_plus_4 = self.pc + Addr::new(4);

        // Create IF/ID register output
        let if_id = IfIdRegister {
            pc: self.pc,
            instruction: instruction.raw(),
            valid: true,
        };

        // Advance PC
        self.pc = self.pc_plus_4;

        Ok(if_id)
    }

    /// Set the PC to a new value.
    pub fn set_pc(&mut self, pc: Addr) {
        self.pc = pc;
        self.pc_plus_4 = pc + Addr::new(4);
    }

    /// Get the current PC.
    pub fn pc(&self) -> Addr {
        self.pc
    }

    /// Reset the fetch stage.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Bus, Ram};

    fn create_test_bus() -> Bus {
        let mut bus = Bus::new();
        let ram = Ram::new(4096);
        bus.attach_memory(Addr::new(0), ram, "RAM");
        bus
    }

    #[test]
    fn test_fetch_stage_default() {
        let stage = FetchStage::new();
        assert_eq!(stage.pc(), Addr::new(0));
    }

    #[test]
    fn test_fetch_instruction() {
        let mut bus = create_test_bus();
        bus.write_word(Addr::new(0), crate::types::Word::new(0x12345678))
            .unwrap();

        let mut stage = FetchStage::new();
        let if_id = stage.execute(&bus, false, Addr::new(0), false).unwrap();

        assert_eq!(if_id.instruction, 0x12345678);
        assert!(if_id.valid);
        assert_eq!(stage.pc(), Addr::new(4));
    }

    #[test]
    fn test_fetch_stall() {
        let mut bus = create_test_bus();
        bus.write_word(Addr::new(0), crate::types::Word::new(0x12345678))
            .unwrap();

        let mut stage = FetchStage::new();

        // First fetch
        let _ = stage.execute(&bus, false, Addr::new(0), false).unwrap();

        // Stalled fetch - PC should not advance
        let if_id = stage.execute(&bus, true, Addr::new(0), false).unwrap();
        assert!(!if_id.valid);
        assert_eq!(stage.pc(), Addr::new(4)); // PC unchanged during stall
    }

    #[test]
    fn test_fetch_branch() {
        let mut bus = create_test_bus();
        bus.write_word(Addr::new(0x100), crate::types::Word::new(0xABCDEF00))
            .unwrap();

        let mut stage = FetchStage::new();

        // Branch to 0x100
        let if_id = stage.execute(&bus, false, Addr::new(0x100), true).unwrap();

        assert_eq!(if_id.pc, Addr::new(0x100));
        assert_eq!(if_id.instruction, 0xABCDEF00);
    }
}
