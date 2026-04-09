//! MMU (Memory Management Unit) for RV32 Sv32 translation.
//!
//! Implements:
//! - Bare mode (identity mapping)
//! - Sv32 two-level page table walk
//! - Permission checking (R/W/X/U bits)
//! - A (Accessed) / D (Dirty) bit hardware update
//! - SUM (Supervisor User Memory) and MXR (Make eXecutable Readable)
//! - TlbResult for TLB integration

use crate::cpu::csr::Satp;
use crate::error::{MemoryAccessType, Result, SimError};
use crate::memory::Bus;
use crate::types::{Addr, PrivilegeLevel};

const PTE_SIZE: u32 = 4;

mod pte_bits {
    pub const V: u32 = 1 << 0;
    pub const R: u32 = 1 << 1;
    pub const W: u32 = 1 << 2;
    pub const X: u32 = 1 << 3;
    pub const U: u32 = 1 << 4; // User-mode page
    pub const A: u32 = 1 << 6; // Accessed
    pub const D: u32 = 1 << 7; // Dirty
}

/// Result of an Sv32 translation, carrying physical address and PTE flags
/// for TLB fill.
#[derive(Debug, Clone, Copy)]
pub struct TlbResult {
    pub paddr: Addr,
    /// Cached PTE permission bits: bit0=R, bit1=W, bit2=X, bit3=U.
    pub rwxu: u8,
    /// Whether the A bit was set by hardware.
    pub accessed: bool,
    /// Whether the D bit was set by hardware.
    pub dirty: bool,
}

/// Translate a virtual address into a physical address (no TLB).
///
/// Policy:
/// - M-mode: identity mapping
/// - satp.MODE=0 (Bare): identity mapping
/// - satp.MODE=1 (Sv32): two-level page table walk with full permission checks
pub fn translate_addr(
    bus: &mut Bus,
    satp: &Satp,
    privilege: PrivilegeLevel,
    vaddr: Addr,
    access: MemoryAccessType,
    sstatus_sum: bool,
    sstatus_mxr: bool,
) -> Result<Addr> {
    if privilege == PrivilegeLevel::Machine || !satp.is_sv32() {
        return Ok(vaddr);
    }

    translate_sv32(bus, satp, privilege, vaddr, access, sstatus_sum, sstatus_mxr)
        .map(|r| r.paddr)
}

/// Translate and return full TlbResult for TLB fill.
pub fn translate_addr_full(
    bus: &mut Bus,
    satp: &Satp,
    privilege: PrivilegeLevel,
    vaddr: Addr,
    access: MemoryAccessType,
    sstatus_sum: bool,
    sstatus_mxr: bool,
) -> Result<TlbResult> {
    if privilege == PrivilegeLevel::Machine || !satp.is_sv32() {
        return Ok(TlbResult {
            paddr: vaddr,
            rwxu: 0xF, // full access in bare/m-mode
            accessed: false,
            dirty: false,
        });
    }

    translate_sv32(bus, satp, privilege, vaddr, access, sstatus_sum, sstatus_mxr)
}

fn page_fault(vaddr: Addr, access: MemoryAccessType) -> SimError {
    SimError::PageFault {
        addr: vaddr,
        access,
    }
}

fn is_leaf(pte: u32) -> bool {
    (pte & (pte_bits::R | pte_bits::X)) != 0
}

fn is_pte_valid(pte: u32) -> bool {
    let v = (pte & pte_bits::V) != 0;
    let r = (pte & pte_bits::R) != 0;
    let w = (pte & pte_bits::W) != 0;
    v && !(w && !r)
}

