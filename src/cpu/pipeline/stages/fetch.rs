//! Instruction Fetch (pre-IF + IF) Stage.
//!
//! This module implements the pre-IF and IF phases of the pipeline.
//! The design uses synchronous instruction RAM: the read address is presented
//! in the pre-IF phase (combinational logic, no pipeline latch), and the
//! instruction data becomes available in the IF phase (one cycle later).
//!
//! # Pipeline timing
//!
//! ```text
//! Cycle N:   pre-IF → compute nextPC, issue RAM read at nextPC
//! Cycle N+1: IF     → read instr_latch (data from cycle N's request)
//! ```

use crate::cpu::pipeline::registers::{IfIdRegister, InstrFetchLatch};
use crate::error::Result;
use crate::memory::Bus;
use crate::types::Addr;

/// Instruction Fetch stage.
///
/// Contains both the pre-IF (combinational) and IF (registered) logic.
/// The `pc` field tracks the next PC to fetch from (updated by pre-IF).
#[derive(Debug, Clone, Default)]
pub struct FetchStage {
    /// Next PC to fetch from (set by pre-IF, read by IF).
    pub pc: Addr,
    /// PC + 4 (cached for convenience).
    pub pc_plus_4: Addr,
}

impl FetchStage {
    /// Create a new fetch stage.
    pub fn new() -> Self {
        Self::default()
    }

    /// pre-IF phase: compute nextPC and issue instruction RAM read request.
    ///
    /// This is the combinational "pseudo-stage" that generates the read address
    /// for the synchronous instruction RAM. The result is stored in an
    /// `InstrFetchLatch` and becomes available to the IF phase in the next cycle.
    ///
    /// # Arguments
    /// * `bus` - Memory bus for instruction fetch
    /// * `stall` - Whether the pipeline is stalled (load-use hazard)
    /// * `branch_target` - Branch target address (from EX/MEM stage)
    /// * `branch_taken` - Whether a branch/jump is being taken
    /// * `translate` - Address translation hook (virtual → physical)
    ///
    /// # Returns
    /// A new `InstrFetchLatch` containing the fetched instruction.
    pub fn pre_fetch<F>(
        &mut self,
        bus: &Bus,
        stall: bool,
        branch_target: Addr,
        branch_taken: bool,
        translate: F,
    ) -> Result<InstrFetchLatch>
    where
        F: Fn(&Bus, Addr) -> Result<Addr>,
    {
        if stall {
            // Stall: do not issue new request, return invalid latch
            return Ok(InstrFetchLatch {
                pc: self.pc,
                instruction: 0,
                valid: false,
            });
        }

        // Compute nextPC: branch target or sequential PC+4
        let next_pc = if branch_taken {
            branch_target
        } else {
            self.pc + Addr::new(4)
        };

        // Issue synchronous RAM read: translate virtual address, then read
        let phys_pc = translate(bus, next_pc)?;
        let instruction = bus.read_word(phys_pc)?;

        // Update internal PC state
        self.pc = next_pc;
        self.pc_plus_4 = next_pc + Addr::new(4);

        Ok(InstrFetchLatch {
            pc: next_pc,
            instruction: instruction.raw(),
            valid: true,
        })
    }

    /// IF phase: read from the instruction fetch latch and produce IF/ID register.
    ///
    /// This reads the instruction that was fetched by the previous cycle's
    /// pre-IF phase and creates the IF/ID pipeline register output.
    pub fn fetch(&self, latch: &InstrFetchLatch) -> IfIdRegister {
        IfIdRegister {
            pc: latch.pc,
            instruction: latch.instruction,
            valid: latch.valid,
            prediction: None,
        }
    }

    // -----------------------------------------------------------------------
    // Legacy API (kept for backward compatibility with single-cycle CPU)
    // -----------------------------------------------------------------------

    /// Execute the full IF stage in one step (legacy, for single-cycle model).
    ///
    /// Combines pre-IF and IF into a single operation. Used by the single-cycle
    /// CPU model where synchronous RAM behavior is not needed.
    pub fn execute(
        &mut self,
        bus: &Bus,
        stall: bool,
        branch_target: Addr,
        branch_taken: bool,
    ) -> Result<IfIdRegister> {
        self.execute_with_translate(bus, stall, branch_target, branch_taken, |_bus, addr| {
            Ok(addr)
        })
    }

