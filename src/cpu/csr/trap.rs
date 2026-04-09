//! Trap handling for RISC-V.
//!
//! This module provides trap entry/exit handling logic.

use super::csr_trait::CsrRegister;
use super::machine::{
    exception_code, interrupt_code, medeleg_bits, mideleg_bits, Mcause, Medeleg, Mepc, Mideleg,
    Mstatus, Mtval, Mtvec,
};
use super::supervisor::{Scause, Sepc, Sstatus, Stval, Stvec};
use crate::types::{Addr, PrivilegeLevel};

/// Trap cause enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrapCause {
    /// Exception (synchronous)
    Exception(ExceptionCause),
    /// Interrupt (asynchronous)
    Interrupt(InterruptCause),
}

/// Exception causes (synchronous traps).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionCause {
    /// Instruction address misaligned
    InstructionMisaligned,
    /// Instruction access fault
    InstructionAccessFault,
    /// Illegal instruction
    IllegalInstruction,
    /// Breakpoint
    Breakpoint,
    /// Load address misaligned
    LoadMisaligned,
    /// Load access fault
    LoadAccessFault,
    /// Store/AMO address misaligned
    StoreMisaligned,
    /// Store/AMO access fault
    StoreAccessFault,
    /// Environment call from U-mode
    EcallUser,
    /// Environment call from S-mode
    EcallSupervisor,
    /// Environment call from M-mode
    EcallMachine,
    /// Instruction page fault
    InstructionPageFault,
    /// Load page fault
    LoadPageFault,
    /// Store/AMO page fault
    StorePageFault,
    /// Unknown exception
    Unknown(u32),
}

impl ExceptionCause {
    /// Create from cause code.
    pub fn from_code(code: u32) -> Self {
        match code {
            exception_code::INSTRUCTION_MISALIGNED => Self::InstructionMisaligned,
            exception_code::INSTRUCTION_ACCESS_FAULT => Self::InstructionAccessFault,
            exception_code::ILLEGAL_INSTRUCTION => Self::IllegalInstruction,
            exception_code::BREAKPOINT => Self::Breakpoint,
            exception_code::LOAD_MISALIGNED => Self::LoadMisaligned,
            exception_code::LOAD_ACCESS_FAULT => Self::LoadAccessFault,
            exception_code::STORE_MISALIGNED => Self::StoreMisaligned,
            exception_code::STORE_ACCESS_FAULT => Self::StoreAccessFault,
            exception_code::ECALL_USER => Self::EcallUser,
            exception_code::ECALL_SUPERVISOR => Self::EcallSupervisor,
            exception_code::ECALL_MACHINE => Self::EcallMachine,
            exception_code::INSTRUCTION_PAGE_FAULT => Self::InstructionPageFault,
            exception_code::LOAD_PAGE_FAULT => Self::LoadPageFault,
            exception_code::STORE_PAGE_FAULT => Self::StorePageFault,
            _ => Self::Unknown(code),
        }
    }

    /// Get the cause code.
    pub fn code(&self) -> u32 {
        match self {
            Self::InstructionMisaligned => exception_code::INSTRUCTION_MISALIGNED,
            Self::InstructionAccessFault => exception_code::INSTRUCTION_ACCESS_FAULT,
            Self::IllegalInstruction => exception_code::ILLEGAL_INSTRUCTION,
            Self::Breakpoint => exception_code::BREAKPOINT,
            Self::LoadMisaligned => exception_code::LOAD_MISALIGNED,
            Self::LoadAccessFault => exception_code::LOAD_ACCESS_FAULT,
            Self::StoreMisaligned => exception_code::STORE_MISALIGNED,
            Self::StoreAccessFault => exception_code::STORE_ACCESS_FAULT,
            Self::EcallUser => exception_code::ECALL_USER,
            Self::EcallSupervisor => exception_code::ECALL_SUPERVISOR,
            Self::EcallMachine => exception_code::ECALL_MACHINE,
            Self::InstructionPageFault => exception_code::INSTRUCTION_PAGE_FAULT,
            Self::LoadPageFault => exception_code::LOAD_PAGE_FAULT,
            Self::StorePageFault => exception_code::STORE_PAGE_FAULT,
            Self::Unknown(code) => *code,
        }
    }

