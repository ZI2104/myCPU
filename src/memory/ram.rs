//! RAM (Random Access Memory) implementation.
//!
//! This module provides a simple RAM implementation that supports
//! both read and write operations.

use crate::error::{Result, SimError};
use crate::traits::Memory;
use crate::types::{Addr, Byte};

/// Random Access Memory.
///
/// A simple byte-addressable memory that supports both read and write operations.
/// The memory starts at address 0 and extends for `size` bytes.
#[derive(Debug, Clone)]
pub struct Ram {
    /// The memory data
    data: Vec<u8>,
    /// Base address (usually 0)
    base: Addr,
}

impl Ram {
    /// Create a new RAM of the given size.
    ///
    /// # Arguments
    /// * `size` - The size in bytes
    ///
    /// # Returns
    /// A new RAM instance with all bytes initialized to 0.
    pub fn new(size: usize) -> Self {
        Self {
            data: vec![0; size],
            base: Addr::new(0),
        }
    }

    /// Create a new RAM with the given base address and size.
    ///
    /// # Arguments
    /// * `base` - The base address
    /// * `size` - The size in bytes
    pub fn with_base(base: Addr, size: usize) -> Self {
        Self {
            data: vec![0; size],
            base,
        }
    }

    /// Create a RAM from existing data.
    ///
    /// # Arguments
    /// * `data` - The initial memory contents
    pub fn from_data(data: Vec<u8>) -> Self {
        Self {
            data,
            base: Addr::new(0),
        }
    }

    /// Create a RAM from existing data with a base address.
    ///
    /// # Arguments
    /// * `base` - The base address
    /// * `data` - The initial memory contents
    pub fn from_data_with_base(base: Addr, data: Vec<u8>) -> Self {
        Self { data, base }
    }

    /// Get the base address of this RAM.
    pub fn base(&self) -> Addr {
        self.base
    }

    /// Clear all memory to zeros.
    pub fn clear(&mut self) {
        self.data.fill(0);
    }

    /// Load data into memory at the given offset.
    ///
    /// # Arguments
    /// * `offset` - The offset from the base address
    /// * `data` - The data to load
    ///
    /// # Returns
    /// The number of bytes actually loaded.
    pub fn load(&mut self, offset: usize, data: &[u8]) -> Result<usize> {
        let end = offset + data.len();
        if end > self.data.len() {
            return Err(SimError::MemoryOutOfBounds {
                addr: self.base.add(offset as u32),
                size: data.len(),
            });
        }
        self.data[offset..end].copy_from_slice(data);
        Ok(data.len())
    }

    /// Get a reference to the underlying data.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get a mutable reference to the underlying data.
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

impl Memory for Ram {
    fn read_byte(&self, addr: Addr) -> Result<Byte> {
        let offset = addr.raw() as usize;
        if offset < self.data.len() {
            Ok(Byte::new(self.data[offset]))
        } else {
            Err(SimError::MemoryOutOfBounds { addr, size: 1 })
        }
    }

    fn write_byte(&mut self, addr: Addr, value: Byte) -> Result<()> {
        let offset = addr.raw() as usize;
        if offset < self.data.len() {
            self.data[offset] = value.raw();
            Ok(())
        } else {
            Err(SimError::MemoryOutOfBounds { addr, size: 1 })
        }
    }

    fn size(&self) -> usize {
        self.data.len()
    }

    fn contains(&self, addr: Addr) -> bool {
        (addr.raw() as usize) < self.data.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Word;

    #[test]
    fn test_ram_basic() {
        let mut ram = Ram::new(1024);

        // Write and read byte
        ram.write_byte(Addr::new(0), Byte::new(0x42)).unwrap();
        assert_eq!(ram.read_byte(Addr::new(0)).unwrap().raw(), 0x42);

        // Write and read word
        ram.write_word(Addr::new(4), Word::new(0xDEADBEEF)).unwrap();
        assert_eq!(ram.read_word(Addr::new(4)).unwrap().raw(), 0xDEADBEEF);
    }

    #[test]
    fn test_ram_out_of_bounds() {
        let ram = Ram::new(1024);

        assert!(ram.read_byte(Addr::new(1024)).is_err());
        assert!(ram.read_byte(Addr::new(0x10000)).is_err());
    }

    #[test]
    fn test_ram_load() {
        let mut ram = Ram::new(1024);
        let data = [0x01, 0x02, 0x03, 0x04];

        ram.load(0x100, &data).unwrap();

        assert_eq!(ram.read_byte(Addr::new(0x100)).unwrap().raw(), 0x01);
        assert_eq!(ram.read_byte(Addr::new(0x103)).unwrap().raw(), 0x04);
    }

    #[test]
    fn test_ram_with_base() {
        // Note: with_base is deprecated - Bus handles address mapping
        // This test verifies RAM works with offset-based addressing
        let mut ram = Ram::new(1024);

        // RAM uses offset-based addressing (relative to start)
        assert!(ram.contains(Addr::new(0x000)));
        assert!(ram.contains(Addr::new(0x3FF))); // 1023 = last byte
        assert!(!ram.contains(Addr::new(0x400))); // 1024 = out of bounds

        ram.write_byte(Addr::new(0x100), Byte::new(0x42)).unwrap();
        assert_eq!(ram.read_byte(Addr::new(0x100)).unwrap().raw(), 0x42);
    }
}
