//! Program Counter implementation.
//!
//! This module provides the program counter (PC) for the RISC-V CPU.

use crate::types::Addr;
use std::fmt;

/// Program Counter.
///
/// The PC holds the address of the next instruction to be executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProgramCounter {
    /// The current PC value
    value: Addr,
}

impl ProgramCounter {
    /// Create a new PC with the given initial value.
    pub fn new(value: Addr) -> Self {
        Self { value }
    }

    /// Create a PC starting at address 0.
    pub fn zero() -> Self {
        Self {
            value: Addr::new(0),
        }
    }

    /// Get the current PC value.
    pub fn get(&self) -> Addr {
        self.value
    }

    /// Set the PC to a new value.
    pub fn set(&mut self, value: Addr) {
        self.value = value;
    }

    /// Increment the PC by 4 (one instruction).
    pub fn increment(&mut self) {
        self.value = self.value.add(4);
    }

    /// Jump to a new address.
    ///
    /// This is equivalent to `set`, but provided for clarity.
    pub fn jump(&mut self, target: Addr) {
        self.value = target;
    }

    /// Branch to a new address with a signed offset.
    pub fn branch(&mut self, offset: i32) {
        self.value = self.value.add_signed(offset);
    }
}

impl fmt::Display for ProgramCounter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PC: {}", self.value)
    }
}

impl From<Addr> for ProgramCounter {
    fn from(value: Addr) -> Self {
        Self::new(value)
    }
}

impl From<ProgramCounter> for Addr {
    fn from(pc: ProgramCounter) -> Self {
        pc.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pc_basic() {
        let mut pc = ProgramCounter::new(Addr::new(0x1000));
        assert_eq!(pc.get(), Addr::new(0x1000));

        pc.increment();
        assert_eq!(pc.get(), Addr::new(0x1004));
    }

    #[test]
    fn test_pc_jump() {
        let mut pc = ProgramCounter::new(Addr::new(0x1000));
        pc.jump(Addr::new(0x2000));
        assert_eq!(pc.get(), Addr::new(0x2000));
    }

    #[test]
    fn test_pc_branch() {
        let mut pc = ProgramCounter::new(Addr::new(0x1000));

        // Forward branch
        pc.branch(0x100);
        assert_eq!(pc.get(), Addr::new(0x1100));

        // Backward branch
        pc.branch(-0x200);
        assert_eq!(pc.get(), Addr::new(0x0F00));
    }
}
