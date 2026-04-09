//! Core Local Interruptor (CLINT) implementation.
//!
//! The CLINT provides:
//! - Machine-mode timer (mtime) - 64-bit timer
//! - Machine-mode timer compare (mtimecmp) - 64-bit compare register
//! - Software interrupts (msip) - per-hart software interrupt
//!
//! When mtime >= mtimecmp, a machine timer interrupt is triggered (MTIP bit in mip).
//!
//! Memory map (QEMU virt machine compatible):
//! - 0x0200_0000 - 0x0200_3FFF: msip (software interrupts, 4 bytes per hart)
//! - 0x0200_4000 - 0x0200_7FFF: mtimecmp (timer compare, 8 bytes per hart)
//! - 0x0200_BFF8 - 0x0200_BFFF: mtime (timer value, 64-bit)

use crate::error::{Result, SimError};
use crate::traits::{InterruptSource, Memory, Peripheral};
use crate::types::{Addr, Byte, Half, Word};

/// CLINT base address (QEMU virt machine).
pub const CLINT_BASE: u32 = 0x0200_0000;

/// CLINT size.
pub const CLINT_SIZE: usize = 0x0001_0000;

/// Offset for msip (software interrupt) registers.
pub const MSIP_OFFSET: u32 = 0x0000;

/// Offset for mtimecmp register.
pub const MTIMECMP_OFFSET: u32 = 0x4000;

/// Offset for mtime register.
pub const MTIME_OFFSET: u32 = 0xBFF8;

/// CLINT (Core Local Interruptor).
///
/// Provides timer and software interrupt functionality for a single hart.
#[derive(Debug, Clone)]
pub struct Clint {
    /// Machine software interrupt pending (msip).
    msip: u32,
    /// Machine timer compare register (mtimecmp) - 64-bit.
    mtimecmp: u64,
    /// Machine time register (mtime) - 64-bit.
    mtime: u64,
    /// Base address for this CLINT.
    base: Addr,
}

impl Default for Clint {
    fn default() -> Self {
        Self::new()
    }
}

impl Clint {
    /// Create a new CLINT instance.
    pub fn new() -> Self {
        Self {
            msip: 0,
            mtimecmp: u64::MAX, // Default: no timer interrupt
            mtime: 0,
            base: Addr::new(CLINT_BASE),
        }
    }

    /// Create a CLINT with a custom base address.
    pub fn with_base(base: Addr) -> Self {
        Self {
            msip: 0,
            mtimecmp: u64::MAX,
            mtime: 0,
            base,
        }
    }

    /// Get the base address.
    pub fn base(&self) -> Addr {
        self.base
    }

    /// Check if machine timer interrupt is pending.
    ///
    /// Returns true when mtime >= mtimecmp.
    pub fn mtip(&self) -> bool {
        self.mtime >= self.mtimecmp
    }

    /// Check if machine software interrupt is pending.
    pub fn msip(&self) -> bool {
        (self.msip & 0x1) != 0
    }

    /// Get the current mtime value.
    pub fn mtime(&self) -> u64 {
        self.mtime
    }

    /// Get the current mtimecmp value.
    pub fn mtimecmp(&self) -> u64 {
        self.mtimecmp
    }

    /// Set the mtimecmp value.
    pub fn set_mtimecmp(&mut self, value: u64) {
        self.mtimecmp = value;
    }

    /// Set the msip value.
    pub fn set_msip(&mut self, value: u32) {
        self.msip = value & 0x1;
    }

    /// Advance the timer by a given number of cycles.
    ///
    /// This is called by the CPU on each cycle.
    pub fn tick(&mut self, cycles: u64) {
        self.mtime = self.mtime.wrapping_add(cycles);
    }

    /// Build a memory alignment error for a CLINT offset.
    fn alignment_error(&self, offset: u32, size: usize, alignment: usize) -> SimError {
        SimError::MemoryAlignment {
            addr: Addr::new(self.base.raw().wrapping_add(offset)),
            size,
            alignment,
        }
    }

