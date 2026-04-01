//! Machine-mode CSR registers.
//!
//! This module implements the Machine-mode Control and Status Registers
//! as defined in the RISC-V Privileged Architecture specification.

use super::csr_trait::CsrRegister;
use crate::types::{Addr, PrivilegeLevel};

/// CSR address constants for Machine mode.
pub mod csr_addr {
    // Machine Information
    pub const MVENDORID: u16 = 0xF11;
    pub const MARCHID: u16 = 0xF12;
    pub const MIMPID: u16 = 0xF13;
    pub const MHARTID: u16 = 0xF14;

    // Machine Trap Setup
    pub const MSTATUS: u16 = 0x300;
    pub const MISA: u16 = 0x301;
    pub const MEDELEG: u16 = 0x302;
    pub const MIDELEG: u16 = 0x303;
    pub const MIE: u16 = 0x304;
    pub const MTVEC: u16 = 0x305;
    pub const MCOUNTEREN: u16 = 0x306;

    // Machine Trap Handling
    pub const MSCRATCH: u16 = 0x340;
    pub const MEPC: u16 = 0x341;
    pub const MCAUSE: u16 = 0x342;
    pub const MTVAL: u16 = 0x343;
    pub const MIP: u16 = 0x344;

    // Machine Memory Protection (optional)
    pub const PMPCFG0: u16 = 0x3A0;
    pub const PMPCFG1: u16 = 0x3A1;
    pub const PMPCFG2: u16 = 0x3A2;
    pub const PMPCFG3: u16 = 0x3A3;
    pub const PMPADDR0: u16 = 0x3B0;
}

// ============================================================================
// mstatus register
// ============================================================================

/// Machine Status Register (mstatus).
///
/// Layout for RV32:
/// [31]    SD     - State Dirty (read-only, = (FS == 11) | (XS == 11))
/// [30:23] WPRI   - Reserved (Write Preserve, Read Ignore)
/// [22]    TSR    - Trap SRET
/// [21]    TW     - Timeout Wait
/// [20]    TVM    - Trap Virtual Memory
/// [19:18] -      - WPRI
/// [17]    MPRV   - Modify PRiVilege
/// [16]    -      - WPRI
/// [15]    MXR    - Make eXecutable Readable
/// [14:13] -      - WPRI
/// [12:11] MPP    - Machine Previous Privilege (2 bits)
/// [10:9]  VS     - Vector Status (2 bits)
/// [8]     SPP    - Supervisor Previous Privilege
/// [7]     MPIE   - Machine Previous Interrupt Enable
/// [6]     UBE    - User Big Endian
/// [5]     SPIE   - Supervisor Previous Interrupt Enable
/// [4]     -      - WPRI
/// [3]     MIE    - Machine Interrupt Enable
/// [2]     -      - WPRI
/// [1]     SIE    - Supervisor Interrupt Enable
/// [0]     -      - WPRI
#[derive(Debug, Clone, Copy, Default)]
pub struct Mstatus {
    value: u32,
}

// mstatus bit positions and masks
mod mstatus_bits {
    pub const TSR: u32 = 1 << 22;
    pub const TW: u32 = 1 << 21;
    pub const TVM: u32 = 1 << 20;
    pub const MPRV: u32 = 1 << 17;
    pub const MXR: u32 = 1 << 19;
    pub const MPP_SHIFT: u32 = 11;
    pub const MPP_MASK: u32 = 0x3 << MPP_SHIFT;
    pub const VS_MASK: u32 = 0x3 << 9;
    pub const SPP: u32 = 1 << 8;
    pub const MPIE: u32 = 1 << 7;
    pub const UBE: u32 = 1 << 6;
    pub const SPIE: u32 = 1 << 5;
    pub const MIE: u32 = 1 << 3;
    pub const SIE: u32 = 1 << 1;

    // Writeable mask (bits that can be written)
    pub const WRITABLE: u32 =
        TSR | TW | TVM | MPRV | MXR | MPP_MASK | VS_MASK | SPP | MPIE | UBE | SPIE | MIE | SIE;
}

impl Mstatus {
    /// Create a new mstatus register with default value (0).
    pub fn new() -> Self {
        Self { value: 0 }
    }

    /// Get MIE (Machine Interrupt Enable) bit.
    pub fn mie(&self) -> bool {
        (self.value & mstatus_bits::MIE) != 0
    }

    /// Set MIE bit.
    pub fn set_mie(&mut self, value: bool) {
        if value {
            self.value |= mstatus_bits::MIE;
        } else {
            self.value &= !mstatus_bits::MIE;
        }
    }

    /// Get MPIE (Machine Previous Interrupt Enable) bit.
    pub fn mpie(&self) -> bool {
        (self.value & mstatus_bits::MPIE) != 0
    }

