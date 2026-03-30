//! Platform Level Interrupt Controller (PLIC) implementation.
//!
//! The PLIC manages external interrupts from peripherals and routes them
//! to harts based on priority and enable configuration.
//!
//! Memory map (QEMU virt machine compatible):
//! - 0x0C00_0000 - 0x0C00_0FFF: Priority registers (4 bytes per source)
//! - 0x0C00_1000 - 0x0C00_107F: Pending registers (bit per source)
//! - 0x0C00_2000 - 0x0C20_FFFF: Enable/Threshold/Claim registers
//!
//! For hart 0:
//! - 0x0C00_2000: Enable bits for sources 0-31
//! - 0x0C20_0000: Priority threshold
//! - 0x0C20_0004: Claim/Complete register

use crate::error::Result;
use crate::traits::{Memory, Peripheral};
use crate::types::{Addr, Byte, Half, Word};

/// PLIC base address (QEMU virt machine).
pub const PLIC_BASE: u32 = 0x0C00_0000;

/// PLIC size.
pub const PLIC_SIZE: usize = 0x0040_0000;

/// Maximum number of interrupt sources.
pub const MAX_SOURCES: usize = 1024;

/// Number of supported harts.
pub const MAX_HARTS: usize = 1;

// Register offsets
/// Priority register base offset.
pub const PRIORITY_BASE: u32 = 0x0000;
/// Pending register base offset.
pub const PENDING_BASE: u32 = 0x1000;
/// Enable register base offset per hart.
pub const ENABLE_BASE: u32 = 0x2000;
/// Context base offset (threshold + claim) per hart.
pub const CONTEXT_BASE: u32 = 0x200_000;
/// Threshold register offset within context.
pub const THRESHOLD_OFFSET: u32 = 0x0000;
/// Claim/Complete register offset within context.
pub const CLAIM_OFFSET: u32 = 0x0004;

/// PLIC (Platform Level Interrupt Controller).
///
/// Manages external interrupts for a single hart.
#[derive(Debug, Clone)]
pub struct Plic {
    /// Interrupt priority for each source (0 = disabled).
    priority: [u32; MAX_SOURCES],
    /// Pending interrupt bits.
    pending: [u32; 32], // 1024 bits = 32 words
    /// Enable bits for each hart.
    enable: [[u32; 32]; MAX_HARTS],
    /// Priority threshold for each hart.
    threshold: [u32; MAX_HARTS],
    /// Claimed interrupt ID for each hart (during claim process).
    claimed: [u32; MAX_HARTS],
    /// Base address.
    base: Addr,
}

impl Default for Plic {
    fn default() -> Self {
        Self::new()
    }
}

impl Plic {
    /// Create a new PLIC instance.
    pub fn new() -> Self {
        Self {
            priority: [0; MAX_SOURCES],
            pending: [0; 32],
            enable: [[0; 32]; MAX_HARTS],
            threshold: [0; MAX_HARTS],
            claimed: [0; MAX_HARTS],
            base: Addr::new(PLIC_BASE),
        }
    }

    /// Create a PLIC with a custom base address.
    pub fn with_base(base: Addr) -> Self {
        let mut plic = Self::new();
        plic.base = base;
        plic
    }

    /// Get the base address.
    pub fn base(&self) -> Addr {
        self.base
    }

    /// Set an interrupt source as pending.
    ///
    /// This is called by peripherals to raise an interrupt.
    pub fn set_pending(&mut self, source: usize) {
        if source > 0 && source < MAX_SOURCES {
            let word = source / 32;
            let bit = source % 32;
            self.pending[word] |= 1 << bit;
        }
    }

    /// Clear a pending interrupt.
    pub fn clear_pending(&mut self, source: usize) {
        if source > 0 && source < MAX_SOURCES {
            let word = source / 32;
            let bit = source % 32;
            self.pending[word] &= !(1 << bit);
        }
    }

    /// Check if an interrupt is pending.
    pub fn is_pending(&self, source: usize) -> bool {
        if source > 0 && source < MAX_SOURCES {
            let word = source / 32;
            let bit = source % 32;
            (self.pending[word] & (1 << bit)) != 0
        } else {
            false
        }
    }