    /// Read from CLINT registers.
    fn read_register(&self, offset: u32, _width: u32) -> Result<u32> {
        match offset {
            // msip (offset 0x0000)
            o if (MSIP_OFFSET..MTIMECMP_OFFSET).contains(&o) => {
                // 4-byte aligned access to msip
                if o % 4 == 0 {
                    Ok(self.msip)
                } else {
                    Err(self.alignment_error(o, 4, 4))
                }
            }

            // mtimecmp (offset 0x4000-0x4007, 64-bit, little-endian)
            o if (MTIMECMP_OFFSET..MTIMECMP_OFFSET + 8).contains(&o) => match o - MTIMECMP_OFFSET {
                0 => Ok((self.mtimecmp & 0xFFFFFFFF) as u32),
                4 => Ok((self.mtimecmp >> 32) as u32),
                _ => Err(self.alignment_error(o, 4, 4)),
            },

            // mtime (offset 0xBFF8-0xBFFF, 64-bit, little-endian)
            o if (MTIME_OFFSET..MTIME_OFFSET + 8).contains(&o) => match o - MTIME_OFFSET {
                0 => Ok((self.mtime & 0xFFFFFFFF) as u32),
                4 => Ok((self.mtime >> 32) as u32),
                _ => Err(self.alignment_error(o, 4, 4)),
            },

            _ => Err(SimError::Peripheral(format!(
                "Invalid CLINT read at offset 0x{:04x}",
                offset
            ))),
        }
    }

    /// Write to CLINT registers.
    fn write_register(&mut self, offset: u32, value: u32) -> Result<()> {
        match offset {
            // msip (offset 0x0000) - only bit 0 is writable
            o if (MSIP_OFFSET..MTIMECMP_OFFSET).contains(&o) => {
                if o % 4 == 0 {
                    self.msip = value & 0x1;
                    Ok(())
                } else {
                    Err(self.alignment_error(o, 4, 4))
                }
            }

            // mtimecmp (offset 0x4000-0x4007, 64-bit)
            o if (MTIMECMP_OFFSET..MTIMECMP_OFFSET + 8).contains(&o) => {
                match o - MTIMECMP_OFFSET {
                    0 => {
                        let high = self.mtimecmp >> 32;
                        self.mtimecmp = (high << 32) | (value as u64);
                    }
                    4 => {
                        let low = self.mtimecmp & 0xFFFFFFFF;
                        self.mtimecmp = ((value as u64) << 32) | low;
                    }
                    _ => return Err(self.alignment_error(o, 4, 4)),
                }
                Ok(())
            }

            // mtime (offset 0xBFF8-0xBFFF, 64-bit)
            o if (MTIME_OFFSET..MTIME_OFFSET + 8).contains(&o) => {
                match o - MTIME_OFFSET {
                    0 => {
                        let high = self.mtime >> 32;
                        self.mtime = (high << 32) | (value as u64);
                    }
                    4 => {
                        let low = self.mtime & 0xFFFFFFFF;
                        self.mtime = ((value as u64) << 32) | low;
                    }
                    _ => return Err(self.alignment_error(o, 4, 4)),
                }
                Ok(())
            }

            _ => Err(SimError::Peripheral(format!(
                "Invalid CLINT write at offset 0x{:04x}",
                offset
            ))),
        }
    }
}

impl Memory for Clint {
    fn read_byte(&self, addr: Addr) -> Result<Byte> {
        let offset = addr.raw() - CLINT_BASE;
        let word = self.read_register(offset & !0x3, 4)?;
        let byte_offset = offset & 0x3;
        Ok(Byte::new((word >> (byte_offset * 8)) as u8))
    }

    fn read_half(&self, addr: Addr) -> Result<Half> {
        let offset = addr.raw() - CLINT_BASE;
        let word = self.read_register(offset & !0x3, 4)?;
        let half_offset = offset & 0x2;
        Ok(Half::new((word >> (half_offset * 8)) as u16))
    }

    fn read_word(&self, addr: Addr) -> Result<Word> {
        let offset = addr.raw() - CLINT_BASE;
        self.read_register(offset, 4).map(Word::new)
    }

    fn write_byte(&mut self, addr: Addr, value: Byte) -> Result<()> {
        let offset = addr.raw() - CLINT_BASE;
        let aligned_offset = offset & !0x3;
        let byte_offset = offset & 0x3;

        let word = self.read_register(aligned_offset, 4)?;
        let mask: u32 = !(0xFF << (byte_offset * 8));
        let new_word = (word & mask) | ((value.raw() as u32) << (byte_offset * 8));
        self.write_register(aligned_offset, new_word)
    }