    /// Set MPIE bit.
    pub fn set_mpie(&mut self, value: bool) {
        if value {
            self.value |= mstatus_bits::MPIE;
        } else {
            self.value &= !mstatus_bits::MPIE;
        }
    }

    /// Get MPP (Machine Previous Privilege) field.
    pub fn mpp(&self) -> PrivilegeLevel {
        match (self.value >> mstatus_bits::MPP_SHIFT) & 0x3 {
            0 => PrivilegeLevel::User,
            1 => PrivilegeLevel::Supervisor,
            3 => PrivilegeLevel::Machine,
            _ => PrivilegeLevel::User, // Reserved, treat as User
        }
    }

    /// Set MPP field.
    pub fn set_mpp(&mut self, level: PrivilegeLevel) {
        let level_bits = level.bits() as u32;
        self.value =
            (self.value & !mstatus_bits::MPP_MASK) | (level_bits << mstatus_bits::MPP_SHIFT);
    }

    /// Get SIE (Supervisor Interrupt Enable) bit.
    pub fn sie(&self) -> bool {
        (self.value & mstatus_bits::SIE) != 0
    }

    /// Set SIE bit.
    pub fn set_sie(&mut self, value: bool) {
        if value {
            self.value |= mstatus_bits::SIE;
        } else {
            self.value &= !mstatus_bits::SIE;
        }
    }

    /// Get SPIE (Supervisor Previous Interrupt Enable) bit.
    pub fn spie(&self) -> bool {
        (self.value & mstatus_bits::SPIE) != 0
    }

    /// Set SPIE bit.
    pub fn set_spie(&mut self, value: bool) {
        if value {
            self.value |= mstatus_bits::SPIE;
        } else {
            self.value &= !mstatus_bits::SPIE;
        }
    }

    /// Get SPP (Supervisor Previous Privilege) bit.
    pub fn spp(&self) -> PrivilegeLevel {
        if (self.value & mstatus_bits::SPP) != 0 {
            PrivilegeLevel::Supervisor
        } else {
            PrivilegeLevel::User
        }
    }

    /// Set SPP bit.
    pub fn set_spp(&mut self, level: PrivilegeLevel) {
        if level == PrivilegeLevel::Supervisor {
            self.value |= mstatus_bits::SPP;
        } else {
            self.value &= !mstatus_bits::SPP;
        }
    }

    /// Get MPRV (Modify PRiVilege) bit.
    pub fn mprv(&self) -> bool {
        (self.value & mstatus_bits::MPRV) != 0
    }

    /// Get MXR (Make eXecutable Readable) bit.
    pub fn mxr(&self) -> bool {
        (self.value & mstatus_bits::MXR) != 0
    }
}

impl CsrRegister for Mstatus {
    fn address(&self) -> u16 {
        csr_addr::MSTATUS
    }

    fn name(&self) -> &'static str {
        "mstatus"
    }

    fn read(&self) -> u32 {
        self.value
    }

    fn write(&mut self, value: u32) {
        // Only write to writable fields, preserve reserved bits
        self.value = (self.value & !mstatus_bits::WRITABLE) | (value & mstatus_bits::WRITABLE);
    }

    fn min_privilege(&self) -> PrivilegeLevel {
        PrivilegeLevel::Machine
    }
}

// ============================================================================
// mtvec register
// ============================================================================

/// Machine Trap Vector register (mtvec).
///
/// [31:2] BASE - Trap vector base address (4-byte aligned)
/// [1:0] MODE  - Vector mode
///         00 = Direct (all exceptions set PC = BASE)
///         01 = Vectored (interrupts set PC = BASE + 4*cause)
#[derive(Debug, Clone, Copy, Default)]
pub struct Mtvec {
    value: u32,
}

/// Trap vector mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrapVectorMode {
    /// Direct mode: all traps set PC = BASE
    Direct = 0,
    /// Vectored mode: interrupts set PC = BASE + 4*cause
    Vectored = 1,
}

impl Mtvec {
    /// Create a new mtvec register.
    pub fn new() -> Self {
        Self { value: 0 }
    }

    /// Get the trap vector base address.
    pub fn base(&self) -> Addr {
        Addr::new(self.value & !0x3)
    }

    /// Set the trap vector base address.
    pub fn set_base(&mut self, addr: Addr) {
        self.value = (self.value & 0x3) | (addr.raw() & !0x3);
    }

    /// Get the trap vector mode.
    pub fn mode(&self) -> TrapVectorMode {
        match self.value & 0x3 {
            0 => TrapVectorMode::Direct,
            1 => TrapVectorMode::Vectored,
            _ => TrapVectorMode::Direct, // Reserved modes treated as Direct
        }
    }

