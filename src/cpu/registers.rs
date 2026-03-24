//! General-purpose registers implementation.
//!
//! This module provides the 32 general-purpose registers (x0-x31)
//! for the RISC-V architecture.

use crate::types::{RegIdx, Word};
use std::fmt;

/// General-purpose registers (x0-x31).
///
/// The RISC-V architecture has 32 general-purpose registers.
/// Register x0 is always hardwired to 0.
#[derive(Debug, Clone, Default)]
pub struct Registers {
    /// The 32 general-purpose registers
    regs: [u32; 32],
}

impl Registers {
    /// Create a new register file with all registers initialized to 0.
    pub fn new() -> Self {
        Self { regs: [0; 32] }
    }

    /// Read a register value.
    ///
    /// # Note
    /// Reading x0 always returns 0, as per RISC-V specification.
    pub fn read(&self, idx: RegIdx) -> Word {
        if idx.is_zero() {
            Word::ZERO
        } else {
            Word::new(self.regs[idx.raw() as usize])
        }
    }

    /// Write a value to a register.
    ///
    /// # Note
    /// Writing to x0 is ignored, as per RISC-V specification.
    pub fn write(&mut self, idx: RegIdx, value: Word) {
        if !idx.is_zero() {
            self.regs[idx.raw() as usize] = value.raw();
        }
    }

    /// Get the raw register value (for internal use).
    ///
    /// # Safety
    /// This bypasses the x0=0 rule. Use `read` for normal access.
    pub fn raw(&self, idx: usize) -> u32 {
        self.regs[idx]
    }

    /// Set the raw register value (for internal use).
    ///
    /// # Safety
    /// This bypasses the x0=0 rule. Use `write` for normal access.
    pub fn set_raw(&mut self, idx: usize, value: u32) {
        self.regs[idx] = value;
    }

    /// Reset all registers to 0.
    pub fn reset(&mut self) {
        self.regs.fill(0);
    }

    /// Get a reference to all registers as a slice.
    pub fn as_slice(&self) -> &[u32] {
        &self.regs
    }

    /// Get ABI name for a register index.
    pub fn abi_name(idx: RegIdx) -> &'static str {
        match idx.raw() {
            0 => "zero",
            1 => "ra",
            2 => "sp",
            3 => "gp",
            4 => "tp",
            5 => "t0",
            6 => "t1",
            7 => "t2",
            8 => "s0",
            9 => "s1",
            10 => "a0",
            11 => "a1",
            12 => "a2",
            13 => "a3",
            14 => "a4",
            15 => "a5",
            16 => "a6",
            17 => "a7",
            18 => "s2",
            19 => "s3",
            20 => "s4",
            21 => "s5",
            22 => "s6",
            23 => "s7",
            24 => "s8",
            25 => "s9",
            26 => "s10",
            27 => "s11",
            28 => "t3",
            29 => "t4",
            30 => "t5",
            31 => "t6",
            _ => "invalid",
        }
    }
}

impl fmt::Display for Registers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Registers:")?;
        for i in 0..32u8 {
            let idx = RegIdx::new(i);
            writeln!(
                f,
                "  x{:02} ({:4}): 0x{:08x}",
                i,
                Self::abi_name(idx),
                self.regs[i as usize]
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_x0_always_zero() {
        let mut regs = Registers::new();

        // Reading x0 should return 0
        assert_eq!(regs.read(RegIdx::new(0)), Word::ZERO);

        // Writing to x0 should be ignored
        regs.write(RegIdx::new(0), Word::new(0xDEADBEEF));
        assert_eq!(regs.read(RegIdx::new(0)), Word::ZERO);
    }

    #[test]
    fn test_register_read_write() {
        let mut regs = Registers::new();

        regs.write(RegIdx::new(1), Word::new(0x12345678));
        assert_eq!(regs.read(RegIdx::new(1)).raw(), 0x12345678);

        regs.write(RegIdx::new(31), Word::new(0xDEADBEEF));
        assert_eq!(regs.read(RegIdx::new(31)).raw(), 0xDEADBEEF);
    }

    #[test]
    fn test_register_reset() {
        let mut regs = Registers::new();
        regs.write(RegIdx::new(1), Word::new(0x12345678));
        regs.reset();
        assert_eq!(regs.read(RegIdx::new(1)), Word::ZERO);
    }

    #[test]
    fn test_abi_names() {
        assert_eq!(Registers::abi_name(RegIdx::new(0)), "zero");
        assert_eq!(Registers::abi_name(RegIdx::new(1)), "ra");
        assert_eq!(Registers::abi_name(RegIdx::new(2)), "sp");
        assert_eq!(Registers::abi_name(RegIdx::new(10)), "a0");
    }
}
