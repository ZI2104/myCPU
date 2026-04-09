//! CSR Register trait definition.
//!
//! This module provides the trait for Control and Status Registers.

use crate::types::PrivilegeLevel;

/// CSR Register trait.
///
/// All CSR registers must implement this trait to provide
/// consistent read/write behavior and privilege checking.
pub trait CsrRegister {
    /// Get the CSR address.
    fn address(&self) -> u16;

    /// Get the name of the CSR for debugging.
    fn name(&self) -> &'static str;

    /// Read the CSR value.
    fn read(&self) -> u32;

    /// Write to the CSR value.
    ///
    /// Some CSRs are read-only or have read-only fields.
    /// Implementations should handle this appropriately.
    fn write(&mut self, value: u32);

    /// Get the minimum privilege level required to access this CSR.
    fn min_privilege(&self) -> PrivilegeLevel;

    /// Check if this CSR is writable.
    ///
    /// Some CSRs (like misa) are read-only.
    fn is_writable(&self) -> bool {
        true
    }

    /// Set specific bits in the CSR (CSRRS instruction).
    ///
    /// Returns the old value before the operation.
    fn set_bits(&mut self, mask: u32) -> u32 {
        let old = self.read();
        self.write(old | mask);
        old
    }

    /// Clear specific bits in the CSR (CSRRC instruction).
    ///
    /// Returns the old value before the operation.
    fn clear_bits(&mut self, mask: u32) -> u32 {
        let old = self.read();
        self.write(old & !mask);
        old
    }
}

/// CSR access width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CsrWidth {
    /// 32-bit CSR (standard for RV32)
    #[default]
    Xlen32,
    /// 64-bit CSR (for timers in RV32)
    Xlen64,
}

/// Result of a CSR operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsrOpResult {
    /// Read operation completed, returns value
    Read(u32),
    /// Write operation completed
    Written,
    /// Read-Write operation completed, returns old value
    ReadWritten(u32),
}

/// CSR access check result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsrAccessCheck {
    /// Access is allowed
    Allowed,
    /// CSR does not exist
    NotFound,
    /// Access denied due to privilege level
    PrivilegeDenied,
    /// CSR is read-only but write was attempted
    ReadOnly,
}

/// Check if a CSR address exists and can be accessed.
pub fn check_csr_access(addr: u16, privilege: PrivilegeLevel, is_write: bool) -> CsrAccessCheck {
    // Determine minimum privilege level from address
    // CSR address[9:8] determines privilege:
    // 00 = User, 01 = Supervisor, 10 = Hypervisor, 11 = Machine
    let min_priv = match (addr >> 8) & 0x3 {
        0 => PrivilegeLevel::User,
        1 => PrivilegeLevel::Supervisor,
        2 => PrivilegeLevel::Machine, // Hypervisor not implemented, treat as Machine
        3 => PrivilegeLevel::Machine,
        _ => return CsrAccessCheck::NotFound,
    };

    // Check privilege level
    if privilege < min_priv {
        return CsrAccessCheck::PrivilegeDenied;
    }

    // Check read-only (CSR address[11:10] = 11 means read-only)
    // Actually, CSR address[11:10]:
    // 00 = read/write, 01 = read/write, 10 = read-only, 11 = read-only
    if is_write {
        let read_only = matches!((addr >> 10) & 0x3, 2 | 3);
        if read_only {
            return CsrAccessCheck::ReadOnly;
        }
    }

    CsrAccessCheck::Allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csr_access_check_machine() {
        // mstatus at 0x300 - Machine mode, should be accessible from M-mode
        assert_eq!(
            check_csr_access(0x300, PrivilegeLevel::Machine, false),
            CsrAccessCheck::Allowed
        );
        // Not accessible from User mode
        assert_eq!(
            check_csr_access(0x300, PrivilegeLevel::User, false),
            CsrAccessCheck::PrivilegeDenied
        );
    }

    #[test]
    fn test_csr_read_only() {
        // misa at 0x301 - read-only in bits [11:10] = 00, so writable
        // Actually for misa, it's at 0x301, bits [11:10] = 00, writable
        // For read-only test, we need an address with bits [11:10] = 10 or 11
        // e.g., 0xC00 - bits [11:10] = 11, read-only
        assert_eq!(
            check_csr_access(0xC00, PrivilegeLevel::Machine, true),
            CsrAccessCheck::ReadOnly
        );
        assert_eq!(
            check_csr_access(0xC00, PrivilegeLevel::Machine, false),
            CsrAccessCheck::Allowed
        );
    }
}