    fn write_half(&mut self, addr: Addr, value: Half) -> Result<()> {
        let offset = addr.raw() - CLINT_BASE;
        let aligned_offset = offset & !0x3;
        let half_offset = offset & 0x2;

        let word = self.read_register(aligned_offset, 4)?;
        let mask: u32 = !(0xFFFF << (half_offset * 8));
        let new_word = (word & mask) | ((value.raw() as u32) << (half_offset * 8));
        self.write_register(aligned_offset, new_word)
    }

    fn write_word(&mut self, addr: Addr, value: Word) -> Result<()> {
        let offset = addr.raw() - CLINT_BASE;
        self.write_register(offset, value.raw())
    }

    fn size(&self) -> usize {
        CLINT_SIZE
    }

    fn contains(&self, addr: Addr) -> bool {
        let base = self.base;
        let end = Addr::new(base.raw().saturating_add(CLINT_SIZE as u32));
        addr >= base && addr < end
    }
}

impl Peripheral for Clint {
    fn read(&self, offset: Addr) -> Result<u8> {
        // Read from CLINT using absolute address
        let abs_addr = Addr::new(CLINT_BASE + offset.raw());
        self.read_byte(abs_addr).map(|b| b.raw())
    }

    fn write(&mut self, offset: Addr, value: u8) -> Result<()> {
        // Write to CLINT using absolute address
        let abs_addr = Addr::new(CLINT_BASE + offset.raw());
        self.write_byte(abs_addr, Byte::new(value))
    }

    fn base_addr(&self) -> Addr {
        self.base
    }

    fn size(&self) -> usize {
        CLINT_SIZE
    }

    fn name(&self) -> &str {
        "CLINT"
    }

    /// Downcast to Any for runtime type inspection.
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// Mutable downcast to Any.
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    /// Check if CLINT has a pending interrupt.
    ///
    /// Returns true if either:
    /// - Machine Timer Interrupt Pending (MTIP): mtime >= mtimecmp
    /// - Machine Software Interrupt Pending (MSIP): msip bit 0 is set
    fn has_interrupt(&self) -> bool {
        self.mtip() || self.msip()
    }

    /// Acknowledge the software interrupt by clearing MSIP.
    ///
    /// Note: Timer interrupt (MTIP) is not automatically cleared.
    /// Software must write to mtimecmp to clear it.
    fn acknowledge_interrupt(&mut self) {
        // Only clear software interrupt pending
        // Timer interrupt is cleared by writing mtimecmp
        self.msip = 0;
    }
}

impl InterruptSource for Clint {
    /// Check if machine timer interrupt is pending (MTIP).
    ///
    /// Returns true when mtime >= mtimecmp.
    fn mtip(&self) -> bool {
        self.mtime >= self.mtimecmp
    }

    /// Check if machine software interrupt is pending (MSIP).
    ///
    /// Returns true when msip[0] is set.
    fn msip(&self) -> bool {
        (self.msip & 0x1) != 0
    }

