//! Control and Status Registers (CSR) module.
//!
//! This module implements the RISC-V CSR subsystem, including:
//! - Machine-mode CSRs (mstatus, mtvec, mepc, mcause, etc.)
//! - Supervisor-mode CSRs (sstatus, stvec, sepc, etc.)
//! - User-mode CSRs (ustatus, utvec, etc.)
//! - Performance Monitor CSRs (mcycle, minstret, mhpmcounter3-31)
//! - CSR access instructions (CSRRW, CSRRS, CSRRC)

pub mod csr_trait;
pub mod machine;
pub mod perf;
pub mod supervisor;
pub mod trap;
pub mod user;

pub use csr_trait::{CsrAccessCheck, CsrRegister};
pub use machine::{
    exception_code, ie_bits, interrupt_code, ip_bits, Mcause, Medeleg, Mepc, Mideleg, Mie, Mip,
    Misa, Mscratch, Mstatus, Mtval, Mtvec, TrapVectorMode,
};
pub use perf::{
    csr_addr as perf_csr_addr, Mcountinhibit, Mcycle, Mcycleh, Mhpmcounter, Mhpmevent, Minstret,
    Minstreth, PerfCounters, PerfEvent, HPM_COUNTER_BASE, HPM_COUNTER_COUNT,
};
pub use supervisor::{Satp, Scause, Sepc, Sie, Sip, Sscratch, Sstatus, Stval, Stvec};
pub use trap::{ExceptionCause, InterruptCause, Trap, TrapCause};
pub use user::{Ucause, Uepc, Ustatus, Utvec};

use crate::error::{Result, SimError};
use crate::types::PrivilegeLevel;
use std::collections::BTreeMap;

/// CSR address constants for all privilege levels.
pub mod csr_addr {
    // Machine-mode CSRs
    pub const MSTATUS: u16 = 0x300;
    pub const MISA: u16 = 0x301;
    pub const MEDELEG: u16 = 0x302;
    pub const MIDELEG: u16 = 0x303;
    pub const MIE: u16 = 0x304;
    pub const MTVEC: u16 = 0x305;
    pub const MCOUNTEREN: u16 = 0x306;
    pub const PMPCFG0: u16 = 0x3A0;
    pub const PMPCFG15: u16 = 0x3AF;
    pub const PMPADDR0: u16 = 0x3B0;
    pub const PMPADDR63: u16 = 0x3EF;
    pub const MSCRATCH: u16 = 0x340;
    pub const MEPC: u16 = 0x341;
    pub const MCAUSE: u16 = 0x342;
    pub const MTVAL: u16 = 0x343;
    pub const MIP: u16 = 0x344;

    // Supervisor-mode CSRs
    pub const SSTATUS: u16 = 0x100;
    pub const SIE: u16 = 0x104;
    pub const STVEC: u16 = 0x105;
    pub const SATP: u16 = 0x180;
    pub const SSCRATCH: u16 = 0x140;
    pub const SEPC: u16 = 0x141;
    pub const SCAUSE: u16 = 0x142;
    pub const STVAL: u16 = 0x143;
    pub const SIP: u16 = 0x144;

    // User-mode CSRs (N extension)
    pub const USTATUS: u16 = 0x000;
    pub const UIE: u16 = 0x004;
    pub const UTVEC: u16 = 0x005;
    pub const USCRATCH: u16 = 0x040;
    pub const UEPC: u16 = 0x041;
    pub const UCAUSE: u16 = 0x042;
    pub const UTVAL: u16 = 0x043;
    pub const UIP: u16 = 0x044;

    // User-level counter/timer CSRs (read-only aliases)
    pub const CYCLE: u16 = 0xC00;
    pub const TIME: u16 = 0xC01;
    pub const INSTRET: u16 = 0xC02;
    pub const HPMCOUNTER3: u16 = 0xC03;
    pub const HPMCOUNTER31: u16 = 0xC1F;
    pub const CYCLEH: u16 = 0xC80;
    pub const TIMEH: u16 = 0xC81;
    pub const INSTRETH: u16 = 0xC82;
    pub const HPMCOUNTER3H: u16 = 0xC83;
    pub const HPMCOUNTER31H: u16 = 0xC9F;

