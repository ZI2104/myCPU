//! Peripheral trait definition.
//!
//! This module defines the `Peripheral` trait that abstracts peripheral
//! devices in the RISC-V simulator.

use crate::error::Result;
use crate::traits::Memory;
use crate::types::Addr;
use std::any::Any;

/// Peripheral device trait.
///
/// This trait defines the interface for peripheral devices that can be
/// attached to the system bus. Peripherals respond to memory-mapped I/O
/// operations within their address range.
///
/// The `Send` bound is required for async visualization support.
pub trait Peripheral: Send {
    /// Read a byte from the peripheral.
    ///
    /// # Arguments
    /// * `addr` - The offset within the peripheral's address space
    ///
    /// # Returns
    /// The byte value, or an error if the read fails.
    fn read(&self, offset: Addr) -> Result<u8>;

    /// Write a byte to the peripheral.
    ///
    /// # Arguments
    /// * `addr` - The offset within the peripheral's address space
    /// * `value` - The byte value to write
    ///
    /// # Returns
    /// Ok(()) on success, or an error if the write fails.
    fn write(&mut self, offset: Addr, value: u8) -> Result<()>;

    /// Get the base address of this peripheral in the system address space.
    fn base_addr(&self) -> Addr;

    /// Get the size of this peripheral's address space in bytes.
    fn size(&self) -> usize;

    /// Get the name of this peripheral for debugging.
    fn name(&self) -> &str;

    /// Check if an address falls within this peripheral's range.
    fn contains(&self, addr: Addr) -> bool {
        let base = self.base_addr().raw() as usize;
        let target = addr.raw() as usize;
        target >= base && target < base + self.size()
    }

    /// Convert an absolute address to an offset within this peripheral.
    ///
    /// # Returns
    /// None if the address is not within this peripheral's range.
    fn to_offset(&self, addr: Addr) -> Option<Addr> {
        if self.contains(addr) {
            Some(Addr::new(addr.raw() - self.base_addr().raw()))
        } else {
            None
        }
    }

    /// Check if this peripheral has a pending interrupt.
    ///
    /// This is used by the interrupt controller to poll peripherals.
    fn has_interrupt(&self) -> bool {
        false
    }

    /// Acknowledge and clear a pending interrupt.
    ///
    /// This is called when the CPU handles an interrupt from this peripheral.
    fn acknowledge_interrupt(&mut self) {}

    /// Downcast to Any for runtime type inspection.
    ///
    /// This allows downcasting to concrete peripheral types like CLINT.
    fn as_any(&self) -> &dyn Any {
        unreachable!("as_any must be implemented for peripherals that need downcasting")
    }

    /// Mutable downcast to Any for runtime type inspection.
    fn as_any_mut(&mut self) -> &mut dyn Any {
        unreachable!("as_any_mut must be implemented for peripherals that need downcasting")
    }

    /// Try to execute any pending DMA operation after a register write.
    ///
    /// Returns `true` if work was performed. The default does nothing.
    /// Accelerators (GPU, TPU, etc.) override this to trigger compute
    /// when the Bus detects a pending start or descriptor notification.
    fn try_execute_pending(
        &mut self,
        _ram_regions: &mut Vec<(Addr, usize, Box<dyn Memory>)>,
    ) -> Result<bool> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A simple test peripheral
    struct TestPeripheral {
        base: Addr,
        data: [u8; 16],
    }

    impl TestPeripheral {
        fn new(base: Addr) -> Self {
            Self {
                base,
                data: [0; 16],
            }
        }
    }

    impl Peripheral for TestPeripheral {
        fn read(&self, offset: Addr) -> Result<u8> {
            let idx = offset.raw() as usize;
            if idx < self.data.len() {
                Ok(self.data[idx])
            } else {
                Err(crate::error::SimError::Peripheral(
                    "Offset out of bounds".to_string(),
                ))
            }
        }

        fn write(&mut self, offset: Addr, value: u8) -> Result<()> {
            let idx = offset.raw() as usize;
            if idx < self.data.len() {
                self.data[idx] = value;
                Ok(())
            } else {
                Err(crate::error::SimError::Peripheral(
                    "Offset out of bounds".to_string(),
                ))
            }
        }

        fn base_addr(&self) -> Addr {
            self.base
        }

        fn size(&self) -> usize {
            self.data.len()
        }

        fn name(&self) -> &str {
            "TestPeripheral"
        }
    }

    #[test]
    fn test_peripheral_address_mapping() {
        let periph = TestPeripheral::new(Addr::new(0x1000));

        assert!(periph.contains(Addr::new(0x1000)));
        assert!(periph.contains(Addr::new(0x100F)));
        assert!(!periph.contains(Addr::new(0x0FFF)));
        assert!(!periph.contains(Addr::new(0x1010)));

        assert_eq!(periph.to_offset(Addr::new(0x1005)), Some(Addr::new(0x0005)));
        assert_eq!(periph.to_offset(Addr::new(0x2000)), None);
    }
}