    /// Set the trap vector mode.
    pub fn set_mode(&mut self, mode: TrapVectorMode) {
        self.value = (self.value & !0x3) | (mode as u32);
    }

    /// Calculate trap handler address for given cause.
    pub fn trap_address(&self, cause: u32, is_interrupt: bool) -> Addr {
        match self.mode() {
            TrapVectorMode::Direct => self.base(),
            TrapVectorMode::Vectored => {
                if is_interrupt {
                    Addr::new(self.base().raw().wrapping_add(cause * 4))
                } else {
                    // Exceptions always go to BASE in vectored mode
                    self.base()
                }
            }
        }
    }
}

impl CsrRegister for Mtvec {
    fn address(&self) -> u16 {
        csr_addr::MTVEC
    }

    fn name(&self) -> &'static str {
        "mtvec"
    }

    fn read(&self) -> u32 {
        self.value
    }

    fn write(&mut self, value: u32) {
        // MODE bits [1:0] only support 0 and 1, writing 2 or 3 is implementation-defined
        // We'll accept any value but only support Direct and Vectored
        self.value = value;
    }

    fn min_privilege(&self) -> PrivilegeLevel {
        PrivilegeLevel::Machine
    }
}

// ============================================================================
// mepc register
// ============================================================================

/// Machine Exception PC register (mepc).
///
/// Holds the address of the instruction that caused the trap,
/// or the instruction that was being executed when an interrupt occurred.
#[derive(Debug, Clone, Copy, Default)]
pub struct Mepc {
    value: u32,
}

impl Mepc {
    /// Create a new mepc register.
    pub fn new() -> Self {
        Self { value: 0 }
    }

    /// Get the exception PC value.
    pub fn get(&self) -> Addr {
        Addr::new(self.value)
    }

    /// Set the exception PC value.
    pub fn set(&mut self, addr: Addr) {
        self.value = addr.raw();
    }
}

impl CsrRegister for Mepc {
    fn address(&self) -> u16 {
        csr_addr::MEPC
    }

    fn name(&self) -> &'static str {
        "mepc"
    }

    fn read(&self) -> u32 {
        self.value
    }

    fn write(&mut self, value: u32) {
        // For RV32, bits [1:0] are always 0 (4-byte alignment)
        self.value = value & !0x3;
    }

    fn min_privilege(&self) -> PrivilegeLevel {
        PrivilegeLevel::Machine
    }
}

// ============================================================================
// mcause register
// ============================================================================

/// Machine Cause register (mcause).
///
/// [31]    Interrupt - 1 if interrupt, 0 if exception
/// [30:0]  Code      - Exception/interrupt code
#[derive(Debug, Clone, Copy, Default)]
pub struct Mcause {
    value: u32,
}

/// Exception cause codes (when Interrupt bit = 0).
pub mod exception_code {
    pub const INSTRUCTION_MISALIGNED: u32 = 0;
    pub const INSTRUCTION_ACCESS_FAULT: u32 = 1;
    pub const ILLEGAL_INSTRUCTION: u32 = 2;
    pub const BREAKPOINT: u32 = 3;
    pub const LOAD_MISALIGNED: u32 = 4;
    pub const LOAD_ACCESS_FAULT: u32 = 5;
    pub const STORE_MISALIGNED: u32 = 6;
    pub const STORE_ACCESS_FAULT: u32 = 7;
    pub const ECALL_USER: u32 = 8;
    pub const ECALL_SUPERVISOR: u32 = 9;
    pub const ECALL_HYPERVISOR: u32 = 10;
    pub const ECALL_MACHINE: u32 = 11;
    pub const INSTRUCTION_PAGE_FAULT: u32 = 12;
    pub const LOAD_PAGE_FAULT: u32 = 13;
    pub const STORE_PAGE_FAULT: u32 = 15;
}

/// Interrupt cause codes (when Interrupt bit = 1).
pub mod interrupt_code {
    pub const USER_SOFTWARE: u32 = 0;
    pub const SUPERVISOR_SOFTWARE: u32 = 1;
    pub const MACHINE_SOFTWARE: u32 = 3;
    pub const USER_TIMER: u32 = 4;
    pub const SUPERVISOR_TIMER: u32 = 5;
    pub const MACHINE_TIMER: u32 = 7;
    pub const USER_EXTERNAL: u32 = 8;
    pub const SUPERVISOR_EXTERNAL: u32 = 9;
    pub const MACHINE_EXTERNAL: u32 = 11;
}

impl Mcause {
    /// Create a new mcause register.
    pub fn new() -> Self {
        Self { value: 0 }
    }

    /// Check if this is an interrupt (vs exception).
    pub fn is_interrupt(&self) -> bool {
        (self.value >> 31) != 0
    }