    // Read-only CSRs
    pub const MVENDORID: u16 = 0xF11;
    pub const MARCHID: u16 = 0xF12;
    pub const MIMPID: u16 = 0xF13;
    pub const MHARTID: u16 = 0xF14;
}

/// CSR operation type for Zicsr instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CsrOp {
    /// CSRRW: csr = rs1; rd = old_csr
    #[default]
    ReadWrite,
    /// CSRRS: csr |= rs1; rd = old_csr (set bits)
    ReadSet,
    /// CSRRC: csr &= ~rs1; rd = old_csr (clear bits)
    ReadClear,
    /// CSRRWI: csr = zimm; rd = old_csr (immediate)
    ReadWriteImm,
    /// CSRRSI: csr |= zimm; rd = old_csr (immediate)
    ReadSetImm,
    /// CSRRCI: csr &= ~zimm; rd = old_csr (immediate)
    ReadClearImm,
}

impl CsrOp {
    /// Check if this operation writes to the CSR.
    pub fn is_write(&self) -> bool {
        matches!(
            self,
            CsrOp::ReadWrite
                | CsrOp::ReadSet
                | CsrOp::ReadClear
                | CsrOp::ReadWriteImm
                | CsrOp::ReadSetImm
                | CsrOp::ReadClearImm
        )
    }

    /// Check if this operation uses immediate value.
    pub fn is_imm(&self) -> bool {
        matches!(
            self,
            CsrOp::ReadWriteImm | CsrOp::ReadSetImm | CsrOp::ReadClearImm
        )
    }
}

/// CSR file containing all Control and Status Registers.
#[derive(Debug, Clone)]
pub struct CsrFile {
    // Machine-mode CSRs
    pub mstatus: Mstatus,
    pub misa: Misa,
    pub mie: Mie,
    pub mtvec: Mtvec,
    pub mcounteren: u32,
    pub mscratch: Mscratch,
    pub mepc: Mepc,
    pub mcause: Mcause,
    pub mtval: Mtval,
    pub mip: Mip,
    pub pmpcfg: [u32; 16],
    pub pmpaddr: [u32; 64],
    pub mideleg: Mideleg,
    pub medeleg: Medeleg,

    // Supervisor-mode CSRs
    pub sstatus: Sstatus,
    pub sie: Sie,
    pub stvec: Stvec,
    pub satp: Satp,
    pub sscratch: Sscratch,
    pub sepc: Sepc,
    pub scause: Scause,
    pub stval: Stval,
    pub sip: Sip,

    // User-mode CSRs
    pub ustatus: Ustatus,
    pub utvec: Utvec,
    pub uepc: Uepc,
    pub ucause: Ucause,

    // Performance Monitor CSRs
    pub perf: PerfCounters,

    // Compatibility storage for optional/implementation-defined machine CSRs.
    pub machine_compat_csrs: BTreeMap<u16, u32>,
}

impl Default for CsrFile {
    fn default() -> Self {
        Self::new()
    }
}

impl CsrFile {
    /// Create a new CSR file with default values.
    pub fn new() -> Self {
        Self {
            mstatus: Mstatus::new(),
            misa: Misa::new(),
            mie: Mie::new(),
            mtvec: Mtvec::new(),
            mcounteren: 0,
            mscratch: Mscratch::new(),
            mepc: Mepc::new(),
            mcause: Mcause::new(),
            mtval: Mtval::new(),
            mip: Mip::new(),
            pmpcfg: [0; 16],
            pmpaddr: [0; 64],
            mideleg: Mideleg::new(),
            medeleg: Medeleg::new(),
            sstatus: Sstatus::new(),
            sie: Sie::new(),
            stvec: Stvec::new(),
            satp: Satp::new(),
            sscratch: Sscratch::new(),
            sepc: Sepc::new(),
            scause: Scause::new(),
            stval: Stval::new(),
            sip: Sip::new(),
            ustatus: Ustatus::new(),
            utvec: Utvec::new(),
            uepc: Uepc::new(),
            ucause: Ucause::new(),
            perf: PerfCounters::new(),
            machine_compat_csrs: BTreeMap::new(),
        }
    }