    /// Get ecall cause from privilege level.
    pub fn ecall_from(privilege: PrivilegeLevel) -> Self {
        match privilege {
            PrivilegeLevel::User => Self::EcallUser,
            PrivilegeLevel::Supervisor => Self::EcallSupervisor,
            PrivilegeLevel::Machine => Self::EcallMachine,
        }
    }
}

impl std::fmt::Display for ExceptionCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InstructionMisaligned => write!(f, "Instruction address misaligned"),
            Self::InstructionAccessFault => write!(f, "Instruction access fault"),
            Self::IllegalInstruction => write!(f, "Illegal instruction"),
            Self::Breakpoint => write!(f, "Breakpoint"),
            Self::LoadMisaligned => write!(f, "Load address misaligned"),
            Self::LoadAccessFault => write!(f, "Load access fault"),
            Self::StoreMisaligned => write!(f, "Store address misaligned"),
            Self::StoreAccessFault => write!(f, "Store access fault"),
            Self::EcallUser => write!(f, "ECALL from U-mode"),
            Self::EcallSupervisor => write!(f, "ECALL from S-mode"),
            Self::EcallMachine => write!(f, "ECALL from M-mode"),
            Self::InstructionPageFault => write!(f, "Instruction page fault"),
            Self::LoadPageFault => write!(f, "Load page fault"),
            Self::StorePageFault => write!(f, "Store page fault"),
            Self::Unknown(code) => write!(f, "Unknown exception ({})", code),
        }
    }
}

/// Interrupt causes (asynchronous traps).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptCause {
    /// User software interrupt
    UserSoftware,
    /// Supervisor software interrupt
    SupervisorSoftware,
    /// Machine software interrupt
    MachineSoftware,
    /// User timer interrupt
    UserTimer,
    /// Supervisor timer interrupt
    SupervisorTimer,
    /// Machine timer interrupt
    MachineTimer,
    /// User external interrupt
    UserExternal,
    /// Supervisor external interrupt
    SupervisorExternal,
    /// Machine external interrupt
    MachineExternal,
    /// Unknown interrupt
    Unknown(u32),
}

impl InterruptCause {
    /// Create from cause code.
    pub fn from_code(code: u32) -> Self {
        match code {
            interrupt_code::USER_SOFTWARE => Self::UserSoftware,
            interrupt_code::SUPERVISOR_SOFTWARE => Self::SupervisorSoftware,
            interrupt_code::MACHINE_SOFTWARE => Self::MachineSoftware,
            interrupt_code::USER_TIMER => Self::UserTimer,
            interrupt_code::SUPERVISOR_TIMER => Self::SupervisorTimer,
            interrupt_code::MACHINE_TIMER => Self::MachineTimer,
            interrupt_code::USER_EXTERNAL => Self::UserExternal,
            interrupt_code::SUPERVISOR_EXTERNAL => Self::SupervisorExternal,
            interrupt_code::MACHINE_EXTERNAL => Self::MachineExternal,
            _ => Self::Unknown(code),
        }
    }

    /// Get the cause code.
    pub fn code(&self) -> u32 {
        match self {
            Self::UserSoftware => interrupt_code::USER_SOFTWARE,
            Self::SupervisorSoftware => interrupt_code::SUPERVISOR_SOFTWARE,
            Self::MachineSoftware => interrupt_code::MACHINE_SOFTWARE,
            Self::UserTimer => interrupt_code::USER_TIMER,
            Self::SupervisorTimer => interrupt_code::SUPERVISOR_TIMER,
            Self::MachineTimer => interrupt_code::MACHINE_TIMER,
            Self::UserExternal => interrupt_code::USER_EXTERNAL,
            Self::SupervisorExternal => interrupt_code::SUPERVISOR_EXTERNAL,
            Self::MachineExternal => interrupt_code::MACHINE_EXTERNAL,
            Self::Unknown(code) => *code,
        }
    }
}

impl std::fmt::Display for InterruptCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UserSoftware => write!(f, "User software interrupt"),
            Self::SupervisorSoftware => write!(f, "Supervisor software interrupt"),
            Self::MachineSoftware => write!(f, "Machine software interrupt"),
            Self::UserTimer => write!(f, "User timer interrupt"),
            Self::SupervisorTimer => write!(f, "Supervisor timer interrupt"),
            Self::MachineTimer => write!(f, "Machine timer interrupt"),
            Self::UserExternal => write!(f, "User external interrupt"),
            Self::SupervisorExternal => write!(f, "Supervisor external interrupt"),
            Self::MachineExternal => write!(f, "Machine external interrupt"),
            Self::Unknown(code) => write!(f, "Unknown interrupt ({})", code),
        }
    }
}

