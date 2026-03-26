//! Core types for the RISC-V simulator.
//!
//! This module defines the fundamental types used throughout the simulator,
//! including addresses, words, and other basic constructs.

use std::fmt;

/// A 32-bit address in the RISC-V memory space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Addr(pub u32);

impl Addr {
    /// Create a new address.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Get the raw address value.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Add an offset to the address.
    pub const fn add(self, offset: u32) -> Self {
        Self(self.0.wrapping_add(offset))
    }

    /// Add a signed offset to the address.
    pub const fn add_signed(self, offset: i32) -> Self {
        Self(self.0.wrapping_add(offset as u32))
    }

    /// Align the address down to the given alignment (must be power of 2).
    ///
    /// Uses bitwise AND with complement of (align - 1) to clear low bits.
    /// This works because powers of 2 have exactly one bit set.
    pub const fn align_down(self, align: u32) -> Self {
        Self(self.0 & !(align - 1))
    }

    /// Check if the address is aligned to the given alignment.
    pub const fn is_aligned(self, align: u32) -> bool {
        self.0.is_multiple_of(align)
    }
}

impl fmt::Display for Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:08x}", self.0)
    }
}

impl From<u32> for Addr {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<Addr> for u32 {
    fn from(addr: Addr) -> Self {
        addr.0
    }
}

impl From<Addr> for usize {
    fn from(addr: Addr) -> Self {
        addr.0 as usize
    }
}

/// A 32-bit word (data value).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Word(pub u32);

impl Word {
    /// Zero word.
    pub const ZERO: Word = Word(0);

    /// Create a new word.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Get the raw word value.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Sign-extend a byte to a word.
    pub const fn from_byte(value: u8) -> Self {
        Self(value as i8 as i32 as u32)
    }

    /// Sign-extend a half-word to a word.
    pub const fn from_half(value: u16) -> Self {
        Self(value as i16 as i32 as u32)
    }

    /// Zero-extend a byte to a word.
    pub const fn from_byte_zero(value: u8) -> Self {
        Self(value as u32)
    }

    /// Zero-extend a half-word to a word.
    pub const fn from_half_zero(value: u16) -> Self {
        Self(value as u32)
    }

    /// Get the low byte.
    pub const fn byte(self) -> u8 {
        self.0 as u8
    }

    /// Get the low half-word.
    pub const fn half(self) -> u16 {
        self.0 as u16
    }

    /// Get bits [7:0] as a byte.
    pub const fn bits_7_0(self) -> u8 {
        self.0 as u8
    }

    /// Get bits [15:8] as a byte.
    pub const fn bits_15_8(self) -> u8 {
        (self.0 >> 8) as u8
    }

    /// Get bits [23:16] as a byte.
    pub const fn bits_23_16(self) -> u8 {
        (self.0 >> 16) as u8
    }

    /// Get bits [31:24] as a byte.
    pub const fn bits_31_24(self) -> u8 {
        (self.0 >> 24) as u8
    }

    /// Interpret as signed value.
    pub const fn as_signed(self) -> i32 {
        self.0 as i32
    }
}

impl fmt::Display for Word {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:08x}", self.0)
    }
}

impl From<u32> for Word {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<Word> for u32 {
    fn from(word: Word) -> Self {
        word.0
    }
}

impl From<i32> for Word {
    fn from(value: i32) -> Self {
        Self(value as u32)
    }
}

/// A single byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Byte(pub u8);

impl Byte {
    /// Zero byte.
    pub const ZERO: Byte = Byte(0);

    /// Create a new byte.
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    /// Get the raw byte value.
    pub const fn raw(self) -> u8 {
        self.0
    }
}

impl fmt::Display for Byte {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:02x}", self.0)
    }
}

impl From<u8> for Byte {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl From<Byte> for u8 {
    fn from(byte: Byte) -> Self {
        byte.0
    }
}

/// A half-word (16 bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Half(pub u16);

impl Half {
    /// Zero half-word.
    pub const ZERO: Half = Half(0);

    /// Create a new half-word.
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    /// Get the raw half-word value.
    pub const fn raw(self) -> u16 {
        self.0
    }
}

impl fmt::Display for Half {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:04x}", self.0)
    }
}

impl From<u16> for Half {
    fn from(value: u16) -> Self {
        Self(value)
    }
}

impl From<Half> for u16 {
    fn from(half: Half) -> Self {
        half.0
    }
}

/// Register index (0-31).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegIdx(pub u8);

impl RegIdx {
    /// Create a new register index.
    ///
    /// # Panics
    /// Panics if the index is >= 32.
    pub const fn new(index: u8) -> Self {
        assert!(index < 32, "Register index must be < 32");
        Self(index)
    }

    /// Get the raw index value.
    pub const fn raw(self) -> u8 {
        self.0
    }

    /// Check if this is the zero register (x0).
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for RegIdx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "x{}", self.0)
    }
}

impl TryFrom<u8> for RegIdx {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value < 32 {
            Ok(Self(value))
        } else {
            Err("Register index must be < 32")
        }
    }
}

impl From<RegIdx> for usize {
    fn from(idx: RegIdx) -> Self {
        idx.0 as usize
    }
}

/// Privilege level in RISC-V.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PrivilegeLevel {
    /// User mode (U-mode)
    User = 0,
    /// Supervisor mode (S-mode)
    Supervisor = 1,
    /// Machine mode (M-mode)
    #[default]
    Machine = 3,
}

impl PrivilegeLevel {
    /// Get the numeric value of the privilege level.
    pub const fn bits(self) -> u8 {
        self as u8
    }

    /// Try to create from bits.
    pub const fn from_bits(bits: u8) -> Option<Self> {
        match bits {
            0 => Some(Self::User),
            1 => Some(Self::Supervisor),
            3 => Some(Self::Machine),
            _ => None,
        }
    }
}

impl fmt::Display for PrivilegeLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::User => write!(f, "U"),
            Self::Supervisor => write!(f, "S"),
            Self::Machine => write!(f, "M"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_addr_alignment() {
        let addr = Addr::new(0x1003);
        assert_eq!(addr.align_down(4), Addr::new(0x1000));
        assert!(!addr.is_aligned(4));
        assert!(Addr::new(0x1000).is_aligned(4));
    }

    #[test]
    fn test_word_sign_extension() {
        assert_eq!(Word::from_byte(0x7F).as_signed(), 127);
        assert_eq!(Word::from_byte(0x80).as_signed(), -128);
        assert_eq!(Word::from_half(0x7FFF).as_signed(), 32767);
        assert_eq!(Word::from_half(0x8000).as_signed(), -32768);
    }

    #[test]
    fn test_reg_idx() {
        assert!(RegIdx::new(0).is_zero());
        assert!(!RegIdx::new(1).is_zero());
    }

    #[test]
    fn test_privilege_level() {
        assert_eq!(PrivilegeLevel::from_bits(0), Some(PrivilegeLevel::User));
        assert_eq!(PrivilegeLevel::from_bits(3), Some(PrivilegeLevel::Machine));
        assert_eq!(PrivilegeLevel::from_bits(2), None);
    }
}