    /// Check if a CSR address is valid and accessible from the given privilege level.
    pub fn check_access(addr: u16, privilege: PrivilegeLevel, is_write: bool) -> Result<()> {
        // Check if address is valid (12-bit CSR address space)
        if addr > 0xFFF {
            return Err(SimError::InvalidCsr(addr));
        }

        // Determine minimum privilege level from address bits [9:8]
        let min_priv = match (addr >> 8) & 0x3 {
            0 => PrivilegeLevel::User,
            1 => PrivilegeLevel::Supervisor,
            2 | 3 => PrivilegeLevel::Machine,
            _ => return Err(SimError::InvalidCsr(addr)),
        };

        // Check privilege level
        if privilege < min_priv {
            return Err(SimError::CsrAccessDenied {
                csr: addr,
                mode: format!("{:?}", privilege),
            });
        }

        // Check read-only (address bits [11:10] = 10 or 11)
        if is_write {
            let read_only = matches!((addr >> 10) & 0x3, 2 | 3);
            let writable_machine_counter = addr == perf_csr_addr::MCYCLE
                || addr == perf_csr_addr::MCYCLEH
                || addr == perf_csr_addr::MINSTRET
                || addr == perf_csr_addr::MINSTRETH
                || (perf_csr_addr::MHPMCOUNTER_BASE..=perf_csr_addr::MHPMCOUNTER_END)
                    .contains(&addr)
                || (perf_csr_addr::MHPMCOUNTERH_BASE..=perf_csr_addr::MHPMCOUNTERH_END)
                    .contains(&addr);

            if read_only && !writable_machine_counter {
                return Err(SimError::CsrAccessDenied {
                    csr: addr,
                    mode: format!("{:?}", privilege),
                });
            }
        }

        Ok(())
    }

