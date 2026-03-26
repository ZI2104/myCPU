//! Memory trait definition.
//!
//! This module defines the `Memory` trait that abstracts memory access
//! operations in the RISC-V simulator.

use crate::error::{check_alignment, Result};
use crate::types::{Addr, Byte, Half, Word};

/// Memory access trait.
///
/// This trait defines the interface for all memory-like components
/// in the simulator, including RAM, ROM, and memory-mapped peripherals.
pub trait Memory {
    /// Read a byte from memory.
    ///
    /// # Arguments
    /// * `addr` - The address to read from
    ///
    /// # Returns
    /// The byte at the given address, or an error if the access is invalid.
    fn read_byte(&self, addr: Addr) -> Result<Byte>;

    /// Write a byte to memory.
    ///
    /// # Arguments
    /// * `addr` - The address to write to
    /// * `value` - The byte value to write
    ///
    /// # Returns
    /// An error if the write fails (e.g., read-only memory).
    fn write_byte(&mut self, addr: Addr, value: Byte) -> Result<()>;

    /// Read a half-word (16 bits) from memory.
    ///
    /// # Arguments
    /// * `addr` - The address to read from (must be 2-byte aligned)
    ///
    /// # Returns
    /// The half-word at the given address in little-endian order.
    ///
    /// # Errors
    /// Returns `SimError::MemoryAlignment` if the address is not 2-byte aligned.
    fn read_half(&self, addr: Addr) -> Result<Half> {
        check_alignment(addr, 2)?;
        let low = self.read_byte(addr)?.raw();
        let high = self.read_byte(addr.add(1))?.raw();
        Ok(Half::new(((high as u16) << 8) | (low as u16)))
    }

    /// Write a half-word (16 bits) to memory.
    ///
    /// # Arguments
    /// * `addr` - The address to write to (must be 2-byte aligned)
    /// * `value` - The half-word value to write
    ///
    /// # Details
    /// Writes in little-endian order.
    ///
    /// # Errors
    /// Returns `SimError::MemoryAlignment` if the address is not 2-byte aligned.
    fn write_half(&mut self, addr: Addr, value: Half) -> Result<()> {
        check_alignment(addr, 2)?;
        self.write_byte(addr, Byte::new(value.raw() as u8))?;
        self.write_byte(addr.add(1), Byte::new((value.raw() >> 8) as u8))
    }

    /// Read a word (32 bits) from memory.
    ///
    /// # Arguments
    /// * `addr` - The address to read from (must be 4-byte aligned)
    ///
    /// # Returns
    /// The word at the given address in little-endian order.
    ///
    /// # Errors
    /// Returns `SimError::MemoryAlignment` if the address is not 4-byte aligned.
    fn read_word(&self, addr: Addr) -> Result<Word> {
        check_alignment(addr, 4)?;
        let b0 = self.read_byte(addr)?.raw() as u32;
        let b1 = self.read_byte(addr.add(1))?.raw() as u32;
        let b2 = self.read_byte(addr.add(2))?.raw() as u32;
        let b3 = self.read_byte(addr.add(3))?.raw() as u32;
        Ok(Word::new(b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)))
    }

    /// Write a word (32 bits) to memory.
    ///
    /// # Arguments
    /// * `addr` - The address to write to (must be 4-byte aligned)
    /// * `value` - The word value to write
    ///
    /// # Details
    /// Writes in little-endian order.
    ///
    /// # Errors
    /// Returns `SimError::MemoryAlignment` if the address is not 4-byte aligned.
    fn write_word(&mut self, addr: Addr, value: Word) -> Result<()> {
        check_alignment(addr, 4)?;
        let raw = value.raw();
        self.write_byte(addr, Byte::new(raw as u8))?;
        self.write_byte(addr.add(1), Byte::new((raw >> 8) as u8))?;
        self.write_byte(addr.add(2), Byte::new((raw >> 16) as u8))?;
        self.write_byte(addr.add(3), Byte::new((raw >> 24) as u8))
    }

    /// Read a slice of bytes from memory.
    ///
    /// # Arguments
    /// * `addr` - The starting address
    /// * `buf` - The buffer to read into
    ///
    /// # Returns
    /// The number of bytes actually read.
    fn read_bytes(&self, addr: Addr, buf: &mut [u8]) -> Result<usize> {
        for (i, byte) in buf.iter_mut().enumerate() {
            *byte = self.read_byte(addr.add(i as u32))?.raw();
        }
        Ok(buf.len())
    }

    /// Write a slice of bytes to memory.
    ///
    /// # Arguments
    /// * `addr` - The starting address
    /// * `data` - The data to write
    ///
    /// # Returns
    /// The number of bytes actually written.
    fn write_bytes(&mut self, addr: Addr, data: &[u8]) -> Result<usize> {
        for (i, &byte) in data.iter().enumerate() {
            self.write_byte(addr.add(i as u32), Byte::new(byte))?;
        }
        Ok(data.len())
    }

    /// Get the size of this memory region in bytes.
    fn size(&self) -> usize;

    /// Check if an address is within this memory's bounds.
    fn contains(&self, addr: Addr) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A simple test memory implementation
    struct TestMemory {
        data: Vec<u8>,
    }

    impl TestMemory {
        fn new(size: usize) -> Self {
            Self {
                data: vec![0; size],
            }
        }
    }

    impl Memory for TestMemory {
        fn read_byte(&self, addr: Addr) -> Result<Byte> {
            let idx: usize = addr.into();
            if idx < self.data.len() {
                Ok(Byte::new(self.data[idx]))
            } else {
                Err(crate::error::SimError::MemoryOutOfBounds {
                    addr,
                    size: 1,
                })
            }
        }

        fn write_byte(&mut self, addr: Addr, value: Byte) -> Result<()> {
            let idx: usize = addr.into();
            if idx < self.data.len() {
                self.data[idx] = value.raw();
                Ok(())
            } else {
                Err(crate::error::SimError::MemoryOutOfBounds {
                    addr,
                    size: 1,
                })
            }
        }

        fn size(&self) -> usize {
            self.data.len()
        }

        fn contains(&self, addr: Addr) -> bool {
            let idx: usize = addr.into();
            idx < self.data.len()
        }
    }

    #[test]
    fn test_memory_word_operations() {
        let mut mem = TestMemory::new(1024);
        let addr = Addr::new(0x100);

        // Write and read word
        mem.write_word(addr, Word::new(0xDEADBEEF)).unwrap();
        let word = mem.read_word(addr).unwrap();
        assert_eq!(word.raw(), 0xDEADBEEF);

        // Verify little-endian byte order
        assert_eq!(mem.read_byte(addr).unwrap().raw(), 0xEF);
        assert_eq!(mem.read_byte(addr.add(1)).unwrap().raw(), 0xBE);
        assert_eq!(mem.read_byte(addr.add(2)).unwrap().raw(), 0xAD);
        assert_eq!(mem.read_byte(addr.add(3)).unwrap().raw(), 0xDE);
    }

    #[test]
    fn test_memory_half_operations() {
        let mut mem = TestMemory::new(1024);
        let addr = Addr::new(0x100);

        // Write and read half-word
        mem.write_half(addr, Half::new(0x1234)).unwrap();
        let half = mem.read_half(addr).unwrap();
        assert_eq!(half.raw(), 0x1234);

        // Verify little-endian byte order
        assert_eq!(mem.read_byte(addr).unwrap().raw(), 0x34);
        assert_eq!(mem.read_byte(addr.add(1)).unwrap().raw(), 0x12);
    }
}