/// Trap information.
#[derive(Debug, Clone, Copy)]
pub struct Trap {
    /// The cause of the trap.
    pub cause: TrapCause,
    /// The PC where the trap occurred.
    pub epc: Addr,
    /// Additional trap value (faulting address, instruction bits, etc.).
    pub tval: u32,
}

impl Trap {
    /// Create a new trap.
    pub fn new(cause: TrapCause, epc: Addr, tval: u32) -> Self {
        Self { cause, epc, tval }
    }

    /// Create an exception trap.
    pub fn exception(cause: ExceptionCause, epc: Addr, tval: u32) -> Self {
        Self::new(TrapCause::Exception(cause), epc, tval)
    }

    /// Create an interrupt trap.
    pub fn interrupt(cause: InterruptCause, epc: Addr) -> Self {
        Self::new(TrapCause::Interrupt(cause), epc, 0)
    }

    /// Create an ECALL trap.
    pub fn ecall(privilege: PrivilegeLevel, epc: Addr) -> Self {
        Self::exception(ExceptionCause::ecall_from(privilege), epc, 0)
    }

    /// Create an EBREAK trap.
    pub fn ebreak(epc: Addr) -> Self {
        Self::exception(ExceptionCause::Breakpoint, epc, 0)
    }

    /// Check if this is an interrupt.
    pub fn is_interrupt(&self) -> bool {
        matches!(self.cause, TrapCause::Interrupt(_))
    }

    /// Get the cause code.
    pub fn cause_code(&self) -> u32 {
        match &self.cause {
            TrapCause::Exception(e) => e.code(),
            TrapCause::Interrupt(i) => i.code(),
        }
    }

    /// Take the trap in machine mode.
    ///
    /// This updates the machine-mode CSRs and returns the trap handler address.
    pub fn take_m_trap(
        &self,
        mstatus: &mut Mstatus,
        mepc: &mut Mepc,
        mcause: &mut Mcause,
        mtval: &mut Mtval,
        mtvec: &Mtvec,
        current_privilege: PrivilegeLevel,
    ) -> (Addr, PrivilegeLevel) {
        // Save current privilege to MPP
        mstatus.set_mpp(current_privilege);

        // Save current MIE to MPIE, then disable interrupts
        mstatus.set_mpie(mstatus.mie());
        mstatus.set_mie(false);

        // Save exception PC
        mepc.set(self.epc);

        // Save cause
        mcause.set(self.is_interrupt(), self.cause_code());

        // Save trap value
        mtval.set(self.tval);

        // Calculate trap handler address
        let handler_addr = if self.is_interrupt() {
            match mtvec.mode() {
                super::machine::TrapVectorMode::Direct => mtvec.base(),
                super::machine::TrapVectorMode::Vectored => {
                    Addr::new(mtvec.base().raw() + self.cause_code() * 4)
                }
            }
        } else {
            // Exceptions always go to BASE
            mtvec.base()
        };

        (handler_addr, PrivilegeLevel::Machine)
    }

    /// Check if this trap should be delegated to supervisor mode.
    ///
    /// Delegation is controlled by medeleg (exceptions) and mideleg (interrupts).
    pub fn should_delegate(&self, mideleg: &Mideleg, medeleg: &Medeleg) -> bool {
        match &self.cause {
            TrapCause::Interrupt(interrupt) => {
                let bit = match interrupt {
                    InterruptCause::SupervisorSoftware => mideleg_bits::SSIP,
                    InterruptCause::SupervisorTimer => mideleg_bits::STIP,
                    InterruptCause::SupervisorExternal => mideleg_bits::SEIP,
                    _ => return false, // Other interrupts cannot be delegated
                };
                (mideleg.read() & bit) != 0
            }
            TrapCause::Exception(exception) => {
                let bit = match exception {
                    ExceptionCause::InstructionMisaligned => medeleg_bits::IAM,
                    ExceptionCause::InstructionAccessFault => medeleg_bits::IAF,
                    ExceptionCause::IllegalInstruction => medeleg_bits::ILGL,
                    ExceptionCause::Breakpoint => medeleg_bits::BKPT,
                    ExceptionCause::LoadMisaligned => medeleg_bits::LAM,
                    ExceptionCause::LoadAccessFault => medeleg_bits::LAF,
                    ExceptionCause::StoreMisaligned => medeleg_bits::SAM,
                    ExceptionCause::StoreAccessFault => medeleg_bits::SAF,
                    ExceptionCause::EcallUser => medeleg_bits::UECL,
                    ExceptionCause::EcallSupervisor => medeleg_bits::SECL,
                    ExceptionCause::InstructionPageFault => medeleg_bits::IPFI,
                    ExceptionCause::LoadPageFault => medeleg_bits::LPFL,
                    ExceptionCause::StorePageFault => medeleg_bits::SPFS,
                    ExceptionCause::EcallMachine => return false, // ECALL_M cannot be delegated
                    ExceptionCause::Unknown(_) => return false,
                };
                (medeleg.read() & bit) != 0
            }
        }
    }