    /// Set priority for an interrupt source.
    pub fn set_priority(&mut self, source: usize, priority: u32) {
        if source < MAX_SOURCES {
            self.priority[source] = priority & 0x7; // 3-bit priority
        }
    }

    /// Get priority for an interrupt source.
    pub fn get_priority(&self, source: usize) -> u32 {
        if source < MAX_SOURCES {
            self.priority[source]
        } else {
            0
        }
    }

    /// Enable/disable an interrupt source for a hart.
    pub fn set_enable(&mut self, hart: usize, source: usize, enabled: bool) {
        if hart < MAX_HARTS && source > 0 && source < MAX_SOURCES {
            let word = source / 32;
            let bit = source % 32;
            if enabled {
                self.enable[hart][word] |= 1 << bit;
            } else {
                self.enable[hart][word] &= !(1 << bit);
            }
        }
    }

    /// Check if an interrupt source is enabled for a hart.
    pub fn is_enabled(&self, hart: usize, source: usize) -> bool {
        if hart < MAX_HARTS && source > 0 && source < MAX_SOURCES {
            let word = source / 32;
            let bit = source % 32;
            (self.enable[hart][word] & (1 << bit)) != 0
        } else {
            false
        }
    }

    /// Set priority threshold for a hart.
    pub fn set_threshold(&mut self, hart: usize, threshold: u32) {
        if hart < MAX_HARTS {
            self.threshold[hart] = threshold & 0x7;
        }
    }

    /// Get priority threshold for a hart.
    pub fn get_threshold(&self, hart: usize) -> u32 {
        if hart < MAX_HARTS {
            self.threshold[hart]
        } else {
            0
        }
    }

    /// Check if there's a pending enabled interrupt above threshold.
    ///
    /// Returns true if MEIP should be asserted.
    pub fn has_pending_interrupt(&self, hart: usize) -> bool {
        if hart >= MAX_HARTS {
            return false;
        }

        for source in 1..MAX_SOURCES {
            if self.is_pending(source) && self.is_enabled(hart, source) {
                let priority = self.get_priority(source);
                if priority > self.threshold[hart] {
                    return true;
                }
            }
        }
        false
    }

    /// Claim the highest priority pending interrupt.
    ///
    /// Returns the interrupt source ID, or 0 if none.
    pub fn claim(&mut self, hart: usize) -> u32 {
        if hart >= MAX_HARTS {
            return 0;
        }

        let mut best_source = 0;
        let mut best_priority = 0;

        for source in 1..MAX_SOURCES {
            if self.is_pending(source) && self.is_enabled(hart, source) {
                let priority = self.get_priority(source);
                if priority > self.threshold[hart] && priority > best_priority {
                    best_priority = priority;
                    best_source = source;
                }
            }
        }

        if best_source > 0 {
            // Clear pending bit when claimed
            self.clear_pending(best_source);
            self.claimed[hart] = best_source as u32;
        }

        best_source as u32
    }

    /// Complete (finish processing) an interrupt.
    ///
    /// This should be called after handling the interrupt.
    pub fn complete(&mut self, hart: usize, source: u32) {
        if hart < MAX_HARTS && source > 0 && (source as usize) < MAX_SOURCES {
            // Signal completion - in real PLIC this allows the same interrupt to be re-raised
            if self.claimed[hart] == source {
                self.claimed[hart] = 0;
            }
        }
    }

    /// Read a PLIC register.
    fn read_register(&self, addr: Addr) -> Result<u32> {
        let offset = addr.raw() - PLIC_BASE;

        // Priority registers (source 0 has no priority, starts at offset 0)
        if (PRIORITY_BASE..PENDING_BASE).contains(&offset) {
            let source = ((offset - PRIORITY_BASE) / 4) as usize;
            if source < MAX_SOURCES {
                return Ok(self.priority[source]);
            }
        }

        // Pending registers
        if (PENDING_BASE..ENABLE_BASE).contains(&offset) {
            let word = ((offset - PENDING_BASE) / 4) as usize;
            if word < 32 {
                return Ok(self.pending[word]);
            }
        }

        // Enable registers for hart 0
        if (ENABLE_BASE..ENABLE_BASE + 0x80).contains(&offset) {
            let word = ((offset - ENABLE_BASE) / 4) as usize;
            if word < 32 {
                return Ok(self.enable[0][word]);
            }
        }

        // Context registers for hart 0
        if (CONTEXT_BASE..CONTEXT_BASE + 0x1000).contains(&offset) {
            let ctx_offset = offset - CONTEXT_BASE;
            match ctx_offset {
                THRESHOLD_OFFSET => return Ok(self.threshold[0]),
                CLAIM_OFFSET => {
                    // Reading claim returns the claimed interrupt
                    return Ok(self.claimed[0]);
                }
                _ => {}
            }
        }

        Ok(0)
    }

