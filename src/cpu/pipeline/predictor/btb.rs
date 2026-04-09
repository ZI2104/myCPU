//! Branch Target Buffer (BTB).
//!
//! A small direct-mapped cache that maps branch/jump PC to target address.
//! Used alongside the direction predictor to provide both "will this branch
//! be taken?" and "if taken, where does it go?" predictions.

use serde::{Deserialize, Serialize};

use super::BTB_SIZE;
use crate::types::Addr;

/// Store target as raw u32 in BTB entries for serialization compatibility.

/// BTB statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BtbStats {
    pub lookups: u64,
    pub hits: u64,
    pub misses: u64,
}

impl BtbStats {
    pub fn hit_rate(&self) -> Option<f64> {
        if self.lookups == 0 {
            None
        } else {
            Some(self.hits as f64 / self.lookups as f64 * 100.0)
        }
    }
}

/// A single BTB entry: tag + target + metadata.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct BtbEntry {
    /// Tag (upper bits of PC) to distinguish colliding addresses.
    pub tag: u32,
    /// Predicted target address (raw u32 for serialization).
    pub target: u32,
    /// Whether this entry contains valid data.
    pub valid: bool,
    /// Whether the entry corresponds to a branch (vs. jump).
    pub is_branch: bool,
}

/// Extract BTB index from PC.
#[inline]
fn btb_index(pc: Addr) -> usize {
    ((pc.raw() >> 2) as usize) & (BTB_SIZE - 1)
}

/// Extract BTB tag from PC.
/// Tag = upper bits above the index field: pc >> (log2(BTB_SIZE) + 2) = pc >> 10
#[inline]
fn btb_tag(pc: Addr) -> u32 {
    pc.raw() >> 10
}

/// Branch Target Buffer: direct-mapped cache mapping PC to target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchTargetBuffer {
    entries: Vec<BtbEntry>,
    stats: BtbStats,
}

impl BranchTargetBuffer {
    /// Create a new BTB with `BTB_SIZE` entries (all invalid).
    pub fn new() -> Self {
        Self {
            entries: vec![BtbEntry::default(); BTB_SIZE],
            stats: BtbStats::default(),
        }
    }

    /// Look up the predicted target for a branch/jump at `pc`.
    /// Returns `Some(target)` if a valid entry matches, `None` otherwise.
    pub fn lookup(&mut self, pc: Addr) -> Option<Addr> {
        let idx = btb_index(pc);
        let tag = btb_tag(pc);
        let entry = &self.entries[idx];

        self.stats.lookups += 1;

        if entry.valid && entry.tag == tag {
            self.stats.hits += 1;
            Some(Addr::new(entry.target))
        } else {
            self.stats.misses += 1;
            None
        }
    }

    /// Update the BTB with a known branch/jump target.
    pub fn update(&mut self, pc: Addr, target: Addr, is_branch: bool) {
        let idx = btb_index(pc);
        let tag = btb_tag(pc);

        self.entries[idx] = BtbEntry {
            tag,
            target: target.raw(),
            valid: true,
            is_branch,
        };
    }

    /// Invalidate a specific entry (e.g., on misprediction with wrong target).
    pub fn invalidate(&mut self, pc: Addr) {
        let idx = btb_index(pc);
        self.entries[idx].valid = false;
    }

    /// Reset all entries and statistics.
    pub fn reset(&mut self) {
        for entry in &mut self.entries {
            *entry = BtbEntry::default();
        }
        self.stats = BtbStats::default();
    }

    /// Get BTB statistics.
    pub fn stats(&self) -> &BtbStats {
        &self.stats
    }

    /// Get a snapshot of valid BTB entries for visualization.
    /// Returns up to `limit` entries.
    pub fn snapshot(&self, limit: usize) -> Vec<BtbEntry> {
        self.entries
            .iter()
            .filter(|e| e.valid)
            .take(limit)
            .copied()
            .collect()
    }
}