    /// Read a CSR by address.
    pub fn read(&self, addr: u16, privilege: PrivilegeLevel) -> Result<u32> {
        Self::check_access(addr, privilege, false)?;

        match addr {
            // Machine-mode
            csr_addr::MSTATUS => Ok(self.mstatus.read()),
            csr_addr::MISA => Ok(self.misa.read()),
            csr_addr::MIE => Ok(self.mie.read()),
            csr_addr::MTVEC => Ok(self.mtvec.read()),
            csr_addr::MCOUNTEREN => Ok(self.mcounteren),
            csr_addr::MSCRATCH => Ok(self.mscratch.read()),
            csr_addr::MEPC => Ok(self.mepc.read()),
            csr_addr::MCAUSE => Ok(self.mcause.read()),
            csr_addr::MTVAL => Ok(self.mtval.read()),
            csr_addr::MIP => Ok(self.mip.read()),
            addr if (csr_addr::PMPCFG0..=csr_addr::PMPCFG15).contains(&addr) => {
                Ok(self.pmpcfg[(addr - csr_addr::PMPCFG0) as usize])
            }
            addr if (csr_addr::PMPADDR0..=csr_addr::PMPADDR63).contains(&addr) => {
                Ok(self.pmpaddr[(addr - csr_addr::PMPADDR0) as usize])
            }
            csr_addr::MIDELEG => Ok(self.mideleg.read()),
            csr_addr::MEDELEG => Ok(self.medeleg.read()),
            csr_addr::MVENDORID => Ok(0),
            csr_addr::MARCHID => Ok(0),
            csr_addr::MIMPID => Ok(0),
            csr_addr::MHARTID => Ok(0),

            // Supervisor-mode
            csr_addr::SSTATUS => Ok(self.sstatus.read()),
            csr_addr::SIE => Ok(self.sie.read()),
            csr_addr::STVEC => Ok(self.stvec.read()),
            csr_addr::SATP => Ok(self.satp.read()),
            csr_addr::SSCRATCH => Ok(self.sscratch.read()),
            csr_addr::SEPC => Ok(self.sepc.read()),
            csr_addr::SCAUSE => Ok(self.scause.read()),
            csr_addr::STVAL => Ok(self.stval.read()),
            csr_addr::SIP => Ok(self.sip.read()),

            // User-mode
            csr_addr::USTATUS => Ok(self.ustatus.read()),
            csr_addr::UTVEC => Ok(self.utvec.read()),
            csr_addr::UEPC => Ok(self.uepc.read()),
            csr_addr::UCAUSE => Ok(self.ucause.read()),
            csr_addr::CYCLE => Ok(self.perf.mcycle.read_low()),
            csr_addr::TIME => Ok(self.perf.mcycle.read_low()),
            csr_addr::INSTRET => Ok(self.perf.minstret.read_low()),
            csr_addr::CYCLEH => Ok(self.perf.mcycleh.read()),
            csr_addr::TIMEH => Ok(self.perf.mcycleh.read()),
            csr_addr::INSTRETH => Ok(self.perf.minstreth.read()),
            addr if (csr_addr::HPMCOUNTER3..=csr_addr::HPMCOUNTER31).contains(&addr) => {
                let index = (addr - csr_addr::HPMCOUNTER3) as usize + 3;
                if index <= 31 {
                    Ok(self.perf.mhpmcounters[index - 3].read_low())
                } else {
                    Err(SimError::InvalidCsr(addr))
                }
            }
            addr if (csr_addr::HPMCOUNTER3H..=csr_addr::HPMCOUNTER31H).contains(&addr) => {
                let index = (addr - csr_addr::HPMCOUNTER3H) as usize + 3;
                if index <= 31 {
                    Ok(self.perf.mhpmcounters[index - 3].read_high())
                } else {
                    Err(SimError::InvalidCsr(addr))
                }
            }

            // Performance Monitor CSRs
            perf_csr_addr::MCYCLE => Ok(self.perf.mcycle.read_low()),
            perf_csr_addr::MCYCLEH => Ok(self.perf.mcycleh.read()),
            perf_csr_addr::MINSTRET => Ok(self.perf.minstret.read_low()),
            perf_csr_addr::MINSTRETH => Ok(self.perf.minstreth.read()),
            perf_csr_addr::MCOUNTINHIBIT => Ok(self.perf.mcountinhibit.read()),
            addr if (perf_csr_addr::MHPMCOUNTER_BASE..=perf_csr_addr::MHPMCOUNTER_END)
                .contains(&addr) =>
            {
                let index = (addr - perf_csr_addr::MHPMCOUNTER_BASE) as usize + 3;
                if index <= 31 {
                    Ok(self.perf.mhpmcounters[index - 3].read_low())
                } else {
                    Err(SimError::InvalidCsr(addr))
                }
            }
            addr if (perf_csr_addr::MHPMCOUNTERH_BASE..=perf_csr_addr::MHPMCOUNTERH_END)
                .contains(&addr) =>
            {
                let index = (addr - perf_csr_addr::MHPMCOUNTERH_BASE) as usize + 3;
                if index <= 31 {
                    Ok(self.perf.mhpmcounters[index - 3].read_high())
                } else {
                    Err(SimError::InvalidCsr(addr))
                }
            }
            addr if (perf_csr_addr::MHPMEVENT_BASE..=perf_csr_addr::MHPMEVENT_END)
                .contains(&addr) =>
            {
                let index = (addr - perf_csr_addr::MHPMEVENT_BASE) as usize + 3;
                if index <= 31 {
                    Ok(self.perf.mhpmevents[index - 3].read())
                } else {
                    Err(SimError::InvalidCsr(addr))
                }
            }

            // Optional/implementation-defined machine CSRs are treated as compatibility stubs.
            _ if privilege == PrivilegeLevel::Machine => {
                Ok(*self.machine_compat_csrs.get(&addr).unwrap_or(&0))
            }

            // Unimplemented
            _ => Err(SimError::InvalidCsr(addr)),
        }
    }