    /// Write a PLIC register.
    fn write_register(&mut self, addr: Addr, value: u32) -> Result<()> {
        let offset = addr.raw() - PLIC_BASE;

        // Priority registers
        if (PRIORITY_BASE..PENDING_BASE).contains(&offset) {
            let source = ((offset - PRIORITY_BASE) / 4) as usize;
            if source < MAX_SOURCES {
                self.priority[source] = value & 0x7;
                return Ok(());
            }
        }

        // Pending registers - read-only, ignore writes

        // Enable registers for hart 0
        if (ENABLE_BASE..ENABLE_BASE + 0x80).contains(&offset) {
            let word = ((offset - ENABLE_BASE) / 4) as usize;
            if word < 32 {
                self.enable[0][word] = value;
                return Ok(());
            }
        }

        // Context registers for hart 0
        if (CONTEXT_BASE..CONTEXT_BASE + 0x1000).contains(&offset) {
            let ctx_offset = offset - CONTEXT_BASE;
            match ctx_offset {
                THRESHOLD_OFFSET => {
                    self.threshold[0] = value & 0x7;
                    return Ok(());
                }
                CLAIM_OFFSET => {
                    // Writing to claim performs complete
                    self.complete(0, value);
                    return Ok(());
                }
                _ => {}
            }
        }

        Ok(())
    }
}

impl Memory for Plic {
    fn read_byte(&self, addr: Addr) -> Result<Byte> {
        let offset = addr.raw() - PLIC_BASE;
        let word = self.read_register(Addr::new(PLIC_BASE + (offset & !0x3)))?;
        let byte_offset = offset & 0x3;
        Ok(Byte::new((word >> (byte_offset * 8)) as u8))
    }

    fn read_half(&self, addr: Addr) -> Result<Half> {
        let offset = addr.raw() - PLIC_BASE;
        let word = self.read_register(Addr::new(PLIC_BASE + (offset & !0x3)))?;
        let half_offset = offset & 0x2;
        Ok(Half::new((word >> (half_offset * 8)) as u16))
    }

    fn read_word(&self, addr: Addr) -> Result<Word> {
        let offset = addr.raw() - PLIC_BASE;
        self.read_register(Addr::new(PLIC_BASE + offset)).map(Word::new)
    }

    fn write_byte(&mut self, addr: Addr, value: Byte) -> Result<()> {
        let offset = addr.raw() - PLIC_BASE;
        let aligned_offset = offset & !0x3;
        let byte_offset = offset & 0x3;

        let word = self.read_register(Addr::new(PLIC_BASE + aligned_offset))?;
        let mask: u32 = !(0xFF << (byte_offset * 8));
        let new_word = (word & mask) | ((value.raw() as u32) << (byte_offset * 8));
        self.write_register(Addr::new(PLIC_BASE + aligned_offset), new_word)
    }

    fn write_half(&mut self, addr: Addr, value: Half) -> Result<()> {
        let offset = addr.raw() - PLIC_BASE;
        let aligned_offset = offset & !0x3;
        let half_offset = offset & 0x2;

        let word = self.read_register(Addr::new(PLIC_BASE + aligned_offset))?;
        let mask: u32 = !(0xFFFF << (half_offset * 8));
        let new_word = (word & mask) | ((value.raw() as u32) << (half_offset * 8));
        self.write_register(Addr::new(PLIC_BASE + aligned_offset), new_word)
    }

    fn write_word(&mut self, addr: Addr, value: Word) -> Result<()> {
        let offset = addr.raw() - PLIC_BASE;
        self.write_register(Addr::new(PLIC_BASE + offset), value.raw())
    }

    fn size(&self) -> usize {
        PLIC_SIZE
    }

