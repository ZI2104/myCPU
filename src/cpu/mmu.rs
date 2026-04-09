//! MMU (Memory Management Unit) helpers for RV32 Sv32 translation.
//!
//! This module currently provides a software page-table walker used by the CPU
//! to translate virtual addresses when `satp.MODE=Sv32`.

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
}

/// Translate a virtual address into a physical address.
///
/// Translation policy for current milestone:
/// - M-mode: identity mapping (no translation)
/// - satp.MODE=0 (Bare): identity mapping
/// - satp.MODE=1 (Sv32): two-level page table walk
pub fn translate_addr(
    bus: &Bus,
    satp: &Satp,
    privilege: PrivilegeLevel,
    vaddr: Addr,
    access: MemoryAccessType,
) -> Result<Addr> {
    if privilege == PrivilegeLevel::Machine || !satp.is_sv32() {
        return Ok(vaddr);
    }

    translate_sv32(bus, satp, vaddr, access)
}

fn page_fault(vaddr: Addr, access: MemoryAccessType) -> SimError {
    SimError::PageFault {
        addr: vaddr,
        access,
    }
}

fn is_leaf(pte: u32) -> bool {
    let r = (pte & pte_bits::R) != 0;
    let x = (pte & pte_bits::X) != 0;
    r || x
}

fn is_pte_valid(pte: u32) -> bool {
    let v = (pte & pte_bits::V) != 0;
    let r = (pte & pte_bits::R) != 0;
    let w = (pte & pte_bits::W) != 0;
    v && !(w && !r)
}

fn check_leaf_permission(pte: u32, access: MemoryAccessType) -> bool {
    let r = (pte & pte_bits::R) != 0;
    let w = (pte & pte_bits::W) != 0;
    let x = (pte & pte_bits::X) != 0;

    match access {
        MemoryAccessType::Instruction => x,
        MemoryAccessType::Load => r,
        MemoryAccessType::Store => w,
    }
}

fn translate_sv32(bus: &Bus, satp: &Satp, vaddr: Addr, access: MemoryAccessType) -> Result<Addr> {
    // Sv32 virtual address split:
    // VPN[1] = va[31:22], VPN[0] = va[21:12], page offset = va[11:0]
    let va = vaddr.raw();
    let vpn = [(va >> 12) & 0x3FF, (va >> 22) & 0x3FF];
    let page_offset = va & 0xFFF;

    // Root page table base physical address.
    let mut table_addr = satp.root_table_addr().raw();

    // Walk level 1 -> 0.
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
            if !check_leaf_permission(pte, access) {
                return Err(page_fault(vaddr, access));
            }

            let ppn = (pte >> 10) & 0x003F_FFFF;
            let ppn0 = ppn & 0x3FF;
            let ppn1 = (ppn >> 10) & 0xFFF;

            let phys = if level == 1 {
                // Superpage leaf: lower PPN bits must be zero.
                if ppn0 != 0 {
                    return Err(page_fault(vaddr, access));
                }
                (ppn1 << 22) | (vpn[0] << 12) | page_offset
            } else {
                (ppn1 << 22) | (ppn0 << 12) | page_offset
            };

            return Ok(Addr::new(phys));
        }

        // Non-leaf entry at level 0 is invalid in Sv32.
        if level == 0 {
            return Err(page_fault(vaddr, access));
        }

        // Next level table base from PPN.
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
        let ram = Ram::new(0x20000); // 128 KiB for page tables and data
        bus.attach_memory(Addr::new(0), ram, "RAM");
        bus
    }

    #[test]
    fn test_translate_bare_identity() {
        let bus = setup_bus();
        let satp = Satp::new(); // mode=0 Bare
        let va = Addr::new(0x8040_1234);

        let pa = translate_addr(
            &bus,
            &satp,
            PrivilegeLevel::Supervisor,
            va,
            MemoryAccessType::Instruction,
        )
        .unwrap();

        assert_eq!(pa, va);
    }

    #[test]
    fn test_translate_sv32_two_level_leaf() {
        let mut bus = setup_bus();

        // Root table at 0x1000 (PPN=1), level-0 table at 0x2000 (PPN=2)
        let mut satp = Satp::new();
        satp.write((1 << 31) | 0x1);

        let vpn1 = 1u32;
        let vpn0 = 2u32;
        let offset = 0x34u32;
        let va = Addr::new((vpn1 << 22) | (vpn0 << 12) | offset);

        // Root PTE -> next level table (V=1)
        let root_pte_addr = Addr::new(0x1000 + vpn1 * 4);
        let root_pte = (0x2 << 10) | pte_bits::V;
        bus.write_word(root_pte_addr, Word::new(root_pte)).unwrap();

        // Leaf PTE in level-0 table -> physical page PPN=0x345, flags V|R|W|X
        let leaf_pte_addr = Addr::new(0x2000 + vpn0 * 4);
        let leaf_pte = (0x345 << 10) | pte_bits::V | pte_bits::R | pte_bits::W | pte_bits::X;
        bus.write_word(leaf_pte_addr, Word::new(leaf_pte)).unwrap();

        let pa = translate_addr(
            &bus,
            &satp,
            PrivilegeLevel::Supervisor,
            va,
            MemoryAccessType::Load,
        )
        .unwrap();

        assert_eq!(pa, Addr::new((0x345 << 12) | offset));
    }

    #[test]
    fn test_translate_sv32_permission_fault() {
        let mut bus = setup_bus();

        let mut satp = Satp::new();
        satp.write((1 << 31) | 0x1);

        let vpn1 = 0u32;
        let vpn0 = 1u32;
        let va = Addr::new((vpn1 << 22) | (vpn0 << 12));

        // Root PTE -> next level
        bus.write_word(
            Addr::new(vpn1 * 4 + 0x1000),
            Word::new((0x2 << 10) | pte_bits::V),
        )
        .unwrap();

        // Leaf PTE has R but no X
        let leaf_pte = (0x120 << 10) | pte_bits::V | pte_bits::R;
        bus.write_word(Addr::new(vpn0 * 4 + 0x2000), Word::new(leaf_pte))
            .unwrap();

        let err = translate_addr(
            &bus,
            &satp,
            PrivilegeLevel::Supervisor,
            va,
            MemoryAccessType::Instruction,
        )
        .unwrap_err();

        assert!(matches!(
            err,
            SimError::PageFault {
                access: MemoryAccessType::Instruction,
                ..
            }
        ));
    }
}
