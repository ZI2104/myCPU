//! Interrupt source trait definition.
//!
//! This module defines the `InterruptSource` trait for peripherals
//! that can generate interrupts with detailed status information.

// No imports needed - trait uses only primitive types

/// Interrupt source trait.
///
/// This trait extends `Peripheral` with detailed interrupt status,
/// allowing the CPU to determine which specific interrupts are pending.
pub trait InterruptSource {
    /// Check if machine timer interrupt is pending (MTIP).
    ///
    /// For CLINT, this returns true when mtime >= mtimecmp.
    fn mtip(&self) -> bool;

    /// Check if machine software interrupt is pending (MSIP).
    ///
    /// For CLINT, this returns true when msip[0] is set.
    fn msip(&self) -> bool;

    /// Check if machine external interrupt is pending (MEIP).
    ///
    /// For PLIC, this returns true when there's a pending external interrupt.
    fn meip(&self) -> bool {
        false // Default: not supported
    }

    /// Get the interrupt cause code for the highest priority pending interrupt.
    ///
    /// Returns (is_interrupt, cause_code) if any, or None.
    /// Priority order is implementation-defined.
    fn highest_priority_interrupt(&self) -> Option<(bool, u32)> {
        // Default implementation checks in MEIP > MTIP > MSIP order
        // (implementation-defined priority)
        if self.meip() {
            return Some((true, 11)); // Machine External Interrupt
        }
        if self.mtip() {
            return Some((true, 7)); // Machine Timer Interrupt
        }
        if self.msip() {
            return Some((true, 3)); // Machine Software Interrupt
        }
        None
    }
}