    /// Get the highest priority pending interrupt.
    ///
    /// Priority order: MTIP > MSIP (implementation-defined).
    fn highest_priority_interrupt(&self) -> Option<(bool, u32)> {
        // Timer interrupt has higher priority than software interrupt
        if self.mtip() {
            return Some((true, 7)); // Machine Timer Interrupt
        }
        if self.msip() {
            return Some((true, 3)); // Machine Software Interrupt
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clint_mtip() {
        let mut clint = Clint::new();

        // Initially mtimecmp = MAX, so no interrupt
        assert!(!clint.mtip());

        // Set mtimecmp to 1000
        clint.set_mtimecmp(1000);
        assert!(!clint.mtip());

        // Advance mtime to 1000
        clint.tick(1000);
        assert!(clint.mtip());

        // Advance past mtimecmp
        clint.tick(1);
        assert!(clint.mtip());
    }

    #[test]
    fn test_clint_msip() {
        let mut clint = Clint::new();

        assert!(!clint.msip());

        clint.set_msip(1);
        assert!(clint.msip());

        clint.set_msip(0);
        assert!(!clint.msip());
    }

    #[test]
    fn test_clint_memory_read_write_mtimecmp() {
        let mut clint = Clint::new();

        // Write mtimecmp low
        clint
            .write_word(
                Addr::new(CLINT_BASE + MTIMECMP_OFFSET),
                Word::new(0x12345678),
            )
            .unwrap();
        assert_eq!(clint.mtimecmp() & 0xFFFFFFFF, 0x12345678);

        // Write mtimecmp high
        clint
            .write_word(
                Addr::new(CLINT_BASE + MTIMECMP_OFFSET + 4),
                Word::new(0x87654321),
            )
            .unwrap();
        assert_eq!(clint.mtimecmp(), 0x87654321_12345678);

        // Read back
        let low = clint
            .read_word(Addr::new(CLINT_BASE + MTIMECMP_OFFSET))
            .unwrap();
        let high = clint
            .read_word(Addr::new(CLINT_BASE + MTIMECMP_OFFSET + 4))
            .unwrap();
        assert_eq!(low.raw(), 0x12345678);
        assert_eq!(high.raw(), 0x87654321);
    }

    #[test]
    fn test_clint_mtime() {
        let mut clint = Clint::new();

        // Write mtime
        clint
            .write_word(Addr::new(CLINT_BASE + MTIME_OFFSET), Word::new(0x100))
            .unwrap();
        assert_eq!(clint.mtime(), 0x100);

        // Tick
        clint.tick(100);
        assert_eq!(clint.mtime(), 0x164);
    }

    #[test]
    fn test_clint_byte_access() {
        let mut clint = Clint::new();

        // Write msip byte
        clint
            .write_byte(Addr::new(CLINT_BASE), Byte::new(1))
            .unwrap();
        assert!(clint.msip());

        clint
            .write_byte(Addr::new(CLINT_BASE), Byte::new(0))
            .unwrap();
        assert!(!clint.msip());
    }

    #[test]
    fn test_clint_interrupt_source_trait() {
        let mut clint = Clint::new();

        // No interrupts initially
        assert!(!clint.mtip());
        assert!(!clint.msip());
        assert!(clint.highest_priority_interrupt().is_none());

        // Set timer interrupt
        clint.set_mtimecmp(100);
        clint.tick(100);
        assert!(clint.mtip());
        assert!(!clint.msip());

        // Check highest priority interrupt
        let intr = clint.highest_priority_interrupt();
        assert!(intr.is_some());
        let (is_int, cause) = intr.unwrap();
        assert!(is_int);
        assert_eq!(cause, 7); // Machine Timer Interrupt

        // Set software interrupt too
        clint.set_msip(1);
        assert!(clint.msip());

        // Timer should still be highest priority
        let intr = clint.highest_priority_interrupt();
        assert!(intr.is_some());
        let (_, cause) = intr.unwrap();
        assert_eq!(cause, 7); // Timer still has priority
    }

    #[test]
    fn test_clint_peripheral_trait() {
        use crate::traits::Peripheral;

        let mut clint = Clint::new();

        // Test base_addr and size
        assert_eq!(clint.base_addr(), Addr::new(CLINT_BASE));
        assert_eq!(Peripheral::size(&clint), CLINT_SIZE);
        assert_eq!(clint.name(), "CLINT");

        // Test has_interrupt
        assert!(!clint.has_interrupt());

        clint.set_mtimecmp(10);
        clint.tick(10);
        assert!(clint.has_interrupt());

        // Test acknowledge_interrupt (only clears MSIP)
        clint.set_msip(1);
        clint.acknowledge_interrupt();
        assert!(!clint.msip()); // Software interrupt cleared
        assert!(clint.mtip()); // Timer interrupt still pending
    }

    #[test]
    fn test_clint_unaligned_word_access_errors() {
        let mut clint = Clint::new();

        let read_err = clint.read_word(Addr::new(CLINT_BASE + MTIMECMP_OFFSET + 2));
        assert!(matches!(read_err, Err(SimError::MemoryAlignment { .. })));

        let write_err = clint.write_word(
            Addr::new(CLINT_BASE + MTIME_OFFSET + 2),
            Word::new(0x1234_5678),
        );
        assert!(matches!(write_err, Err(SimError::MemoryAlignment { .. })));
    }
}