    fn contains(&self, addr: Addr) -> bool {
        let base = self.base;
        let end = Addr::new(base.raw().saturating_add(PLIC_SIZE as u32));
        addr >= base && addr < end
    }
}

impl Peripheral for Plic {
    fn read(&self, offset: Addr) -> Result<u8> {
        let abs_addr = Addr::new(PLIC_BASE + offset.raw());
        self.read_byte(abs_addr).map(|b| b.raw())
    }

    fn write(&mut self, offset: Addr, value: u8) -> Result<()> {
        let abs_addr = Addr::new(PLIC_BASE + offset.raw());
        self.write_byte(abs_addr, Byte::new(value))
    }

    fn base_addr(&self) -> Addr {
        self.base
    }

    fn size(&self) -> usize {
        PLIC_SIZE
    }

    fn name(&self) -> &str {
        "PLIC"
    }

    fn has_interrupt(&self) -> bool {
        self.has_pending_interrupt(0)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plic_priority() {
        let mut plic = Plic::new();

        // Set priority for source 1
        plic.set_priority(1, 5);
        assert_eq!(plic.get_priority(1), 5);

        // Priority is 3-bit, so 8 becomes 0
        plic.set_priority(1, 8);
        assert_eq!(plic.get_priority(1), 0);
    }

    #[test]
    fn test_plic_pending() {
        let mut plic = Plic::new();

        // Set and check pending
        assert!(!plic.is_pending(1));
        plic.set_pending(1);
        assert!(plic.is_pending(1));

        // Clear pending
        plic.clear_pending(1);
        assert!(!plic.is_pending(1));
    }

    #[test]
    fn test_plic_enable() {
        let mut plic = Plic::new();

        // Enable source 1
        assert!(!plic.is_enabled(0, 1));
        plic.set_enable(0, 1, true);
        assert!(plic.is_enabled(0, 1));

        // Disable
        plic.set_enable(0, 1, false);
        assert!(!plic.is_enabled(0, 1));
    }

    #[test]
    fn test_plic_threshold() {
        let mut plic = Plic::new();

        plic.set_threshold(0, 3);
        assert_eq!(plic.get_threshold(0), 3);
    }

    #[test]
    fn test_plic_has_pending_interrupt() {
        let mut plic = Plic::new();

        // No interrupts initially
        assert!(!plic.has_pending_interrupt(0));

        // Set pending but not enabled
        plic.set_pending(1);
        plic.set_priority(1, 7);
        assert!(!plic.has_pending_interrupt(0));

        // Enable the interrupt
        plic.set_enable(0, 1, true);
        assert!(plic.has_pending_interrupt(0));

        // Set threshold above priority
        plic.set_threshold(0, 7);
        assert!(!plic.has_pending_interrupt(0));
    }

    #[test]
    fn test_plic_claim() {
        let mut plic = Plic::new();

        // Setup multiple interrupts
        plic.set_priority(1, 2);
        plic.set_priority(2, 5);
        plic.set_priority(3, 3);
        plic.set_enable(0, 1, true);
        plic.set_enable(0, 2, true);
        plic.set_enable(0, 3, true);
        plic.set_pending(1);
        plic.set_pending(2);
        plic.set_pending(3);

        // Claim should return highest priority (source 2)
        let claimed = plic.claim(0);
        assert_eq!(claimed, 2);

        // Source 2 should no longer be pending
        assert!(!plic.is_pending(2));
    }

    #[test]
    fn test_plic_complete() {
        let mut plic = Plic::new();

        plic.set_priority(1, 5);
        plic.set_enable(0, 1, true);
        plic.set_pending(1);

        let claimed = plic.claim(0);
        assert_eq!(claimed, 1);

        // Complete the interrupt
        plic.complete(0, claimed);

        // Claimed should be reset
        assert_eq!(plic.claimed[0], 0);
    }

    #[test]
    fn test_plic_memory_access() {
        let mut plic = Plic::new();

        // Write priority for source 1
        plic.write_word(Addr::new(PLIC_BASE + PRIORITY_BASE + 4), Word::new(5))
            .unwrap();
        assert_eq!(plic.get_priority(1), 5);

        // Read back
        let val = plic.read_word(Addr::new(PLIC_BASE + PRIORITY_BASE + 4)).unwrap();
        assert_eq!(val.raw(), 5);
    }
}
