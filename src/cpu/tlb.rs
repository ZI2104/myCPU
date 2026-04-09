//! Translation Lookaside Buffer (TLB) for Sv32 virtual memory.
//!
//! Caches recent virtual-to-physical address translations to avoid
//! repeated page table walks. 32-entry fully-associative design with
//! ASID support and random replacement.

use serde::{Deserialize, Serialize};

use crate::error::MemoryAccessType;
use crate::types::Addr;

/// Number of TLB entries.
pub const TLB_SIZE: usize = 32;

/// Result of a TLB lookup hit.
#[derive(Debug, Clone, Copy)]
pub struct TlbLookupResult {
    /// Translated physical address.
    pub paddr: Addr,
    /// Cached permission bits: bit0=R, bit1=W, bit2=X, bit3=U.
    pub rwxu: u8,
}

/// A single TLB entry.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct TlbEntry {
    pub valid: bool,
    /// Address Space Identifier (9-bit).
    pub asid: u16,
    /// Virtual Page Number [31:12].
    pub vpn: u32,
    /// Physical Page Number.
    pub ppn: u32,
    /// Cached permission bits: bit0=R, bit1=W, bit2=X, bit3=U.
    pub rwxu: u8,
}

/// TLB statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TlbStats {
    pub lookups: u64,
    pub hits: u64,
    pub misses: u64,
    pub flushes: u64,
}

impl TlbStats {
    pub fn hit_rate(&self) -> Option<f64> {
        if self.lookups == 0 {
            None
        } else {
            Some(self.hits as f64 / self.lookups as f64 * 100.0)
        }
    }
}

/// Extract VPN from virtual address (bits [31:12]).
#[inline]
fn vpn(vaddr: Addr) -> u32 {
    vaddr.raw() >> 12
}

/// Extract page offset from virtual address (bits [11:0]).
#[inline]
fn page_offset(vaddr: Addr) -> u32 {
    vaddr.raw() & 0xFFF
}

/// Check if a TLB entry permits the requested access.
fn check_permission(rwxu: u8, access: MemoryAccessType) -> bool {
    match access {
        MemoryAccessType::Instruction => (rwxu & 0x04) != 0, // X
        // Caller handles MXR policy; allow load lookup on R or X pages.
        MemoryAccessType::Load => (rwxu & 0x01) != 0 || (rwxu & 0x04) != 0, // R or X
        MemoryAccessType::Store => (rwxu & 0x02) != 0,                      // W
    }
}

/// Translation Lookaside Buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tlb {
    entries: Vec<TlbEntry>,
    /// Index for next insertion (round-robin replacement).
    next_insert: usize,
    stats: TlbStats,
}

impl Tlb {
    /// Create a new TLB with `TLB_SIZE` entries (all invalid).
    pub fn new() -> Self {
        Self {
            entries: vec![TlbEntry::default(); TLB_SIZE],
            next_insert: 0,
            stats: TlbStats::default(),
        }
    }

    /// Look up a virtual address translation.
    ///
    /// Returns `Some(TlbLookupResult)` on hit, `None` on miss.
    /// The caller is responsible for permission checks (SUM/MXR/U-bit)
    /// that depend on current CPU state.
    pub fn lookup(
        &mut self,
        vaddr: Addr,
        asid: u16,
        access: MemoryAccessType,
    ) -> Option<TlbLookupResult> {
        let vpn = vpn(vaddr);
        let offset = page_offset(vaddr);

        self.stats.lookups += 1;

        for entry in &self.entries {
            if entry.valid && entry.asid == asid && entry.vpn == vpn {
                // Check cached permissions
                if !check_permission(entry.rwxu, access) {
                    // Permission fault — treat as miss so walker re-checks
                    self.stats.misses += 1;
                    return None;
                }

                self.stats.hits += 1;
                let paddr = Addr::new((entry.ppn << 12) | offset);
                return Some(TlbLookupResult {
                    paddr,
                    rwxu: entry.rwxu,
                });
            }
        }

        self.stats.misses += 1;
        None
    }

    /// Insert a new translation into the TLB.
    pub fn insert(&mut self, vaddr: Addr, paddr: Addr, asid: u16, rwxu: u8) {
        let idx = self.next_insert;
        self.next_insert = (self.next_insert + 1) % TLB_SIZE;

        self.entries[idx] = TlbEntry {
            valid: true,
            asid,
            vpn: vpn(vaddr),
            ppn: paddr.raw() >> 12,
            rwxu,
        };
    }

    /// Flush all TLB entries (SFENCE.VMA with rs1=x0, rs2=x0).
    pub fn flush_all(&mut self) {
        for entry in &mut self.entries {
            *entry = TlbEntry::default();
        }
        self.stats.flushes += 1;
    }

    /// Flush entries matching a specific ASID.
    pub fn flush_asid(&mut self, asid: u16) {
        for entry in &mut self.entries {
            if entry.valid && entry.asid == asid {
                *entry = TlbEntry::default();
            }
        }
        self.stats.flushes += 1;
    }

    /// Flush entries matching a specific virtual address and ASID.
    pub fn flush_vaddr(&mut self, vaddr: Addr, asid: u16) {
        let vpn = vpn(vaddr);
        for entry in &mut self.entries {
            if entry.valid && entry.asid == asid && entry.vpn == vpn {
                *entry = TlbEntry::default();
            }
        }
        self.stats.flushes += 1;
    }

    /// Reset all state.
    pub fn reset(&mut self) {
        for entry in &mut self.entries {
            *entry = TlbEntry::default();
        }
        self.next_insert = 0;
        self.stats = TlbStats::default();
    }

    /// Get TLB statistics.
    pub fn stats(&self) -> &TlbStats {
        &self.stats
    }