    /// Get the cause code.
    pub fn code(&self) -> u32 {
        self.value & 0x7FFFFFFF
    }

    /// Set the cause value.
    pub fn set(&mut self, is_interrupt: bool, code: u32) {
        self.value = if is_interrupt { 1 << 31 } else { 0 } | (code & 0x7FFFFFFF);
    }

    /// Set exception cause.
    pub fn set_exception(&mut self, code: u32) {
        self.set(false, code);
    }

    /// Set interrupt cause.
    pub fn set_interrupt(&mut self, code: u32) {
        self.set(true, code);
    }

    /// Get ecall cause code based on privilege level.
    pub fn ecall_code(privilege: PrivilegeLevel) -> u32 {
        match privilege {
            PrivilegeLevel::User => exception_code::ECALL_USER,
            PrivilegeLevel::Supervisor => exception_code::ECALL_SUPERVISOR,
            PrivilegeLevel::Machine => exception_code::ECALL_MACHINE,
        }
    }
}

impl CsrRegister for Mcause {
    fn address(&self) -> u16 {
        csr_addr::MCAUSE
    }

    fn name(&self) -> &'static str {
        "mcause"
    }

    fn read(&self) -> u32 {
        self.value
    }

    fn write(&mut self, value: u32) {
        self.value = value;
    }

    fn min_privilege(&self) -> PrivilegeLevel {
        PrivilegeLevel::Machine
    }
}

// ============================================================================
// mtval register
// ============================================================================

/// Machine Trap Value register (mtval).
///
/// Holds additional information about the trap:
/// - For memory access faults: the faulting address
/// - For illegal instruction: the instruction bits
/// - For other exceptions: 0
#[derive(Debug, Clone, Copy, Default)]
pub struct Mtval {
    value: u32,
}

impl Mtval {
    /// Create a new mtval register.
    pub fn new() -> Self {
        Self { value: 0 }
    }

    /// Get the trap value.
    pub fn get(&self) -> u32 {
        self.value
    }

    /// Set the trap value.
    pub fn set(&mut self, value: u32) {
        self.value = value;
    }
}

impl CsrRegister for Mtval {
    fn address(&self) -> u16 {
        csr_addr::MTVAL
    }

    fn name(&self) -> &'static str {
        "mtval"
    }

    fn read(&self) -> u32 {
        self.value
    }

    fn write(&mut self, value: u32) {
        self.value = value;
    }

    fn min_privilege(&self) -> PrivilegeLevel {
        PrivilegeLevel::Machine
    }
}

// ============================================================================
// mie register
// ============================================================================

/// Machine Interrupt Enable register (mie).
///
/// Controls which interrupts can trigger.
#[derive(Debug, Clone, Copy, Default)]
pub struct Mie {
    value: u32,
}

/// Interrupt enable bit positions.
pub mod ie_bits {
    pub const USIE: u32 = 1 << 0; // User Software Interrupt Enable
    pub const SSIE: u32 = 1 << 1; // Supervisor Software Interrupt Enable
    pub const MSIE: u32 = 1 << 3; // Machine Software Interrupt Enable
    pub const UTIE: u32 = 1 << 4; // User Timer Interrupt Enable
    pub const STIE: u32 = 1 << 5; // Supervisor Timer Interrupt Enable
    pub const MTIE: u32 = 1 << 7; // Machine Timer Interrupt Enable
    pub const UEIE: u32 = 1 << 8; // User External Interrupt Enable
    pub const SEIE: u32 = 1 << 9; // Supervisor External Interrupt Enable
    pub const MEIE: u32 = 1 << 11; // Machine External Interrupt Enable
}

impl Mie {
    /// Create a new mie register.
    pub fn new() -> Self {
        Self { value: 0 }
    }

    /// Check if machine software interrupt is enabled.
    pub fn msie(&self) -> bool {
        (self.value & ie_bits::MSIE) != 0
    }

    /// Check if machine timer interrupt is enabled.
    pub fn mtie(&self) -> bool {
        (self.value & ie_bits::MTIE) != 0
    }

    /// Check if machine external interrupt is enabled.
    pub fn meie(&self) -> bool {
        (self.value & ie_bits::MEIE) != 0
    }

    /// Check if supervisor software interrupt is enabled.
    pub fn ssie(&self) -> bool {
        (self.value & ie_bits::SSIE) != 0
    }

    /// Check if supervisor timer interrupt is enabled.
    pub fn stie(&self) -> bool {
        (self.value & ie_bits::STIE) != 0
    }

    /// Check if supervisor external interrupt is enabled.
    pub fn seie(&self) -> bool {
        (self.value & ie_bits::SEIE) != 0
    }
}

impl CsrRegister for Mie {
    fn address(&self) -> u16 {
        csr_addr::MIE
    }