    /// Write a CSR by address.
    pub fn write(&mut self, addr: u16, value: u32, privilege: PrivilegeLevel) -> Result<()> {
        Self::check_access(addr, privilege, true)?;

        match addr {
            // Machine-mode
            csr_addr::MSTATUS => {
                self.mstatus.write(value);
                Ok(())
            }
            csr_addr::MISA => {
                self.misa.write(value);
                Ok(())
            }
            csr_addr::MIE => {
                self.mie.write(value);
                Ok(())
            }
            csr_addr::MTVEC => {
                self.mtvec.write(value);
                Ok(())
            }
            csr_addr::MCOUNTEREN => {
                self.mcounteren = value;
                Ok(())
            }
            csr_addr::MSCRATCH => {
                self.mscratch.write(value);
                Ok(())
            }
            csr_addr::MEPC => {
                self.mepc.write(value);
                Ok(())
            }
            csr_addr::MCAUSE => {
                self.mcause.write(value);
                Ok(())
            }
            csr_addr::MTVAL => {
                self.mtval.write(value);
                Ok(())
            }
            csr_addr::MIP => {
                self.mip.write(value);
                Ok(())
            }
            addr if (csr_addr::PMPCFG0..=csr_addr::PMPCFG15).contains(&addr) => {
                self.pmpcfg[(addr - csr_addr::PMPCFG0) as usize] = value;
                Ok(())
            }
            addr if (csr_addr::PMPADDR0..=csr_addr::PMPADDR63).contains(&addr) => {
                self.pmpaddr[(addr - csr_addr::PMPADDR0) as usize] = value;
                Ok(())
            }
            csr_addr::MIDELEG => {
                self.mideleg.write(value);
                Ok(())
            }
            csr_addr::MEDELEG => {
                self.medeleg.write(value);
                Ok(())
            }

            // Supervisor-mode
            csr_addr::SSTATUS => {
                self.sstatus.write(value);
                Ok(())
            }
            csr_addr::SIE => {
                self.sie.write(value);
                Ok(())
            }
            csr_addr::STVEC => {
                self.stvec.write(value);
                Ok(())
            }
            csr_addr::SATP => {
                self.satp.write(value);
                Ok(())
            }
            csr_addr::SSCRATCH => {
                self.sscratch.write(value);
                Ok(())
            }
            csr_addr::SEPC => {
                self.sepc.write(value);
                Ok(())
            }
            csr_addr::SCAUSE => {
                self.scause.write(value);
                Ok(())
            }
            csr_addr::STVAL => {
                self.stval.write(value);
                Ok(())
            }
            csr_addr::SIP => {
                if privilege == PrivilegeLevel::Machine {
                    // Machine mode may inject supervisor pending interrupts.
                    self.sip.set_ssip((value & ip_bits::SSIP) != 0);
                    self.sip.set_stip((value & ip_bits::STIP) != 0);
                    self.sip.set_seip((value & ip_bits::SEIP) != 0);
                } else {
                    // Supervisor mode can clear SSIP via CSR write semantics.
                    self.sip.write(value);
                }
                Ok(())
            }

            // User-mode
            csr_addr::USTATUS => {
                self.ustatus.write(value);
                Ok(())
            }
            csr_addr::UTVEC => {
                self.utvec.write(value);
                Ok(())
            }
            csr_addr::UEPC => {
                self.uepc.write(value);
                Ok(())
            }
            csr_addr::UCAUSE => {
                self.ucause.write(value);
                Ok(())
            }

            // Performance Monitor CSRs
            perf_csr_addr::MCYCLE => {
                self.perf.mcycle.write_low(value);
                Ok(())
            }
            perf_csr_addr::MCYCLEH => {
                self.perf.mcycleh.write(value);
                Ok(())
            }
            perf_csr_addr::MINSTRET => {
                self.perf.minstret.write_low(value);
                Ok(())
            }
            perf_csr_addr::MINSTRETH => {
                self.perf.minstreth.write(value);
                Ok(())
            }
            perf_csr_addr::MCOUNTINHIBIT => {
                self.perf.mcountinhibit.write(value);
                Ok(())
            }
            addr if (perf_csr_addr::MHPMCOUNTER_BASE..=perf_csr_addr::MHPMCOUNTER_END)
                .contains(&addr) =>
            {
                let index = (addr - perf_csr_addr::MHPMCOUNTER_BASE) as usize + 3;
                if index <= 31 {
                    self.perf.mhpmcounters[index - 3].write_low(value);
                    Ok(())
                } else {
                    Err(SimError::InvalidCsr(addr))
                }
            }
            addr if (perf_csr_addr::MHPMCOUNTERH_BASE..=perf_csr_addr::MHPMCOUNTERH_END)
                .contains(&addr) =>
            {
                let index = (addr - perf_csr_addr::MHPMCOUNTERH_BASE) as usize + 3;
                if index <= 31 {
                    self.perf.mhpmcounters[index - 3].write_high(value);
                    Ok(())
                } else {
                    Err(SimError::InvalidCsr(addr))
                }
            }
            addr if (perf_csr_addr::MHPMEVENT_BASE..=perf_csr_addr::MHPMEVENT_END)
                .contains(&addr) =>
            {
                let index = (addr - perf_csr_addr::MHPMEVENT_BASE) as usize + 3;
                if index <= 31 {
                    self.perf.mhpmevents[index - 3].write(value);
                    Ok(())
                } else {
                    Err(SimError::InvalidCsr(addr))
                }
            }

            // Optional/implementation-defined machine CSRs are treated as compatibility stubs.
            _ if privilege == PrivilegeLevel::Machine => {
                self.machine_compat_csrs.insert(addr, value);
                Ok(())
            }

            // Unimplemented
            _ => Err(SimError::InvalidCsr(addr)),
        }
    }

