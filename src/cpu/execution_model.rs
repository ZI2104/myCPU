//! Execution model abstraction.
//!
//! This module provides the `ExecutionModel` trait that abstracts different
//! CPU execution strategies (single-cycle, pipeline, etc.).

use crate::cpu::CpuState;
use crate::error::Result;
use crate::types::Addr;

/// Execution model abstraction for switching between single-cycle and pipeline.
///
/// This trait allows the simulator to switch between different execution
/// implementations while maintaining a consistent interface.
pub trait ExecutionModel {
    /// Execute one instruction (complete one cycle).
    ///
    /// For pipeline implementations, this advances all stages by one cycle.
    /// Returns the CPU state after the instruction commits.
    fn step(&mut self) -> Result<CpuState>;

    /// Reset the CPU to initial state.
    fn reset(&mut self);

    /// Get the current CPU state snapshot.
    fn state(&self) -> CpuState;

    /// Get the current PC value.
    fn pc(&self) -> Addr;

    /// Set the PC to a new value.
    fn set_pc(&mut self, addr: Addr);

    /// Check if the CPU is halted.
    fn is_halted(&self) -> bool;

    /// Halt the CPU.
    fn halt(&mut self);

    /// Get the number of instructions that have committed (completed).
    ///
    /// For pipeline, this counts instructions that have passed WB stage.
    fn instructions_executed(&self) -> u64;

    /// Run the CPU for a given number of instructions or until halted.
    ///
    /// # Arguments
    /// * `max_instructions` - Maximum number of instructions to execute (0 = unlimited)
    ///
    /// # Returns
    /// The number of instructions actually executed.
    fn run(&mut self, max_instructions: u64) -> Result<u64> {
        let mut count = 0u64;
        while !self.is_halted() {
            if max_instructions > 0 && count >= max_instructions {
                break;
            }
            self.step()?;
            count += 1;
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    // Tests will be added when implementations are created
}
