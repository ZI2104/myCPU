//! User-mode CSR registers.
//!
//! This module implements the User-mode Control and Status Registers
//! for N extension (User-level interrupts).

use crate::types::Addr;

/// User-mode CSR addresses.
pub mod u_csr_addr {
    pub const USTATUS: u16 = 0x000;
    pub const UIE: u16 = 0x004;
    pub const UTVEC: u16 = 0x005;
    pub const USCRATCH: u16 = 0x040;
    pub const UEPC: u16 = 0x041;
    pub const UCAUSE: u16 = 0x042;
    pub const UTVAL: u16 = 0x043;
    pub const UIP: u16 = 0x044;
}

/// User Status Register (ustatus).
///
/// Minimal implementation for N extension.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ustatus {
    value: u32,
}

impl Ustatus {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn read(&self) -> u32 {
        self.value
    }

    pub fn write(&mut self, value: u32) {
        // Only UIE bit is writable
        self.value = value & 0x1;
    }

    pub fn uie(&self) -> bool {
        (self.value & 0x1) != 0
    }

    pub fn set_uie(&mut self, value: bool) {
        if value {
            self.value |= 0x1;
        } else {
            self.value &= !0x1;
        }
    }
}

/// User Interrupt Enable register (uie).
#[derive(Debug, Clone, Copy, Default)]
pub struct Uie {
    value: u32,
}

impl Uie {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn read(&self) -> u32 {
        self.value
    }

    pub fn write(&mut self, value: u32) {
        // Only USIE, UTIE, UEIE bits
        self.value = value & 0x111;
    }
}

/// User Trap Vector register (utvec).
#[derive(Debug, Clone, Copy, Default)]
pub struct Utvec {
    value: u32,
}

impl Utvec {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn read(&self) -> u32 {
        self.value
    }

    pub fn write(&mut self, value: u32) {
        self.value = value;
    }

    pub fn base(&self) -> Addr {
        Addr::new(self.value & !0x3)
    }

    pub fn mode(&self) -> u8 {
        (self.value & 0x3) as u8
    }
}

/// User Scratch register (uscratch).
#[derive(Debug, Clone, Copy, Default)]
pub struct Uscratch {
    value: u32,
}

impl Uscratch {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn read(&self) -> u32 {
        self.value
    }

    pub fn write(&mut self, value: u32) {
        self.value = value;
    }
}

/// User Exception PC register (uepc).
#[derive(Debug, Clone, Copy, Default)]
pub struct Uepc {
    value: u32,
}

impl Uepc {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn read(&self) -> u32 {
        self.value
    }

    pub fn write(&mut self, value: u32) {
        self.value = value & !0x3; // 4-byte aligned
    }

    pub fn get(&self) -> Addr {
        Addr::new(self.value)
    }

    pub fn set(&mut self, addr: Addr) {
        self.value = addr.raw();
    }
}

/// User Cause register (ucause).
#[derive(Debug, Clone, Copy, Default)]
pub struct Ucause {
    value: u32,
}

impl Ucause {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn read(&self) -> u32 {
        self.value
    }

    pub fn write(&mut self, value: u32) {
        self.value = value;
    }

    pub fn is_interrupt(&self) -> bool {
        (self.value >> 31) != 0
    }

    pub fn code(&self) -> u32 {
        self.value & 0x7FFFFFFF
    }

    pub fn set(&mut self, is_interrupt: bool, code: u32) {
        self.value = if is_interrupt { 1 << 31 } else { 0 } | (code & 0x7FFFFFFF);
    }
}

/// User Trap Value register (utval).
#[derive(Debug, Clone, Copy, Default)]
pub struct Utval {
    value: u32,
}

impl Utval {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn read(&self) -> u32 {
        self.value
    }

    pub fn write(&mut self, value: u32) {
        self.value = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ustatus() {
        let mut ustatus = Ustatus::new();
        assert!(!ustatus.uie());

        ustatus.set_uie(true);
        assert!(ustatus.uie());
        assert_eq!(ustatus.read(), 1);

        ustatus.write(0);
        assert!(!ustatus.uie());
    }

    #[test]
    fn test_uepc() {
        let mut uepc = Uepc::new();

        uepc.write(0x1004);
        assert_eq!(uepc.read(), 0x1004);

        // Test alignment
        uepc.write(0x1003);
        assert_eq!(uepc.read(), 0x1000);
    }
}