    /// Execute a CSR operation (CSRRW/CSRRS/CSRRC variants).
    ///
    /// Returns the old CSR value (for reading into rd).
    pub fn execute(
        &mut self,
        op: CsrOp,
        addr: u16,
        rs1_val: u32,
        privilege: PrivilegeLevel,
    ) -> Result<u32> {
        let is_imm = op.is_imm();
        let write_val = if is_imm { rs1_val & 0x1F } else { rs1_val };
        let should_write = match op {
            CsrOp::ReadWrite | CsrOp::ReadWriteImm => true,
            CsrOp::ReadSet | CsrOp::ReadSetImm | CsrOp::ReadClear | CsrOp::ReadClearImm => {
                write_val != 0
            }
        };

        // Check access permissions
        Self::check_access(addr, privilege, should_write)?;

        // Read old value
        let old_val = self.read(addr, privilege)?;

        // Perform write operation
        if should_write {
            match op {
                CsrOp::ReadWrite | CsrOp::ReadWriteImm => {
                    self.write(addr, write_val, privilege)?;
                }
                CsrOp::ReadSet | CsrOp::ReadSetImm => {
                    self.write(addr, old_val | write_val, privilege)?;
                }
                CsrOp::ReadClear | CsrOp::ReadClearImm => {
                    self.write(addr, old_val & !write_val, privilege)?;
                }
            }
        }

        Ok(old_val)
    }

    /// Check if there's a pending interrupt that should be taken.
    pub fn has_pending_interrupt(&self, privilege: PrivilegeLevel) -> bool {
        match privilege {
            PrivilegeLevel::Machine => self.mip.has_pending_interrupt(&self.mie),
            PrivilegeLevel::Supervisor => {
                (self.sip.ssip() && self.sie.ssie())
                    || (self.sip.stip() && self.sie.stie())
                    || (self.sip.seip() && self.sie.seie())
            }
            PrivilegeLevel::User => false,
        }
    }

    /// Get the highest priority pending interrupt for the current privilege level.
    pub fn get_pending_interrupt(&self, privilege: PrivilegeLevel) -> Option<(bool, u32)> {
        match privilege {
            PrivilegeLevel::Machine => self.mip.highest_priority_interrupt(&self.mie),
            PrivilegeLevel::Supervisor => {
                if self.sip.seip() && self.sie.seie() {
                    return Some((true, interrupt_code::SUPERVISOR_EXTERNAL));
                }
                if self.sip.stip() && self.sie.stie() {
                    return Some((true, interrupt_code::SUPERVISOR_TIMER));
                }
                if self.sip.ssip() && self.sie.ssie() {
                    return Some((true, interrupt_code::SUPERVISOR_SOFTWARE));
                }
                None
            }
            PrivilegeLevel::User => None,
        }
    }

