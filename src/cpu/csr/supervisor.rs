//! Supervisor-mode CSR registers.
//!
//! This module implements the Supervisor-mode Control and Status Registers.

use crate::types::{Addr, PrivilegeLevel};

/// Supervisor-mode CSR addresses.
pub mod s_csr_addr {
    pub const SSTATUS: u16 = 0x100;
    pub const SIE: u16 = 0x104;
    pub const STVEC: u16 = 0x105;
    pub const SSCRATCH: u16 = 0x140;
    pub const SEPC: u16 = 0x141;
    pub const SCAUSE: u16 = 0x142;
    pub const STVAL: u16 = 0x143;
    pub const SIP: u16 = 0x144;
}

/// Supervisor Status Register (sstatus).
#[derive(Debug, Clone, Copy, Default)]
pub struct Sstatus {
    value: u32,
}

impl Sstatus {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn read(&self) -> u32 {
        self.value
    }

    pub fn write(&mut self, value: u32) {
        self.value = value & 0x0000DE133;
    }

    pub fn sie(&self) -> bool {
        (self.value & (1 << 1)) != 0
    }

    pub fn set_sie(&mut self, value: bool) {
        if value {
            self.value |= 1 << 1;
        } else {
            self.value &= !(1 << 1);
        }
    }

    pub fn spie(&self) -> bool {
        (self.value & (1 << 5)) != 0
    }

    pub fn set_spie(&mut self, value: bool) {
        if value {
            self.value |= 1 << 5;
        } else {
            self.value &= !(1 << 5);
        }
    }

    /// Supervisor Previous Privilege (SPP)
    /// Returns true if previous mode was Supervisor, false if User
    pub fn spp(&self) -> bool {
        (self.value & (1 << 8)) != 0
    }

    /// Set Supervisor Previous Privilege (SPP)
    pub fn set_spp(&mut self, privilege: PrivilegeLevel) {
        // SPP is 1 for Supervisor mode, 0 for User mode
        if privilege == PrivilegeLevel::Supervisor {
            self.value |= 1 << 8;
        } else {
            self.value &= !(1 << 8);
        }
    }
}

/// Supervisor Interrupt Enable register (sie).
#[derive(Debug, Clone, Copy, Default)]
pub struct Sie {
    value: u32,
}

impl Sie {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn read(&self) -> u32 {
        self.value
    }

    pub fn write(&mut self, value: u32) {
        self.value = value & 0x00000222;
    }

    /// Supervisor Software Interrupt Enable
    pub fn ssie(&self) -> bool {
        (self.value & (1 << 1)) != 0
    }

    /// Supervisor Timer Interrupt Enable
    pub fn stie(&self) -> bool {
        (self.value & (1 << 5)) != 0
    }

    /// Supervisor External Interrupt Enable
    pub fn seie(&self) -> bool {
        (self.value & (1 << 9)) != 0
    }
}

/// Supervisor Interrupt Pending register (sip).
#[derive(Debug, Clone, Copy, Default)]
pub struct Sip {
    value: u32,
}

impl Sip {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn read(&self) -> u32 {
        self.value
    }

    pub fn write(&mut self, value: u32) {
        // Only SSIP is writable, and only to clear it
        if value & (1 << 1) == 0 {
            self.value &= !(1 << 1);
        }
    }

    /// Supervisor Software Interrupt Pending
    pub fn ssip(&self) -> bool {
        (self.value & (1 << 1)) != 0
    }

    /// Supervisor Timer Interrupt Pending
    pub fn stip(&self) -> bool {
        (self.value & (1 << 5)) != 0
    }

    /// Supervisor External Interrupt Pending
    pub fn seip(&self) -> bool {
        (self.value & (1 << 9)) != 0
    }

    pub fn set_ssip(&mut self, pending: bool) {
        if pending {
            self.value |= 1 << 1;
        } else {
            self.value &= !(1 << 1);
        }
    }

    pub fn set_stip(&mut self, pending: bool) {
        if pending {
            self.value |= 1 << 5;
        } else {
            self.value &= !(1 << 5);
        }
    }

    pub fn set_seip(&mut self, pending: bool) {
        if pending {
            self.value |= 1 << 9;
        } else {
            self.value &= !(1 << 9);
        }
    }
}

/// Supervisor Trap Vector register (stvec).
#[derive(Debug, Clone, Copy, Default)]
pub struct Stvec {
    value: u32,
}

impl Stvec {
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

/// Supervisor Scratch register (sscratch).
#[derive(Debug, Clone, Copy, Default)]
pub struct Sscratch {
    value: u32,
}

impl Sscratch {
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

/// Supervisor Exception PC register (sepc).
#[derive(Debug, Clone, Copy, Default)]
pub struct Sepc {
    value: u32,
}

impl Sepc {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn read(&self) -> u32 {
        self.value
    }

    pub fn write(&mut self, value: u32) {
        self.value = value & !0x3;
    }

    pub fn get(&self) -> Addr {
        Addr::new(self.value)
    }

    pub fn set(&mut self, addr: Addr) {
        self.value = addr.raw();
    }
}

/// Supervisor Cause register (scause).
#[derive(Debug, Clone, Copy, Default)]
pub struct Scause {
    value: u32,
}

impl Scause {
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

/// Supervisor Trap Value register (stval).
#[derive(Debug, Clone, Copy, Default)]
pub struct Stval {
    value: u32,
}

impl Stval {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn read(&self) -> u32 {
        self.value
    }

    pub fn write(&mut self, value: u32) {
        self.value = value;
    }

    pub fn get(&self) -> u32 {
        self.value
    }

    pub fn set(&mut self, value: u32) {
        self.value = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sstatus_sie() {
        let mut sstatus = Sstatus::new();
        assert!(!sstatus.sie());

        sstatus.set_sie(true);
        assert!(sstatus.sie());

        sstatus.set_sie(false);
        assert!(!sstatus.sie());
    }

    #[test]
    fn test_sip_ssip() {
        let mut sip = Sip::new();

        sip.set_ssip(true);
        assert!(sip.ssip());

        sip.set_ssip(false);
        assert!(!sip.ssip());
    }
}