/// Check leaf permissions including U-bit, SUM, and MXR.
fn check_leaf_permission(
    pte: u32,
    privilege: PrivilegeLevel,
    access: MemoryAccessType,
    sstatus_sum: bool,
    sstatus_mxr: bool,
) -> bool {
    let r = (pte & pte_bits::R) != 0;
    let w = (pte & pte_bits::W) != 0;
    let x = (pte & pte_bits::X) != 0;
    let u = (pte & pte_bits::U) != 0;

    // U-bit vs privilege level check
    match privilege {
        PrivilegeLevel::User => {
            // U-mode can only access U=1 pages
            if !u {
                return false;
            }
        }
        PrivilegeLevel::Supervisor => {
            // S-mode cannot access U=1 pages unless SUM=1 (Load/Store only)
            if u && !sstatus_sum && !matches!(access, MemoryAccessType::Instruction) {
                return false;
            }
            // S-mode can never fetch instructions from U=1 pages
            if u && matches!(access, MemoryAccessType::Instruction) {
                return false;
            }
        }
        PrivilegeLevel::Machine => {} // M-mode checked before calling
    }

    // Permission bit check
    match access {
        MemoryAccessType::Instruction => x,
        MemoryAccessType::Load => r || (sstatus_mxr && x),
        MemoryAccessType::Store => w,
    }
}