    fn name(&self) -> &'static str {
        "mie"
    }

    fn read(&self) -> u32 {
        self.value
    }

    fn write(&mut self, value: u32) {
        // Only bits for implemented interrupts are writable
        let writable_mask = ie_bits::MSIE
            | ie_bits::MTIE
            | ie_bits::MEIE
            | ie_bits::SSIE
            | ie_bits::STIE
            | ie_bits::SEIE
            | ie_bits::USIE
            | ie_bits::UTIE
            | ie_bits::UEIE;
        self.value = value & writable_mask;
    }

    fn min_privilege(&self) -> PrivilegeLevel {
        PrivilegeLevel::Machine
    }
}

// ============================================================================
// mip register
// ============================================================================

/// Machine Interrupt Pending register (mip).
///
/// Shows which interrupts are pending.
#[derive(Debug, Clone, Copy, Default)]
pub struct Mip {
    value: u32,
}

/// Interrupt pending bit positions (same layout as mie).
pub mod ip_bits {
    pub const USIP: u32 = 1 << 0; // User Software Interrupt Pending
    pub const SSIP: u32 = 1 << 1; // Supervisor Software Interrupt Pending
    pub const MSIP: u32 = 1 << 3; // Machine Software Interrupt Pending
    pub const UTIP: u32 = 1 << 4; // User Timer Interrupt Pending
    pub const STIP: u32 = 1 << 5; // Supervisor Timer Interrupt Pending
    pub const MTIP: u32 = 1 << 7; // Machine Timer Interrupt Pending
    pub const UEIP: u32 = 1 << 8; // User External Interrupt Pending
    pub const SEIP: u32 = 1 << 9; // Supervisor External Interrupt Pending
    pub const MEIP: u32 = 1 << 11; // Machine External Interrupt Pending
}

impl Mip {
    /// Create a new mip register.
    pub fn new() -> Self {
        Self { value: 0 }
    }

    /// Check if machine software interrupt is pending.
    pub fn msip(&self) -> bool {
        (self.value & ip_bits::MSIP) != 0
    }

    /// Check if machine timer interrupt is pending.
    pub fn mtip(&self) -> bool {
        (self.value & ip_bits::MTIP) != 0
    }

    /// Check if machine external interrupt is pending.
    pub fn meip(&self) -> bool {
        (self.value & ip_bits::MEIP) != 0
    }

    /// Check if supervisor software interrupt is pending.
    pub fn ssip(&self) -> bool {
        (self.value & ip_bits::SSIP) != 0
    }

    /// Check if supervisor timer interrupt is pending.
    pub fn stip(&self) -> bool {
        (self.value & ip_bits::STIP) != 0
    }

    /// Check if supervisor external interrupt is pending.
    pub fn seip(&self) -> bool {
        (self.value & ip_bits::SEIP) != 0
    }

    /// Set machine timer interrupt pending.
    pub fn set_mtip(&mut self, pending: bool) {
        if pending {
            self.value |= ip_bits::MTIP;
        } else {
            self.value &= !ip_bits::MTIP;
        }
    }

    /// Set machine software interrupt pending.
    pub fn set_msip(&mut self, pending: bool) {
        if pending {
            self.value |= ip_bits::MSIP;
        } else {
            self.value &= !ip_bits::MSIP;
        }
    }

    /// Set machine external interrupt pending.
    pub fn set_meip(&mut self, pending: bool) {
        if pending {
            self.value |= ip_bits::MEIP;
        } else {
            self.value &= !ip_bits::MEIP;
        }
    }

    /// Set supervisor software interrupt pending.
    pub fn set_ssip(&mut self, pending: bool) {
        if pending {
            self.value |= ip_bits::SSIP;
        } else {
            self.value &= !ip_bits::SSIP;
        }
    }

    /// Set supervisor timer interrupt pending.
    pub fn set_stip(&mut self, pending: bool) {
        if pending {
            self.value |= ip_bits::STIP;
        } else {
            self.value &= !ip_bits::STIP;
        }
    }

    /// Set supervisor external interrupt pending.
    pub fn set_seip(&mut self, pending: bool) {
        if pending {
            self.value |= ip_bits::SEIP;
        } else {
            self.value &= !ip_bits::SEIP;
        }
    }

    /// Check if any machine interrupt is pending and enabled.
    pub fn has_pending_interrupt(&self, mie: &Mie) -> bool {
        (self.mtip() && mie.mtie()) || (self.msip() && mie.msie()) || (self.meip() && mie.meie())
    }

