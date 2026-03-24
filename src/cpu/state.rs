//! CPU state representation.
//!
//! This module provides a snapshot of the CPU state for debugging
//! and DiffTest purposes.

use crate::types::{Addr, PrivilegeLevel, Word};
use crate::cpu::{ProgramCounter, Registers};
use std::fmt;

/// CPU state snapshot.
///
/// This structure captures the complete state of the CPU at a point in time,
/// useful for debugging and DiffTest comparisons.
#[derive(Debug, Clone, PartialEq)]
pub struct CpuState {
    /// Program counter
    pub pc: Addr,
    /// General-purpose registers
    pub regs: [u32; 32],
    /// Current privilege level
    pub privilege: PrivilegeLevel,
    /// Number of instructions executed
    pub instructions_executed: u64,
    /// Whether the CPU is halted
    pub halted: bool,
}

impl CpuState {
    /// Create a new CPU state with default values.
    pub fn new() -> Self {
        Self {
            pc: Addr::new(0),
            regs: [0; 32],
            privilege: PrivilegeLevel::Machine,
            instructions_executed: 0,
            halted: false,
        }
    }

    /// Create a CPU state from the current CPU.
    pub fn from_cpu(pc: &ProgramCounter, regs: &Registers, privilege: PrivilegeLevel, instructions_executed: u64) -> Self {
        Self {
            pc: pc.get(),
            regs: regs.as_slice().try_into().unwrap(),
            privilege,
            instructions_executed,
            halted: false,
        }
    }

    /// Get the PC value.
    pub fn pc(&self) -> Addr {
        self.pc
    }

    /// Get a register value.
    pub fn reg(&self, idx: usize) -> Word {
        Word::new(self.regs[idx])
    }

    /// Check if the CPU is halted.
    pub fn is_halted(&self) -> bool {
        self.halted
    }

    /// Mark the CPU as halted.
    pub fn halt(&mut self) {
        self.halted = true;
    }

    /// Reset the CPU state.
    pub fn reset(&mut self) {
        self.pc = Addr::new(0);
        self.regs.fill(0);
        self.privilege = PrivilegeLevel::Machine;
        self.instructions_executed = 0;
        self.halted = false;
    }
}

impl Default for CpuState {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CpuState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "CPU State:")?;
        writeln!(f, "  PC: {}", self.pc)?;
        writeln!(f, "  Privilege: {}", self.privilege)?;
        writeln!(f, "  Instructions: {}", self.instructions_executed)?;
        writeln!(f, "  Halted: {}", self.halted)?;
        writeln!(f, "  Registers:")?;
        for i in 0..32 {
            writeln!(f, "    x{:02}: 0x{:08x}", i, self.regs[i])?;
        }
        Ok(())
    }
}

/// Comparison level for DiffTest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonLevel {
    /// Compare only PC and registers
    Minimal,
    /// Compare PC, registers, and privilege level
    Standard,
    /// Compare all CPU state including counters
    Full,
}

impl CpuState {
    /// Compare this state with another at the given comparison level.
    pub fn compare(&self, other: &CpuState, level: ComparisonLevel) -> bool {
        match level {
            ComparisonLevel::Minimal => {
                self.pc == other.pc && self.regs == other.regs
            }
            ComparisonLevel::Standard => {
                self.pc == other.pc
                    && self.regs == other.regs
                    && self.privilege == other.privilege
            }
            ComparisonLevel::Full => self == other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_state_default() {
        let state = CpuState::new();
        assert_eq!(state.pc(), Addr::new(0));
        assert_eq!(state.reg(0), Word::ZERO);
        assert!(!state.is_halted());
    }

    #[test]
    fn test_cpu_state_halt() {
        let mut state = CpuState::new();
        assert!(!state.is_halted());
        state.halt();
        assert!(state.is_halted());
    }

    #[test]
    fn test_cpu_state_compare() {
        let mut state1 = CpuState::new();
        let mut state2 = CpuState::new();

        assert!(state1.compare(&state2, ComparisonLevel::Full));

        state1.pc = Addr::new(0x1000);
        assert!(!state1.compare(&state2, ComparisonLevel::Minimal));

        state2.pc = Addr::new(0x1000);
        assert!(state1.compare(&state2, ComparisonLevel::Minimal));
    }
}
