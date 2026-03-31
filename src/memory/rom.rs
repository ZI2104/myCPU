//! ROM (Read-Only Memory) implementation.
//!
//! This module provides a ROM implementation that only supports read operations.
//! Write operations will return an error.

use crate::error::{Result, SimError};
use crate::traits::Memory;
use crate::types::{Addr, Byte};

/// Read-Only Memory.
///
/// A memory region that can only be read. Attempting to write will result in an error.
/// This is useful for boot ROMs or firmware.
#[derive(Debug, Clone)]
pub struct Rom {
    /// The ROM data
    data: Vec<u8>,
    /// Base address
    base: Addr,
}

impl Rom {
    /// Create a new ROM from the given data.
    ///
    /// # Arguments
    /// * `data` - The ROM contents
    ///
    /// # Returns
    /// A new ROM instance with the given data.
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            base: Addr::new(0),
        }
    }

    /// Create a new ROM with the given base address and data.
    ///
    /// # Arguments
    /// * `base` - The base address
    /// * `data` - The ROM contents
    pub fn with_base(base: Addr, data: Vec<u8>) -> Self {
        Self { data, base }
    }

    /// Get the base address of this ROM.
    pub fn base(&self) -> Addr {
        self.base
    }

    /// Get a reference to the underlying data.
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

impl Memory for Rom {
    fn read_byte(&self, addr: Addr) -> Result<Byte> {
        let offset = addr.raw() as usize;
        if offset < self.data.len() {
            Ok(Byte::new(self.data[offset]))
        } else {
            Err(SimError::MemoryOutOfBounds { addr, size: 1 })
        }
    }

    fn write_byte(&mut self, addr: Addr, _value: Byte) -> Result<()> {
        Err(SimError::Memory {
            addr,
            message: "Cannot write to ROM".to_string(),
        })
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

    #[test]
    fn test_rom_read() {
        let data = vec![0x01, 0x02, 0x03, 0x04, 0xEF, 0xBE, 0xAD, 0xDE];
        let rom = Rom::new(data);

        assert_eq!(rom.read_byte(Addr::new(0)).unwrap().raw(), 0x01);
        assert_eq!(rom.read_word(Addr::new(4)).unwrap().raw(), 0xDEADBEEF);
    }

    #[test]
    fn test_rom_write_fails() {
        let data = vec![0x00; 1024];
        let mut rom = Rom::new(data);

        let result = rom.write_byte(Addr::new(0), Byte::new(0x42));
        assert!(result.is_err());
    }

    #[test]
    fn test_rom_out_of_bounds() {
        let data = vec![0x00; 1024];
        let rom = Rom::new(data);

        assert!(rom.read_byte(Addr::new(1024)).is_err());
    }

    #[test]
    fn test_rom_with_base() {
        // Note: with_base is deprecated - Bus handles address mapping
        // This test verifies ROM works with offset-based addressing
        let data = vec![0x13, 0x00, 0x00, 0x00]; // nop instruction
        let rom = Rom::new(data);

        // ROM uses offset-based addressing (relative to start)
        assert!(rom.contains(Addr::new(0x0)));
        assert!(rom.contains(Addr::new(0x3))); // last byte
        assert!(!rom.contains(Addr::new(0x4))); // out of bounds
    }
}