impl Default for BranchTargetBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_btb_initial_empty() {
        let mut btb = BranchTargetBuffer::new();
        assert_eq!(btb.lookup(Addr::new(0x100)), None);
        assert_eq!(btb.stats().lookups, 1);
        assert_eq!(btb.stats().misses, 1);
        assert_eq!(btb.stats().hits, 0);
    }

    #[test]
    fn test_btb_update_lookup() {
        let mut btb = BranchTargetBuffer::new();
        btb.update(Addr::new(0x100), Addr::new(0x200), true);

        let target = btb.lookup(Addr::new(0x100));
        assert_eq!(target, Some(Addr::new(0x200)));
        assert_eq!(btb.stats().hits, 1);
    }

    #[test]
    fn test_btb_aliasing_eviction() {
        let mut btb = BranchTargetBuffer::new();
        // Two PCs that hash to the same BTB index
        // index = (pc >> 2) & (BTB_SIZE - 1), BTB_SIZE=256
        // PC=0x100 → index = 64
        // PC=0x100 + 256*4 = 0x500 → index = (0x500 >> 2) & 0xFF = 0x140 & 0xFF = 64
        let pc1 = Addr::new(0x100);
        let pc2 = Addr::new(0x100 + (BTB_SIZE as u32) * 4);

        btb.update(pc1, Addr::new(0x200), true);
        assert!(btb.lookup(pc1).is_some());

        // pc2 has same index but different tag → evicts pc1
        btb.update(pc2, Addr::new(0x300), true);
        assert!(btb.lookup(pc1).is_none()); // evicted
        assert_eq!(btb.lookup(pc2), Some(Addr::new(0x300)));
    }

    #[test]
    fn test_btb_different_indices_coexist() {
        let mut btb = BranchTargetBuffer::new();
        let pc1 = Addr::new(0x100); // index 64
        let pc2 = Addr::new(0x108); // index 66

        btb.update(pc1, Addr::new(0x200), true);
        btb.update(pc2, Addr::new(0x300), false);

        assert_eq!(btb.lookup(pc1), Some(Addr::new(0x200)));
        assert_eq!(btb.lookup(pc2), Some(Addr::new(0x300)));
    }

    #[test]
    fn test_btb_reset() {
        let mut btb = BranchTargetBuffer::new();
        btb.update(Addr::new(0x100), Addr::new(0x200), true);
        btb.lookup(Addr::new(0x100));

        btb.reset();

        assert_eq!(btb.lookup(Addr::new(0x100)), None);
        assert_eq!(btb.stats().lookups, 1); // fresh stats after reset
    }

    #[test]
    fn test_btb_stats_accuracy() {
        let mut btb = BranchTargetBuffer::new();
        btb.update(Addr::new(0x100), Addr::new(0x200), true);

        btb.lookup(Addr::new(0x100)); // hit
        btb.lookup(Addr::new(0x100)); // hit
        btb.lookup(Addr::new(0x500)); // miss

        let stats = btb.stats();
        assert_eq!(stats.lookups, 3);
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate().unwrap() - 66.667).abs() < 0.1);
    }

    #[test]
    fn test_btb_invalidate() {
        let mut btb = BranchTargetBuffer::new();
        btb.update(Addr::new(0x100), Addr::new(0x200), true);
        assert!(btb.lookup(Addr::new(0x100)).is_some());

        btb.invalidate(Addr::new(0x100));
        assert!(btb.lookup(Addr::new(0x100)).is_none());
    }

    #[test]
    fn test_btb_snapshot() {
        let mut btb = BranchTargetBuffer::new();
        assert!(btb.snapshot(10).is_empty());

        btb.update(Addr::new(0x100), Addr::new(0x200), true);
        btb.update(Addr::new(0x108), Addr::new(0x300), false);

        let snap = btb.snapshot(10);
        assert_eq!(snap.len(), 2);
        assert!(snap.iter().all(|e| e.valid));
        // Check target is stored as raw u32
        assert_eq!(snap[0].target, 0x200);
        assert_eq!(snap[1].target, 0x300);
    }
}