fn translate_sv32(
    bus: &mut Bus,
    satp: &Satp,
    privilege: PrivilegeLevel,
    vaddr: Addr,
    access: MemoryAccessType,
    sstatus_sum: bool,
    sstatus_mxr: bool,
) -> Result<TlbResult> {
    let va = vaddr.raw();
    let vpn = [(va >> 12) & 0x3FF, (va >> 22) & 0x3FF];
    let page_offset = va & 0xFFF;

    let mut table_addr = satp.root_table_addr().raw();

    for level in (0..=1).rev() {
        let pte_addr = Addr::new(table_addr.wrapping_add(vpn[level] * PTE_SIZE));
        let pte = bus
            .read_word(pte_addr)
            .map_err(|_| page_fault(vaddr, access))?
            .raw();

        if !is_pte_valid(pte) {
            return Err(page_fault(vaddr, access));
        }

        if is_leaf(pte) {
            if !check_leaf_permission(pte, privilege, access, sstatus_sum, sstatus_mxr) {
                return Err(page_fault(vaddr, access));
            }

            let ppn = (pte >> 10) & 0x003F_FFFF;
            let ppn0 = ppn & 0x3FF;
            let ppn1 = (ppn >> 10) & 0x3FF;

            let phys = if level == 1 {
                if ppn0 != 0 {
                    return Err(page_fault(vaddr, access));
                }
                (ppn1 << 22) | (vpn[0] << 12) | page_offset
            } else {
                (ppn1 << 22) | (ppn0 << 12) | page_offset
            };

            // Hardware A/D bit update
            let mut need_writeback = false;
            let mut updated_pte = pte;

            // Set A bit if not already set
            if (pte & pte_bits::A) == 0 {
                updated_pte |= pte_bits::A;
                need_writeback = true;
            }

            // Set D bit on Store if not already set
            let dirty = if matches!(access, MemoryAccessType::Store) && (pte & pte_bits::D) == 0 {
                updated_pte |= pte_bits::D;
                need_writeback = true;
                true
            } else {
                false
            };

            if need_writeback {
                let _ = bus.write_word(pte_addr, crate::types::Word::new(updated_pte));
            }

            // Pack rwxu flags for TLB
            let rwxu = (if (pte & pte_bits::R) != 0 { 1 } else { 0 })
                | (if (pte & pte_bits::W) != 0 { 2 } else { 0 })
                | (if (pte & pte_bits::X) != 0 { 4 } else { 0 })
                | (if (pte & pte_bits::U) != 0 { 8 } else { 0 });

            return Ok(TlbResult {
                paddr: Addr::new(phys),
                rwxu,
                accessed: need_writeback && (pte & pte_bits::A) == 0,
                dirty,
            });
        }

        if level == 0 {
            return Err(page_fault(vaddr, access));
        }

        table_addr = ((pte >> 10) & 0x003F_FFFF) << 12;
    }

    Err(page_fault(vaddr, access))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Ram;
    use crate::types::Word;

    fn setup_bus() -> Bus {
        let mut bus = Bus::new();
        let ram = Ram::new(0x20000);
        bus.attach_memory(Addr::new(0), ram, "RAM");
        bus
    }

    fn make_satp_sv32(ppn: u32) -> Satp {
        let mut satp = Satp::new();
        satp.write((1u32 << 31) | (ppn & 0x3FFFFF));
        satp
    }

    #[test]
    fn test_translate_bare_identity() {
        let mut bus = setup_bus();
        let satp = Satp::new();
        let va = Addr::new(0x8040_1234);

        let pa = translate_addr(
            &mut bus, &satp, PrivilegeLevel::Supervisor, va,
            MemoryAccessType::Instruction, false, false,
        )
        .unwrap();

        assert_eq!(pa, va);
    }

    #[test]
    fn test_translate_sv32_two_level_leaf() {
        let mut bus = setup_bus();
        let satp = make_satp_sv32(0x1); // root at 0x1000

        let vpn1 = 1u32;
        let vpn0 = 2u32;
        let offset = 0x34u32;
        let va = Addr::new((vpn1 << 22) | (vpn0 << 12) | offset);

        // Root PTE -> next level table (V=1)
        let root_pte = (0x2 << 10) | pte_bits::V | pte_bits::A;
        bus.write_word(
            Addr::new(0x1000 + vpn1 * 4),
            Word::new(root_pte),
        )
        .unwrap();

        // Leaf PTE: PPN=0x345, flags V|R|W|X|A
        let leaf_pte = (0x345 << 10) | pte_bits::V | pte_bits::R | pte_bits::W | pte_bits::X | pte_bits::A;
        bus.write_word(
            Addr::new(0x2000 + vpn0 * 4),
            Word::new(leaf_pte),
        )
        .unwrap();

        let pa = translate_addr(
            &mut bus, &satp, PrivilegeLevel::Supervisor, va,
            MemoryAccessType::Load, false, false,
        )
        .unwrap();

        assert_eq!(pa, Addr::new((0x345 << 12) | offset));
    }

    #[test]
    fn test_sv32_permission_fault_no_x() {
        let mut bus = setup_bus();
        let satp = make_satp_sv32(0x1);

        let va = Addr::new((0u32 << 22) | (1u32 << 12));

        // Root PTE -> next level
        bus.write_word(
            Addr::new(0x1000),
            Word::new((0x2 << 10) | pte_bits::V | pte_bits::A),
        )
        .unwrap();

        // Leaf PTE: R but no X
        bus.write_word(
            Addr::new(0x2000 + 4),
            Word::new((0x120 << 10) | pte_bits::V | pte_bits::R | pte_bits::A),
        )
        .unwrap();

        let err = translate_addr(
            &mut bus, &satp, PrivilegeLevel::Supervisor, va,
            MemoryAccessType::Instruction, false, false,
        )
        .unwrap_err();

        assert!(matches!(err, SimError::PageFault { access: MemoryAccessType::Instruction, .. }));
    }

    #[test]
    fn test_sv32_u_bit_blocks_s_mode() {
        let mut bus = setup_bus();
        let satp = make_satp_sv32(0x1);

        let va = Addr::new((0u32 << 22) | (1u32 << 12));

        // Root -> next level
        bus.write_word(
            Addr::new(0x1000),
            Word::new((0x2 << 10) | pte_bits::V | pte_bits::A),
        )
        .unwrap();

        // Leaf: U=1, R=1 — user page
        bus.write_word(
            Addr::new(0x2000 + 4),
            Word::new((0x120 << 10) | pte_bits::V | pte_bits::R | pte_bits::U | pte_bits::A),
        )
        .unwrap();

        // S-mode without SUM → should fault
        let err = translate_addr(
            &mut bus, &satp, PrivilegeLevel::Supervisor, va,
            MemoryAccessType::Load, false, // sum=false
            false,
        )
        .unwrap_err();
        assert!(matches!(err, SimError::PageFault { .. }));

        // S-mode with SUM=1 → should succeed
        let pa = translate_addr(
            &mut bus, &satp, PrivilegeLevel::Supervisor, va,
            MemoryAccessType::Load, true, // sum=true
            false,
        )
        .unwrap();
        assert_eq!(pa, Addr::new(0x120 << 12));
    }

    #[test]
    fn test_sv32_mxr_allows_read_from_x_page() {
        let mut bus = setup_bus();
        let satp = make_satp_sv32(0x1);

        let va = Addr::new((0u32 << 22) | (1u32 << 12));

        // Root -> next level
        bus.write_word(
            Addr::new(0x1000),
            Word::new((0x2 << 10) | pte_bits::V | pte_bits::A),
        )
        .unwrap();

        // Leaf: X=1 but R=0 — execute-only page
        bus.write_word(
            Addr::new(0x2000 + 4),
            Word::new((0x120 << 10) | pte_bits::V | pte_bits::X | pte_bits::A),
        )
        .unwrap();

        // Without MXR → Load should fail (no R bit)
        let err = translate_addr(
            &mut bus, &satp, PrivilegeLevel::Supervisor, va,
            MemoryAccessType::Load, false,
            false, // mxr=false
        )
        .unwrap_err();
        assert!(matches!(err, SimError::PageFault { .. }));

        // With MXR=1 → Load should succeed (X+MXR allows read)
        let pa = translate_addr(
            &mut bus, &satp, PrivilegeLevel::Supervisor, va,
            MemoryAccessType::Load, false,
            true, // mxr=true
        )
        .unwrap();
        assert_eq!(pa, Addr::new(0x120 << 12));
    }

    #[test]
    fn test_sv32_a_bit_hardware_update() {
        let mut bus = setup_bus();
        let satp = make_satp_sv32(0x1);

        let va = Addr::new((0u32 << 22) | (1u32 << 12));

        // Root -> next level
        bus.write_word(
            Addr::new(0x1000),
            Word::new((0x2 << 10) | pte_bits::V | pte_bits::A),
        )
        .unwrap();

        // Leaf: A=0 initially, should be set by hardware
        let leaf_addr = Addr::new(0x2000 + 4);
        bus.write_word(
            leaf_addr,
            Word::new((0x120 << 10) | pte_bits::V | pte_bits::R | pte_bits::X),
        )
        .unwrap();

        let _pa = translate_addr_full(
            &mut bus, &satp, PrivilegeLevel::Supervisor, va,
            MemoryAccessType::Load, false, false,
        )
        .unwrap();

        // Verify A bit was set in page table
        let pte_after = bus.read_word(leaf_addr).unwrap().raw();
        assert_ne!(pte_after & pte_bits::A, 0, "A bit should be set by hardware");
    }

    #[test]
    fn test_sv32_d_bit_on_store() {
        let mut bus = setup_bus();
        let satp = make_satp_sv32(0x1);

        let va = Addr::new((0u32 << 22) | (1u32 << 12));

        // Root -> next level
        bus.write_word(
            Addr::new(0x1000),
            Word::new((0x2 << 10) | pte_bits::V | pte_bits::A),
        )
        .unwrap();

        // Leaf: D=0, A=0 initially
        let leaf_addr = Addr::new(0x2000 + 4);
        bus.write_word(
            leaf_addr,
            Word::new((0x120 << 10) | pte_bits::V | pte_bits::R | pte_bits::W | pte_bits::X),
        )
        .unwrap();

        let result = translate_addr_full(
            &mut bus, &satp, PrivilegeLevel::Supervisor, va,
            MemoryAccessType::Store, false, false,
        )
        .unwrap();

        assert!(result.dirty, "D bit should be set on Store");
        assert!(result.accessed, "A bit should also be set");

        // Verify D and A bits in page table
        let pte_after = bus.read_word(leaf_addr).unwrap().raw();
        assert_ne!(pte_after & pte_bits::D, 0, "D bit should be set");
        assert_ne!(pte_after & pte_bits::A, 0, "A bit should be set");
    }
}