    /// Reset all CSRs to initial values.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csr_file_read_write() {
        let mut csr_file = CsrFile::new();

        csr_file
            .write(csr_addr::MSCRATCH, 0x12345678, PrivilegeLevel::Machine)
            .unwrap();
        let val = csr_file
            .read(csr_addr::MSCRATCH, PrivilegeLevel::Machine)
            .unwrap();
        assert_eq!(val, 0x12345678);

        csr_file
            .write(csr_addr::SATP, 0x8123_4567, PrivilegeLevel::Supervisor)
            .unwrap();
        let satp = csr_file
            .read(csr_addr::SATP, PrivilegeLevel::Supervisor)
            .unwrap();
        assert_eq!(satp, 0x8123_4567 & 0xFFFF_FFFF);
    }

    #[test]
    fn test_csr_privilege_check() {
        let csr_file = CsrFile::new();

        let result = csr_file.read(csr_addr::MSTATUS, PrivilegeLevel::User);
        assert!(result.is_err());

        let satp_from_user = csr_file.read(csr_addr::SATP, PrivilegeLevel::User);
        assert!(satp_from_user.is_err());
    }

    #[test]
    fn test_csr_read_only() {
        let mut csr_file = CsrFile::new();

        let result = csr_file.write(0xF11, 0x1234, PrivilegeLevel::Machine);
        assert!(result.is_err());
    }

    #[test]
    fn test_csr_ops() {
        let mut csr_file = CsrFile::new();

        let old = csr_file
            .execute(
                CsrOp::ReadWrite,
                csr_addr::MSCRATCH,
                0x11111111,
                PrivilegeLevel::Machine,
            )
            .unwrap();
        assert_eq!(old, 0);
        let val = csr_file
            .read(csr_addr::MSCRATCH, PrivilegeLevel::Machine)
            .unwrap();
        assert_eq!(val, 0x11111111);

        let old = csr_file
            .execute(
                CsrOp::ReadSet,
                csr_addr::MSCRATCH,
                0x00001111,
                PrivilegeLevel::Machine,
            )
            .unwrap();
        assert_eq!(old, 0x11111111);
        let val = csr_file
            .read(csr_addr::MSCRATCH, PrivilegeLevel::Machine)
            .unwrap();
        assert_eq!(val, 0x11111111 | 0x00001111);

        let old = csr_file
            .execute(
                CsrOp::ReadClear,
                csr_addr::MSCRATCH,
                0x00000001,
                PrivilegeLevel::Machine,
            )
            .unwrap();
        assert_eq!(old, 0x11111111 | 0x00001111);
        let val = csr_file
            .read(csr_addr::MSCRATCH, PrivilegeLevel::Machine)
            .unwrap();
        assert_eq!(val, (0x11111111 | 0x00001111) & !0x00000001);
    }

    #[test]
    fn test_csrrs_zero_reads_read_only_csr() {
        let mut csr_file = CsrFile::new();

        let old = csr_file
            .execute(
                CsrOp::ReadSet,
                csr_addr::MHARTID,
                0,
                PrivilegeLevel::Machine,
            )
            .unwrap();

        assert_eq!(old, 0);
    }

    #[test]
    fn test_sip_machine_write_can_set_ssip() {
        let mut csr_file = CsrFile::new();

        csr_file
            .write(
                csr_addr::SIP,
                ip_bits::SSIP | ip_bits::STIP | ip_bits::SEIP,
                PrivilegeLevel::Machine,
            )
            .unwrap();
        assert!(csr_file.sip.ssip());
        assert!(csr_file.sip.stip());
        assert!(csr_file.sip.seip());

        csr_file
            .write(csr_addr::SIP, 0, PrivilegeLevel::Supervisor)
            .unwrap();
        assert!(!csr_file.sip.ssip());
        assert!(csr_file.sip.stip());
        assert!(csr_file.sip.seip());

        csr_file
            .write(csr_addr::SIP, 0, PrivilegeLevel::Machine)
            .unwrap();
        assert!(!csr_file.sip.ssip());
        assert!(!csr_file.sip.stip());
        assert!(!csr_file.sip.seip());
    }

    #[test]
    fn test_pmp_csr_read_write_stub() {
        let mut csr_file = CsrFile::new();

        csr_file
            .write(csr_addr::PMPCFG0, 0xA5A5_5A5A, PrivilegeLevel::Machine)
            .unwrap();
        csr_file
            .write(csr_addr::PMPADDR0, 0x1234_5678, PrivilegeLevel::Machine)
            .unwrap();

        assert_eq!(
            csr_file
                .read(csr_addr::PMPCFG0, PrivilegeLevel::Machine)
                .unwrap(),
            0xA5A5_5A5A
        );
        assert_eq!(
            csr_file
                .read(csr_addr::PMPADDR0, PrivilegeLevel::Machine)
                .unwrap(),
            0x1234_5678
        );
    }

    #[test]
    fn test_mhpmcounter_write_allowed_in_machine_mode() {
        let mut csr_file = CsrFile::new();

        csr_file
            .write(
                perf_csr_addr::MHPMCOUNTER_BASE,
                0xDEAD_BEEF,
                PrivilegeLevel::Machine,
            )
            .unwrap();

        assert_eq!(
            csr_file
                .read(perf_csr_addr::MHPMCOUNTER_BASE, PrivilegeLevel::Machine)
                .unwrap(),
            0xDEAD_BEEF
        );
    }

    #[test]
    fn test_mcounteren_read_write_stub() {
        let mut csr_file = CsrFile::new();

        csr_file
            .write(csr_addr::MCOUNTEREN, 0xFFFF_FFFF, PrivilegeLevel::Machine)
            .unwrap();

        assert_eq!(
            csr_file
                .read(csr_addr::MCOUNTEREN, PrivilegeLevel::Machine)
                .unwrap(),
            0xFFFF_FFFF
        );
    }

    #[test]
    fn test_unknown_machine_csr_compat_stub_roundtrip() {
        let mut csr_file = CsrFile::new();
        let csr = 0x30A;

        assert_eq!(csr_file.read(csr, PrivilegeLevel::Machine).unwrap(), 0);
        csr_file
            .write(csr, 0x1357_9BDF, PrivilegeLevel::Machine)
            .unwrap();
        assert_eq!(
            csr_file.read(csr, PrivilegeLevel::Machine).unwrap(),
            0x1357_9BDF
        );
    }

    #[test]
    fn test_user_counter_aliases_readable_from_supervisor() {
        let mut csr_file = CsrFile::new();
        csr_file.perf.mcycle.write_low(0x1122_3344);
        csr_file.perf.mcycleh.write(0x5566_7788);
        csr_file.perf.minstret.write_low(0x99AA_BBCC);
        csr_file.perf.minstreth.write(0xDDEE_FF00);

        assert_eq!(
            csr_file
                .read(csr_addr::CYCLE, PrivilegeLevel::Supervisor)
                .unwrap(),
            0x1122_3344
        );
        assert_eq!(
            csr_file
                .read(csr_addr::TIME, PrivilegeLevel::Supervisor)
                .unwrap(),
            0x1122_3344
        );
        assert_eq!(
            csr_file
                .read(csr_addr::INSTRET, PrivilegeLevel::Supervisor)
                .unwrap(),
            0x99AA_BBCC
        );
        assert_eq!(
            csr_file
                .read(csr_addr::CYCLEH, PrivilegeLevel::Supervisor)
                .unwrap(),
            0x5566_7788
        );
        assert_eq!(
            csr_file
                .read(csr_addr::TIMEH, PrivilegeLevel::Supervisor)
                .unwrap(),
            0x5566_7788
        );
        assert_eq!(
            csr_file
                .read(csr_addr::INSTRETH, PrivilegeLevel::Supervisor)
                .unwrap(),
            0xDDEE_FF00
        );
    }
}