    /// Get the highest priority pending interrupt.
    /// Returns (is_interrupt, cause_code) if any, or None.
    pub fn highest_priority_interrupt(&self, mie: &Mie) -> Option<(bool, u32)> {
        // Priority order: MEI > MTI > MSI (implementation defined)
        if self.meip() && mie.meie() {
            return Some((true, interrupt_code::MACHINE_EXTERNAL));
        }
        if self.mtip() && mie.mtie() {
            return Some((true, interrupt_code::MACHINE_TIMER));
        }
        if self.msip() && mie.msie() {
            return Some((true, interrupt_code::MACHINE_SOFTWARE));
        }
        None
    }
}

impl CsrRegister for Mip {
    fn address(&self) -> u16 {
        csr_addr::MIP
    }

    fn name(&self) -> &'static str {
        "mip"
    }

    fn read(&self) -> u32 {
        self.value
    }

    fn write(&mut self, value: u32) {
        // Software-writable pending bits in M-mode: SSIP/STIP/SEIP.
        // MTIP/MSIP/MEIP are sourced from CLINT/PLIC and kept read-only here.
        let writable = ip_bits::SSIP | ip_bits::STIP | ip_bits::SEIP;
        self.value = (self.value & !writable) | (value & writable);
    }

    fn min_privilege(&self) -> PrivilegeLevel {
        PrivilegeLevel::Machine
    }
}

// ============================================================================
// mideleg register (Machine Interrupt Delegation)
// ============================================================================

/// Machine Interrupt Delegation register (mideleg).
///
/// Controls which interrupts are delegated to S-mode.
/// When a bit is set, the corresponding interrupt is handled in S-mode.
///
/// Bit layout:
/// [2]     SSIP  - Delegate Supervisor Software Interrupt
/// [6]     STIP  - Delegate Supervisor Timer Interrupt
/// [10]    SEIP  - Delegate Supervisor External Interrupt
/// [Other] WPRI  - Reserved (Write Preserve, Read Ignore)
#[derive(Debug, Clone, Copy, Default)]
pub struct Mideleg {
    value: u32,
}

/// mideleg bit positions
pub mod mideleg_bits {
    /// Supervisor Software Interrupt Delegation
    pub const SSIP: u32 = 1 << 2;
    /// Supervisor Timer Interrupt Delegation
    pub const STIP: u32 = 1 << 6;
    /// Supervisor External Interrupt Delegation
    pub const SEIP: u32 = 1 << 10;
    /// All valid delegation bits
    pub const ALL: u32 = SSIP | STIP | SEIP;
}

impl Mideleg {
    /// Create a new mideleg register (no delegation by default).
    pub fn new() -> Self {
        Self { value: 0 }
    }

    /// Check if supervisor software interrupt is delegated.
    pub fn ssip(&self) -> bool {
        (self.value & mideleg_bits::SSIP) != 0
    }

    /// Check if supervisor timer interrupt is delegated.
    pub fn stip(&self) -> bool {
        (self.value & mideleg_bits::STIP) != 0
    }

    /// Check if supervisor external interrupt is delegated.
    pub fn seip(&self) -> bool {
        (self.value & mideleg_bits::SEIP) != 0
    }

    /// Check if a specific interrupt cause should be delegated.
    pub fn is_delegated(&self, interrupt_cause: u32) -> bool {
        match interrupt_cause {
            interrupt_code::SUPERVISOR_SOFTWARE => self.ssip(),
            interrupt_code::SUPERVISOR_TIMER => self.stip(),
            interrupt_code::SUPERVISOR_EXTERNAL => self.seip(),
            _ => false,
        }
    }
}

impl CsrRegister for Mideleg {
    fn address(&self) -> u16 {
        csr_addr::MIDELEG
    }

    fn name(&self) -> &'static str {
        "mideleg"
    }

    fn read(&self) -> u32 {
        self.value & mideleg_bits::ALL
    }

    fn write(&mut self, value: u32) {
        self.value = value & mideleg_bits::ALL;
    }

    fn min_privilege(&self) -> PrivilegeLevel {
        PrivilegeLevel::Machine
    }
}

// ============================================================================
// medeleg register (Machine Exception Delegation)
// ============================================================================

/// Machine Exception Delegation register (medeleg).
///
/// Controls which exceptions are delegated to S-mode.
/// When a bit is set, the corresponding exception is handled in S-mode.
///
/// Bit layout (one bit per exception type):
/// [0]  IAM  - Instruction Address Misaligned
/// [1]  IAF  - Instruction Access Fault
/// [2]  ILGL - Illegal Instruction
/// [3]  BKPT - Breakpoint
/// [4]  LAM  - Load Address Misaligned
/// [5]  LAF  - Load Access Fault
/// [6]  SAM  - Store/AMO Address Misaligned
/// [7]  SAF  - Store/AMO Access Fault
/// [8]  UECL - User ECALL
/// [9]  SECL - Supervisor ECALL
/// [10] HECL - Hypervisor ECALL (reserved)
/// [11] MPFI - Instruction Page Fault
/// [12] MPFL - Load Page Fault
/// [13] WPRI - Reserved
/// [14] WPRI - Reserved
/// [15] SPFS - Store Page Fault
#[derive(Debug, Clone, Copy, Default)]
pub struct Medeleg {
    value: u32,
}