    /// Get a snapshot of valid entries for visualization.
    pub fn snapshot(&self, limit: usize) -> Vec<TlbEntry> {
        self.entries
            .iter()
            .filter(|e| e.valid)
            .take(limit)
            .copied()
            .collect()
    }
}

impl Default for Tlb {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tlb_initial_empty() {
        let mut tlb = Tlb::new();
        assert!(tlb
            .lookup(Addr::new(0x1000), 0, MemoryAccessType::Load)
            .is_none());
        assert_eq!(tlb.stats().misses, 1);
    }

    #[test]
    fn test_tlb_insert_lookup() {
        let mut tlb = Tlb::new();
        let vaddr = Addr::new(0x1000);
        let paddr = Addr::new(0x2000);

        // Insert: vaddr 0x1000 → paddr 0x2000, R=1, W=0, X=0, U=0
        tlb.insert(vaddr, paddr, 0, 0x01);

        let result = tlb.lookup(vaddr, 0, MemoryAccessType::Load).unwrap();
        assert_eq!(result.paddr, Addr::new(0x2000));
        assert_eq!(tlb.stats().hits, 1);
    }

    #[test]
    fn test_tlb_page_offset_preserved() {
        let mut tlb = Tlb::new();
        tlb.insert(Addr::new(0x1000), Addr::new(0xABCDE000), 0, 0x01);

        let result = tlb
            .lookup(Addr::new(0x1234), 0, MemoryAccessType::Load)
            .unwrap();
        assert_eq!(result.paddr, Addr::new(0xABCDE234));
    }

    #[test]
    fn test_tlb_asid_isolation() {
        let mut tlb = Tlb::new();
        tlb.insert(Addr::new(0x1000), Addr::new(0x2000), 1, 0x01);

        // Different ASID → miss
        assert!(tlb
            .lookup(Addr::new(0x1000), 2, MemoryAccessType::Load)
            .is_none());
        // Same ASID → hit
        assert!(tlb
            .lookup(Addr::new(0x1000), 1, MemoryAccessType::Load)
            .is_some());
    }

    #[test]
    fn test_tlb_permission_check() {
        let mut tlb = Tlb::new();
        // R=1 only
        tlb.insert(Addr::new(0x1000), Addr::new(0x2000), 0, 0x01);

        assert!(tlb
            .lookup(Addr::new(0x1000), 0, MemoryAccessType::Load)
            .is_some());
        // Store needs W → miss (permission fault)
        assert!(tlb
            .lookup(Addr::new(0x1000), 0, MemoryAccessType::Store)
            .is_none());
        // Execute needs X → miss
        assert!(tlb
            .lookup(Addr::new(0x1000), 0, MemoryAccessType::Instruction)
            .is_none());
    }

    #[test]
    fn test_tlb_flush_all() {
        let mut tlb = Tlb::new();
        tlb.insert(Addr::new(0x1000), Addr::new(0x2000), 0, 0x01);
        tlb.flush_all();
        assert!(tlb
            .lookup(Addr::new(0x1000), 0, MemoryAccessType::Load)
            .is_none());
    }

    #[test]
    fn test_tlb_flush_asid() {
        let mut tlb = Tlb::new();
        tlb.insert(Addr::new(0x1000), Addr::new(0x2000), 1, 0x01);
        tlb.insert(Addr::new(0x2000), Addr::new(0x3000), 2, 0x01);

        tlb.flush_asid(1);

        assert!(tlb
            .lookup(Addr::new(0x1000), 1, MemoryAccessType::Load)
            .is_none());
        assert!(tlb
            .lookup(Addr::new(0x2000), 2, MemoryAccessType::Load)
            .is_some());
    }

    #[test]
    fn test_tlb_flush_vaddr() {
        let mut tlb = Tlb::new();
        tlb.insert(Addr::new(0x1000), Addr::new(0x2000), 0, 0x01);
        tlb.insert(Addr::new(0x2000), Addr::new(0x3000), 0, 0x01);

        tlb.flush_vaddr(Addr::new(0x1000), 0);

        assert!(tlb
            .lookup(Addr::new(0x1000), 0, MemoryAccessType::Load)
            .is_none());
        assert!(tlb
            .lookup(Addr::new(0x2000), 0, MemoryAccessType::Load)
            .is_some());
    }

    #[test]
    fn test_tlb_round_robin_replacement() {
        let mut tlb = Tlb::new();
        // Fill all entries
        for i in 0..TLB_SIZE as u32 {
            tlb.insert(Addr::new(i << 12), Addr::new((i + 100) << 12), 0, 0x01);
        }
        // All should still be present
        for i in 0..TLB_SIZE as u32 {
            assert!(tlb
                .lookup(Addr::new(i << 12), 0, MemoryAccessType::Load)
                .is_some());
        }
        // Insert one more → evicts entry 0
        tlb.insert(Addr::new(0xFF000), Addr::new(0xEE000), 0, 0x01);
        assert!(tlb
            .lookup(Addr::new(0x0), 0, MemoryAccessType::Load)
            .is_none()); // evicted
        assert!(tlb
            .lookup(Addr::new(0xFF000), 0, MemoryAccessType::Load)
            .is_some());
    }

    #[test]
    fn test_tlb_reset() {
        let mut tlb = Tlb::new();
        tlb.insert(Addr::new(0x1000), Addr::new(0x2000), 0, 0x01);
        tlb.lookup(Addr::new(0x1000), 0, MemoryAccessType::Load);

        tlb.reset();

        assert!(tlb
            .lookup(Addr::new(0x1000), 0, MemoryAccessType::Load)
            .is_none());
        assert_eq!(tlb.stats().lookups, 1); // fresh stats
    }
}