    /// Take the trap in supervisor mode (delegated trap).
    ///
    /// This updates the supervisor-mode CSRs and returns the trap handler address.
    pub fn take_s_trap(
        &self,
        sstatus: &mut Sstatus,
        sepc: &mut Sepc,
        scause: &mut Scause,
        stval: &mut Stval,
        stvec: &Stvec,
        current_privilege: PrivilegeLevel,
    ) -> (Addr, PrivilegeLevel) {
        // Save current privilege to SPP
        sstatus.set_spp(current_privilege);

        // Save current SIE to SPIE, then disable interrupts
        sstatus.set_spie(sstatus.sie());
        sstatus.set_sie(false);

        // Save exception PC
        sepc.set(self.epc);

        // Save cause
        scause.set(self.is_interrupt(), self.cause_code());

        // Save trap value
        stval.set(self.tval);

        // Calculate trap handler address
        let handler_addr = if self.is_interrupt() {
            match stvec.mode() {
                0 => stvec.base(),                                          // Direct
                1 => Addr::new(stvec.base().raw() + self.cause_code() * 4), // Vectored
                _ => stvec.base(), // Default to Direct for unknown modes
            }
        } else {
            // Exceptions always go to BASE
            stvec.base()
        };

        (handler_addr, PrivilegeLevel::Supervisor)
    }
}

#[cfg(test)]
mod tests {
    use super::super::machine::TrapVectorMode;
    use super::*;

    #[test]
    fn test_exception_cause_ecall() {
        assert_eq!(
            ExceptionCause::ecall_from(PrivilegeLevel::User),
            ExceptionCause::EcallUser
        );
        assert_eq!(
            ExceptionCause::ecall_from(PrivilegeLevel::Supervisor),
            ExceptionCause::EcallSupervisor
        );
        assert_eq!(
            ExceptionCause::ecall_from(PrivilegeLevel::Machine),
            ExceptionCause::EcallMachine
        );
    }

    #[test]
    fn test_trap_creation() {
        let trap = Trap::ecall(PrivilegeLevel::User, Addr::new(0x1000));
        assert!(!trap.is_interrupt());
        assert_eq!(trap.cause_code(), exception_code::ECALL_USER);
        assert_eq!(trap.epc, Addr::new(0x1000));
    }

    #[test]
    fn test_trap_take_m() {
        let mut mstatus = Mstatus::new();
        let mut mepc = Mepc::new();
        let mut mcause = Mcause::new();
        let mut mtval = Mtval::new();
        let mut mtvec = Mtvec::new();

        mtvec.set_base(Addr::new(0x2000));
        mtvec.set_mode(TrapVectorMode::Direct);

        // Enable interrupts
        mstatus.set_mie(true);

        let trap = Trap::ecall(PrivilegeLevel::Supervisor, Addr::new(0x1000));
        let (handler, new_priv) = trap.take_m_trap(
            &mut mstatus,
            &mut mepc,
            &mut mcause,
            &mut mtval,
            &mtvec,
            PrivilegeLevel::Supervisor,
        );

        assert_eq!(handler, Addr::new(0x2000));
        assert_eq!(new_priv, PrivilegeLevel::Machine);
        assert_eq!(mstatus.mpp(), PrivilegeLevel::Supervisor);
        assert!(mstatus.mpie()); // Previous MIE was false
        assert!(!mstatus.mie()); // Now disabled
        assert_eq!(mepc.get(), Addr::new(0x1000));
        assert_eq!(mcause.code(), exception_code::ECALL_SUPERVISOR);
        assert!(!mcause.is_interrupt());
    }
}