/// medeleg bit positions
pub mod medeleg_bits {
    /// Instruction Address Misaligned
    pub const IAM: u32 = 1 << 0;
    /// Instruction Access Fault
    pub const IAF: u32 = 1 << 1;
    /// Illegal Instruction
    pub const ILGL: u32 = 1 << 2;
    /// Breakpoint
    pub const BKPT: u32 = 1 << 3;
    /// Load Address Misaligned
    pub const LAM: u32 = 1 << 4;
    /// Load Access Fault
    pub const LAF: u32 = 1 << 5;
    /// Store/AMO Address Misaligned
    pub const SAM: u32 = 1 << 6;
    /// Store/AMO Access Fault
    pub const SAF: u32 = 1 << 7;
    /// Environment Call from U-mode
    pub const UECL: u32 = 1 << 8;
    /// Environment Call from S-mode
    pub const SECL: u32 = 1 << 9;
    /// Instruction Page Fault
    pub const IPFI: u32 = 1 << 12;
    /// Load Page Fault
    pub const LPFL: u32 = 1 << 13;
    /// Store Page Fault
    pub const SPFS: u32 = 1 << 15;
    /// All valid delegation bits
    pub const ALL: u32 =
        IAM | IAF | ILGL | BKPT | LAM | LAF | SAM | SAF | UECL | SECL | IPFI | LPFL | SPFS;
}

impl Medeleg {
    /// Create a new medeleg register (no delegation by default).
    pub fn new() -> Self {
        Self { value: 0 }
    }

    /// Check if a specific exception cause should be delegated.
    pub fn is_delegated(&self, exception_cause: u32) -> bool {
        if exception_cause > 31 {
            return false;
        }
        (self.value & (1 << exception_cause)) != 0
    }
}

impl CsrRegister for Medeleg {
    fn address(&self) -> u16 {
        csr_addr::MEDELEG
    }

    fn name(&self) -> &'static str {
        "medeleg"
    }

    fn read(&self) -> u32 {
        self.value & medeleg_bits::ALL
    }

    fn write(&mut self, value: u32) {
        self.value = value & medeleg_bits::ALL;
    }

    fn min_privilege(&self) -> PrivilegeLevel {
        PrivilegeLevel::Machine
    }
}

// ============================================================================
// mscratch register
// ============================================================================

/// Machine Scratch register (mscratch).
///
/// General-purpose scratch register for trap handlers.
#[derive(Debug, Clone, Copy, Default)]
pub struct Mscratch {
    value: u32,
}

impl Mscratch {
    /// Create a new mscratch register.
    pub fn new() -> Self {
        Self { value: 0 }
    }

    /// Get the scratch value.
    pub fn get(&self) -> u32 {
        self.value
    }

    /// Set the scratch value.
    pub fn set(&mut self, value: u32) {
        self.value = value;
    }
}

impl CsrRegister for Mscratch {
    fn address(&self) -> u16 {
        csr_addr::MSCRATCH
    }

    fn name(&self) -> &'static str {
        "mscratch"
    }

    fn read(&self) -> u32 {
        self.value
    }

    fn write(&mut self, value: u32) {
        self.value = value;
    }

    fn min_privilege(&self) -> PrivilegeLevel {
        PrivilegeLevel::Machine
    }
}

// ============================================================================
// misa register
// ============================================================================

/// Machine ISA register (misa).
///
/// Describes the ISA supported by the hart.
/// [31]    MXL   - Machine XLEN (1 for RV32)
/// [30:26] -     - WPRI
/// [25:0]  Extensions - One bit per extension
#[derive(Debug, Clone, Copy)]
pub struct Misa {
    value: u32,
}

/// MISA extension bits.
pub mod misa_ext {
    pub const A: u32 = 1 << 0; // Atomic
    pub const B: u32 = 1 << 1; // Bit manipulation
    pub const C: u32 = 1 << 2; // Compressed
    pub const D: u32 = 1 << 3; // Double-precision float
    pub const E: u32 = 1 << 4; // RV32E base ISA
    pub const F: u32 = 1 << 5; // Single-precision float
    pub const G: u32 = 1 << 6; // Additional standard extensions
    pub const H: u32 = 1 << 7; // Hypervisor
    pub const I: u32 = 1 << 8; // RV32I/64I/128I base ISA
    pub const J: u32 = 1 << 9; // Dynamically translated language
    pub const M: u32 = 1 << 12; // Integer multiply/divide
    pub const N: u32 = 1 << 13; // User-level interrupts
    pub const P: u32 = 1 << 15; // Packed SIMD
    pub const Q: u32 = 1 << 16; // Quad-precision float
    pub const S: u32 = 1 << 18; // Supervisor mode
    pub const U: u32 = 1 << 20; // User mode
    pub const V: u32 = 1 << 21; // Vector
    pub const X: u32 = 1 << 23; // Non-standard extensions
}