    /// Execute full IF stage with address translation (legacy).
    pub fn execute_with_translate<F>(
        &mut self,
        bus: &Bus,
        stall: bool,
        branch_target: Addr,
        branch_taken: bool,
        translate: F,
    ) -> Result<IfIdRegister>
    where
        F: Fn(&Bus, Addr) -> Result<Addr>,
    {
        if stall {
            return Ok(IfIdRegister {
                pc: self.pc,
                instruction: 0,
                valid: false,
                prediction: None,
            });
        }

        if branch_taken {
            self.pc = branch_target;
        }

        let phys_pc = translate(bus, self.pc)?;
        let instruction = bus.read_word(phys_pc)?;
        self.pc_plus_4 = self.pc + Addr::new(4);

        let if_id = IfIdRegister {
            pc: self.pc,
            instruction: instruction.raw(),
            valid: true,
            prediction: None,
        };

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
    use crate::types::Word;

    fn create_test_bus() -> Bus {
        let mut bus = Bus::new();
        let ram = Ram::new(4096);
        bus.attach_memory(Addr::new(0), ram, "RAM");
        bus
    }

    fn create_test_bus_with_program(program: &[(u32, u32)]) -> Bus {
        let mut bus = Bus::new();
        let ram = Ram::new(4096);
        bus.attach_memory(Addr::new(0), ram, "RAM");
        for &(addr, instr) in program {
            bus.write_word(Addr::new(addr), Word::new(instr)).unwrap();
        }
        bus
    }

    // === pre-IF / IF split tests ===

    #[test]
    fn test_pre_fetch_then_fetch() {
        let bus = create_test_bus_with_program(&[(0x000, 0x12345678), (0x004, 0xABCDEF00)]);

        let mut stage = FetchStage::new();
        // PC starts at 0, pre_fetch uses nextPC = PC+4 = 4
        let latch = stage
            .pre_fetch(&bus, false, Addr::new(0), false, |_bus, addr| Ok(addr))
            .unwrap();

        // Latch contains instruction at address 4 (nextPC)
        assert!(latch.valid);
        assert_eq!(latch.pc, Addr::new(4));
        assert_eq!(latch.instruction, 0xABCDEF00);
        assert_eq!(stage.pc(), Addr::new(4));

        // IF phase reads from the latch
        let if_id = stage.fetch(&latch);
        assert_eq!(if_id.instruction, 0xABCDEF00);
        assert!(if_id.valid);
    }

    #[test]
    fn test_pre_fetch_stall() {
        let bus = create_test_bus_with_program(&[(0x004, 0xDEADBEEF)]);

        let mut stage = FetchStage::new();
        stage.set_pc(Addr::new(0));

        // Stalled pre-fetch: should return invalid latch, PC unchanged
        let latch = stage
            .pre_fetch(&bus, true, Addr::new(0), false, |_bus, addr| Ok(addr))
            .unwrap();

        assert!(!latch.valid);
        assert_eq!(stage.pc(), Addr::new(0)); // PC unchanged during stall
    }

    #[test]
    fn test_pre_fetch_branch() {
        let bus = create_test_bus_with_program(&[(0x004, 0x11111111), (0x100, 0xABCDEF00)]);

        let mut stage = FetchStage::new();
        // Branch to 0x100: nextPC = 0x100
        let latch = stage
            .pre_fetch(&bus, false, Addr::new(0x100), true, |_bus, addr| Ok(addr))
            .unwrap();

        assert!(latch.valid);
        assert_eq!(latch.pc, Addr::new(0x100));
        assert_eq!(latch.instruction, 0xABCDEF00);
    }

    #[test]
    fn test_fetch_from_invalid_latch() {
        let stage = FetchStage::new();
        let latch = InstrFetchLatch {
            pc: Addr::new(0x100),
            instruction: 0,
            valid: false,
        };

        let if_id = stage.fetch(&latch);
        assert!(!if_id.valid);
    }

    // === Legacy API tests (unchanged) ===

    #[test]
    fn test_fetch_stage_default() {
        let stage = FetchStage::new();
        assert_eq!(stage.pc(), Addr::new(0));
    }

    #[test]
    fn test_fetch_instruction_legacy() {
        let mut bus = create_test_bus();
        bus.write_word(Addr::new(0), Word::new(0x12345678)).unwrap();

        let mut stage = FetchStage::new();
        let if_id = stage.execute(&bus, false, Addr::new(0), false).unwrap();

        assert_eq!(if_id.instruction, 0x12345678);
        assert!(if_id.valid);
        assert_eq!(stage.pc(), Addr::new(4));
    }

    #[test]
    fn test_fetch_stall_legacy() {
        let mut bus = create_test_bus();
        bus.write_word(Addr::new(0), Word::new(0x12345678)).unwrap();

        let mut stage = FetchStage::new();
        let _ = stage.execute(&bus, false, Addr::new(0), false).unwrap();

        let if_id = stage.execute(&bus, true, Addr::new(0), false).unwrap();
        assert!(!if_id.valid);
        assert_eq!(stage.pc(), Addr::new(4));
    }

    #[test]
    fn test_fetch_branch_legacy() {
        let mut bus = create_test_bus();
        bus.write_word(Addr::new(0x100), Word::new(0xABCDEF00))
            .unwrap();

        let mut stage = FetchStage::new();
        let if_id = stage.execute(&bus, false, Addr::new(0x100), true).unwrap();

        assert_eq!(if_id.pc, Addr::new(0x100));
        assert_eq!(if_id.instruction, 0xABCDEF00);
    }
}