impl Misa {
    /// Create misa for RV32I with M, S, U extensions.
    pub fn new() -> Self {
        // MXL = 1 for RV32, extensions = I, M, S, U
        let mxl = 1u32 << 30; // MXL = 1 (RV32)
        let extensions = misa_ext::I | misa_ext::M | misa_ext::S | misa_ext::U;
        Self {
            value: mxl | extensions,
        }
    }
}

impl Default for Misa {
    fn default() -> Self {
        Self::new()
    }
}

impl CsrRegister for Misa {
    fn address(&self) -> u16 {
        csr_addr::MISA
    }

    fn name(&self) -> &'static str {
        "misa"
    }

    fn read(&self) -> u32 {
        self.value
    }

    fn write(&mut self, _value: u32) {
        // misa is optional to be writable, we make it read-only
        // Writing is ignored
    }

    fn is_writable(&self) -> bool {
        false
    }

    fn min_privilege(&self) -> PrivilegeLevel {
        PrivilegeLevel::Machine
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mstatus_mie() {
        let mut mstatus = Mstatus::new();
        assert!(!mstatus.mie());

        mstatus.set_mie(true);
        assert!(mstatus.mie());

        mstatus.set_mie(false);
        assert!(!mstatus.mie());
    }

    #[test]
    fn test_mstatus_mpp() {
        let mut mstatus = Mstatus::new();

        mstatus.set_mpp(PrivilegeLevel::User);
        assert_eq!(mstatus.mpp(), PrivilegeLevel::User);

        mstatus.set_mpp(PrivilegeLevel::Machine);
        assert_eq!(mstatus.mpp(), PrivilegeLevel::Machine);
    }

    #[test]
    fn test_mtvec_trap_address() {
        let mut mtvec = Mtvec::new();
        mtvec.set_base(Addr::new(0x1000));

        // Direct mode
        mtvec.set_mode(TrapVectorMode::Direct);
        assert_eq!(mtvec.trap_address(5, false), Addr::new(0x1000));
        assert_eq!(mtvec.trap_address(7, true), Addr::new(0x1000));

        // Vectored mode
        mtvec.set_mode(TrapVectorMode::Vectored);
        assert_eq!(mtvec.trap_address(5, false), Addr::new(0x1000)); // Exception -> BASE
        assert_eq!(mtvec.trap_address(7, true), Addr::new(0x101C)); // Interrupt -> BASE + 4*7
    }

    #[test]
    fn test_mcause() {
        let mut mcause = Mcause::new();

        mcause.set_exception(exception_code::ECALL_MACHINE);
        assert!(!mcause.is_interrupt());
        assert_eq!(mcause.code(), exception_code::ECALL_MACHINE);

        mcause.set_interrupt(interrupt_code::MACHINE_TIMER);
        assert!(mcause.is_interrupt());
        assert_eq!(mcause.code(), interrupt_code::MACHINE_TIMER);
    }

    #[test]
    fn test_mip_interrupt_priority() {
        let mut mip = Mip::new();
        let mie = Mie::new();

        // No interrupts pending
        assert!(mip.highest_priority_interrupt(&mie).is_none());

        // Set timer interrupt pending
        mip.set_mtip(true);
        let mut mie = Mie::new();
        // No interrupts enabled
        assert!(mip.highest_priority_interrupt(&mie).is_none());

        // Enable timer interrupt
        mie.write(ie_bits::MTIE);
        let result = mip.highest_priority_interrupt(&mie);
        assert!(result.is_some());
        let (is_int, code) = result.unwrap();
        assert!(is_int);
        assert_eq!(code, interrupt_code::MACHINE_TIMER);
    }

    #[test]
    fn test_mip_write_updates_supervisor_pending_bits_only() {
        let mut mip = Mip::new();
        mip.set_mtip(true);

        mip.write(ip_bits::SSIP | ip_bits::STIP | ip_bits::SEIP);

        assert!(mip.ssip());
        assert!(mip.stip());
        assert!(mip.seip());
        assert!(mip.mtip());

        mip.write(0);
        assert!(!mip.ssip());
        assert!(!mip.stip());
        assert!(!mip.seip());
        assert!(mip.mtip());
    }
}
