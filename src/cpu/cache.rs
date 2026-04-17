//! Cache simulation for the RISC-V pipeline.
//!
//! Implements configurable set-associative I-Cache and D-Cache.
//! D-Cache defaults to write-back + write-allocate.

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::memory::Bus;
use crate::types::{Addr, Byte, Half, Word};

/// Default I-Cache size (4 KB).
pub const ICACHE_SIZE: usize = 4096;
/// Default D-Cache size (4 KB).
pub const DCACHE_SIZE: usize = 4096;
/// Cache line size (16 bytes).
pub const CACHE_LINE_SIZE: usize = 16;
/// Default I-Cache associativity.
pub const ICACHE_ASSOCIATIVITY: usize = 4;
/// Default D-Cache associativity.
pub const DCACHE_ASSOCIATIVITY: usize = 4;

/// Write policy for cache lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WritePolicy {
    WriteThrough,
    WriteBack,
}

/// Write-allocate behavior on store miss.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WriteAllocatePolicy {
    WriteAllocate,
    NoWriteAllocate,
}

/// Replacement policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplacementPolicy {
    RoundRobin,
}

/// Cache configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheConfig {
    pub associativity: usize,
    pub replacement: ReplacementPolicy,
    pub write_policy: WritePolicy,
    pub write_allocate: WriteAllocatePolicy,
}

/// Cache statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub read_hits: u64,
    pub read_misses: u64,
    pub write_hits: u64,
    pub write_misses: u64,
    pub evictions: u64,
    pub writebacks: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> Option<f64> {
        let total = self.hits + self.misses;
        if total == 0 {
            None
        } else {
            Some(self.hits as f64 / total as f64 * 100.0)
        }
    }

    pub fn read_hit_rate(&self) -> Option<f64> {
        let total = self.read_hits + self.read_misses;
        if total == 0 {
            None
        } else {
            Some(self.read_hits as f64 / total as f64 * 100.0)
        }
    }

    pub fn write_hit_rate(&self) -> Option<f64> {
        let total = self.write_hits + self.write_misses;
        if total == 0 {
            None
        } else {
            Some(self.write_hits as f64 / total as f64 * 100.0)
        }
    }
}

/// A single cache line.
#[derive(Debug, Clone)]
struct CacheLine {
    valid: bool,
    dirty: bool,
    tag: u32,
    data: [u8; CACHE_LINE_SIZE],
}

impl Default for CacheLine {
    fn default() -> Self {
        Self {
            valid: false,
            dirty: false,
            tag: 0,
            data: [0; CACHE_LINE_SIZE],
        }
    }
}

/// Cache type discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheType {
    Instruction,
    Data,
}

impl CacheType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Instruction => "I-Cache",
            Self::Data => "D-Cache",
        }
    }
}

/// Set-associative cache.
#[derive(Debug, Clone)]
pub struct Cache {
    cache_type: CacheType,
    config: CacheConfig,
    set_count: usize,
    ways: usize,
    line_mask: usize,
    set_bits: usize,
    sets: Vec<Vec<CacheLine>>,
    next_victim: Vec<usize>,
    stats: CacheStats,
}

impl Cache {
    /// Create a new cache with default policy by cache type.
    pub fn new(cache_type: CacheType, size: usize) -> Self {
        let config = match cache_type {
            CacheType::Instruction => CacheConfig {
                associativity: ICACHE_ASSOCIATIVITY,
                replacement: ReplacementPolicy::RoundRobin,
                write_policy: WritePolicy::WriteThrough,
                write_allocate: WriteAllocatePolicy::NoWriteAllocate,
            },
            CacheType::Data => CacheConfig {
                associativity: DCACHE_ASSOCIATIVITY,
                replacement: ReplacementPolicy::RoundRobin,
                write_policy: WritePolicy::WriteBack,
                write_allocate: WriteAllocatePolicy::WriteAllocate,
            },
        };
        Self::new_with_config(cache_type, size, config)
    }

    /// Create a cache with explicit configuration.
    pub fn new_with_config(cache_type: CacheType, size: usize, config: CacheConfig) -> Self {
        assert!(size >= CACHE_LINE_SIZE, "cache size too small");
        assert!(size.is_power_of_two(), "cache size must be power-of-two");
        assert!(
            config.associativity > 0 && config.associativity.is_power_of_two(),
            "associativity must be power-of-two"
        );

        let total_lines = size / CACHE_LINE_SIZE;
        assert!(
            total_lines >= config.associativity,
            "associativity larger than total lines"
        );
        assert!(
            total_lines % config.associativity == 0,
            "total lines must be divisible by associativity"
        );

        let set_count = total_lines / config.associativity;
        assert!(
            set_count.is_power_of_two(),
            "set count must be power-of-two"
        );

        let set_bits = set_count.trailing_zeros() as usize;
        let line_mask = CACHE_LINE_SIZE - 1;

        Self {
            cache_type,
            config,
            set_count,
            ways: config.associativity,
            line_mask,
            set_bits,
            sets: vec![vec![CacheLine::default(); config.associativity]; set_count],
            next_victim: vec![0; set_count],
            stats: CacheStats::default(),
        }
    }

    /// Compute set index from physical address.
    #[inline]
    fn set_index(&self, paddr: Addr) -> usize {
        ((paddr.raw() as usize) >> 4) & (self.set_count - 1)
    }

    /// Compute tag from physical address.
    #[inline]
    fn tag(&self, paddr: Addr) -> u32 {
        paddr.raw() >> (4 + self.set_bits)
    }

    /// Compute byte offset within cache line.
    #[inline]
    fn line_offset(&self, paddr: Addr) -> usize {
        (paddr.raw() as usize) & self.line_mask
    }

    /// Align address to cache line base.
    #[inline]
    fn line_base(&self, paddr: Addr) -> Addr {
        Addr::new(paddr.raw() & !(self.line_mask as u32))
    }

    /// Reconstruct line base from set + tag.
    #[inline]
    fn line_base_from_set_tag(&self, set_idx: usize, tag: u32) -> Addr {
        let set_part = (set_idx as u32) << 4;
        let base = (tag << (4 + self.set_bits)) | set_part;
        Addr::new(base)
    }

    #[inline]
    fn find_way(&self, set_idx: usize, tag: u32) -> Option<usize> {
        self.sets[set_idx]
            .iter()
            .position(|line| line.valid && line.tag == tag)
    }

    #[inline]
    fn choose_victim_way(&mut self, set_idx: usize) -> usize {
        if let Some(invalid_way) = self.sets[set_idx].iter().position(|line| !line.valid) {
            return invalid_way;
        }

        match self.config.replacement {
            ReplacementPolicy::RoundRobin => {
                let victim = self.next_victim[set_idx];
                self.next_victim[set_idx] = (victim + 1) % self.ways;
                victim
            }
        }
    }

    /// Write back one dirty line if required.
    fn writeback_line(&mut self, set_idx: usize, way: usize, bus: &mut Bus) -> Result<()> {
        let (need_writeback, tag, data) = {
            let line = &self.sets[set_idx][way];
            (
                line.valid
                    && line.dirty
                    && self.cache_type == CacheType::Data
                    && self.config.write_policy == WritePolicy::WriteBack,
                line.tag,
                line.data,
            )
        };

        if !need_writeback {
            return Ok(());
        }

        let base = self.line_base_from_set_tag(set_idx, tag);
        for (i, value) in data.iter().enumerate() {
            bus.write_byte(Addr::new(base.raw() + i as u32), Byte::new(*value))?;
        }

        self.sets[set_idx][way].dirty = false;
        self.stats.writebacks += 1;
        Ok(())
    }

    /// Fill target line from bus and return selected way.
    fn fill_line(&mut self, paddr: Addr, bus: &mut Bus) -> Result<usize> {
        let set_idx = self.set_index(paddr);
        let tag = self.tag(paddr);
        let base = self.line_base(paddr);
        let way = self.choose_victim_way(set_idx);

        if self.sets[set_idx][way].valid {
            self.stats.evictions += 1;
            self.writeback_line(set_idx, way, bus)?;
        }

        let mut data = [0u8; CACHE_LINE_SIZE];
        for (i, value) in data.iter_mut().enumerate() {
            *value = bus.read_byte(Addr::new(base.raw() + i as u32))?.raw();
        }

        self.sets[set_idx][way] = CacheLine {
            valid: true,
            dirty: false,
            tag,
            data,
        };

        Ok(way)
    }

    #[inline]
    fn record_read_hit(&mut self) {
        self.stats.hits += 1;
        self.stats.read_hits += 1;
    }

    #[inline]
    fn record_read_miss(&mut self) {
        self.stats.misses += 1;
        self.stats.read_misses += 1;
    }

    #[inline]
    fn record_write_hit(&mut self) {
        self.stats.hits += 1;
        self.stats.write_hits += 1;
    }

    #[inline]
    fn record_write_miss(&mut self) {
        self.stats.misses += 1;
        self.stats.write_misses += 1;
    }

    fn write_bytes_to_bus(&self, paddr: Addr, bytes: &[u8], bus: &mut Bus) -> Result<()> {
        for (i, value) in bytes.iter().enumerate() {
            bus.write_byte(Addr::new(paddr.raw() + i as u32), Byte::new(*value))?;
        }
        Ok(())
    }

    fn write_bytes(&mut self, paddr: Addr, bytes: &[u8], bus: &mut Bus) -> Result<()> {
        let set_idx = self.set_index(paddr);
        let tag = self.tag(paddr);
        let offset = self.line_offset(paddr);

        // Conservatively fallback to direct bus writes if operation crosses line boundary.
        if offset + bytes.len() > CACHE_LINE_SIZE {
            self.record_write_miss();
            return self.write_bytes_to_bus(paddr, bytes, bus);
        }

        if let Some(hit_way) = self.find_way(set_idx, tag) {
            self.record_write_hit();

            {
                let line = &mut self.sets[set_idx][hit_way];
                for (i, value) in bytes.iter().enumerate() {
                    line.data[offset + i] = *value;
                }
                if self.cache_type == CacheType::Data
                    && self.config.write_policy == WritePolicy::WriteBack
                {
                    line.dirty = true;
                }
            }

            if self.config.write_policy == WritePolicy::WriteThrough {
                self.write_bytes_to_bus(paddr, bytes, bus)?;
            }

            return Ok(());
        }

        self.record_write_miss();

        if self.config.write_allocate == WriteAllocatePolicy::WriteAllocate {
            let way = self.fill_line(paddr, bus)?;
            {
                let line = &mut self.sets[set_idx][way];
                for (i, value) in bytes.iter().enumerate() {
                    line.data[offset + i] = *value;
                }
                if self.cache_type == CacheType::Data
                    && self.config.write_policy == WritePolicy::WriteBack
                {
                    line.dirty = true;
                }
            }

            if self.config.write_policy == WritePolicy::WriteThrough {
                self.write_bytes_to_bus(paddr, bytes, bus)?;
            }
            Ok(())
        } else {
            self.write_bytes_to_bus(paddr, bytes, bus)
        }
    }

    /// Read a word (4 bytes) from cache, filling from bus on miss.
    pub fn read_word(&mut self, paddr: Addr, bus: &mut Bus) -> Result<Word> {
        let set_idx = self.set_index(paddr);
        let tag = self.tag(paddr);
        let offset = self.line_offset(paddr);

        if offset + 4 > CACHE_LINE_SIZE {
            self.record_read_miss();
            let b0 = bus.read_byte(paddr)?.raw();
            let b1 = bus.read_byte(Addr::new(paddr.raw() + 1))?.raw();
            let b2 = bus.read_byte(Addr::new(paddr.raw() + 2))?.raw();
            let b3 = bus.read_byte(Addr::new(paddr.raw() + 3))?.raw();
            return Ok(Word::new(u32::from_le_bytes([b0, b1, b2, b3])));
        }

        if let Some(hit_way) = self.find_way(set_idx, tag) {
            self.record_read_hit();
            let data = &self.sets[set_idx][hit_way].data;
            return Ok(Word::new(u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ])));
        }

        self.record_read_miss();
        let way = self.fill_line(paddr, bus)?;
        let data = &self.sets[set_idx][way].data;
        Ok(Word::new(u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ])))
    }

    /// Read a byte from cache.
    pub fn read_byte(&mut self, paddr: Addr, bus: &mut Bus) -> Result<Byte> {
        let set_idx = self.set_index(paddr);
        let tag = self.tag(paddr);
        let offset = self.line_offset(paddr);

        if let Some(hit_way) = self.find_way(set_idx, tag) {
            self.record_read_hit();
            return Ok(Byte::new(self.sets[set_idx][hit_way].data[offset]));
        }

        self.record_read_miss();
        let way = self.fill_line(paddr, bus)?;
        Ok(Byte::new(self.sets[set_idx][way].data[offset]))
    }

    /// Read a half-word from cache.
    pub fn read_half(&mut self, paddr: Addr, bus: &mut Bus) -> Result<Half> {
        let set_idx = self.set_index(paddr);
        let tag = self.tag(paddr);
        let offset = self.line_offset(paddr);

        if offset + 2 > CACHE_LINE_SIZE {
            self.record_read_miss();
            let b0 = bus.read_byte(paddr)?.raw();
            let b1 = bus.read_byte(Addr::new(paddr.raw() + 1))?.raw();
            return Ok(Half::new(u16::from_le_bytes([b0, b1])));
        }

        if let Some(hit_way) = self.find_way(set_idx, tag) {
            self.record_read_hit();
            return Ok(Half::new(u16::from_le_bytes([
                self.sets[set_idx][hit_way].data[offset],
                self.sets[set_idx][hit_way].data[offset + 1],
            ])));
        }

        self.record_read_miss();
        let way = self.fill_line(paddr, bus)?;
        Ok(Half::new(u16::from_le_bytes([
            self.sets[set_idx][way].data[offset],
            self.sets[set_idx][way].data[offset + 1],
        ])))
    }

    /// Write a byte.
    pub fn write_byte(&mut self, paddr: Addr, value: Byte, bus: &mut Bus) -> Result<()> {
        self.write_bytes(paddr, &[value.raw()], bus)
    }

    /// Write a half-word.
    pub fn write_half(&mut self, paddr: Addr, value: Half, bus: &mut Bus) -> Result<()> {
        self.write_bytes(paddr, &value.raw().to_le_bytes(), bus)
    }

    /// Write a word.
    pub fn write_word(&mut self, paddr: Addr, value: Word, bus: &mut Bus) -> Result<()> {
        self.write_bytes(paddr, &value.raw().to_le_bytes(), bus)
    }

    /// Invalidate cache line containing `paddr` without write-back.
    pub fn invalidate(&mut self, paddr: Addr) {
        let set_idx = self.set_index(paddr);
        let tag = self.tag(paddr);
        if let Some(way) = self.find_way(set_idx, tag) {
            self.sets[set_idx][way] = CacheLine::default();
        }
    }

    /// Invalidate cache line containing `paddr`, writing back dirty data if required.
    pub fn invalidate_with_bus(&mut self, paddr: Addr, bus: &mut Bus) -> Result<()> {
        let set_idx = self.set_index(paddr);
        let tag = self.tag(paddr);
        if let Some(way) = self.find_way(set_idx, tag) {
            self.writeback_line(set_idx, way, bus)?;
            self.sets[set_idx][way] = CacheLine::default();
        }
        Ok(())
    }

    /// Flush all cache lines without write-back.
    pub fn flush_all(&mut self) {
        for set in &mut self.sets {
            for line in set {
                *line = CacheLine::default();
            }
        }
    }

    /// Flush all cache lines and write back dirty lines if required.
    pub fn flush_all_with_bus(&mut self, bus: &mut Bus) -> Result<()> {
        for set_idx in 0..self.set_count {
            for way in 0..self.ways {
                self.writeback_line(set_idx, way, bus)?;
                self.sets[set_idx][way] = CacheLine::default();
            }
        }
        Ok(())
    }

    /// Reset cache state and statistics.
    pub fn reset(&mut self) {
        self.flush_all();
        self.stats = CacheStats::default();
        self.next_victim.fill(0);
    }

    /// Get cache statistics.
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Get cache configuration.
    pub fn config(&self) -> CacheConfig {
        self.config
    }

    /// Get cache associativity.
    pub fn associativity(&self) -> usize {
        self.ways
    }

    /// Get number of sets.
    pub fn set_count(&self) -> usize {
        self.set_count
    }

    /// Get the cache type.
    pub fn cache_type(&self) -> CacheType {
        self.cache_type
    }

    /// Get the number of valid lines (for visualization).
    pub fn valid_line_count(&self) -> usize {
        self.sets
            .iter()
            .map(|set| set.iter().filter(|line| line.valid).count())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Ram;

    fn setup_bus() -> Bus {
        let mut bus = Bus::new();
        let ram = Ram::new(0x20000);
        bus.attach_memory(Addr::new(0), ram, "RAM");
        bus
    }

    #[test]
    fn test_cache_initial_miss_then_hit() {
        let mut bus = setup_bus();
        let mut cache = Cache::new(CacheType::Instruction, 4096);

        bus.write_word(Addr::new(0x100), Word::new(0xDEADBEEF))
            .unwrap();

        let word = cache.read_word(Addr::new(0x100), &mut bus).unwrap();
        assert_eq!(word.raw(), 0xDEADBEEF);
        assert_eq!(cache.stats().read_misses, 1);
        assert_eq!(cache.stats().read_hits, 0);

        let word = cache.read_word(Addr::new(0x100), &mut bus).unwrap();
        assert_eq!(word.raw(), 0xDEADBEEF);
        assert_eq!(cache.stats().read_hits, 1);
    }

    #[test]
    fn test_two_way_set_associative_reduces_conflict_miss() {
        let mut bus = setup_bus();
        let cfg = CacheConfig {
            associativity: 2,
            replacement: ReplacementPolicy::RoundRobin,
            write_policy: WritePolicy::WriteBack,
            write_allocate: WriteAllocatePolicy::WriteAllocate,
        };
        let mut cache = Cache::new_with_config(CacheType::Data, 64, cfg);

        // 64B / 16B = 4 lines, 2-way => 2 sets
        // Same set addresses: 0x000, 0x020
        bus.write_word(Addr::new(0x000), Word::new(0x1111)).unwrap();
        bus.write_word(Addr::new(0x020), Word::new(0x2222)).unwrap();

        cache.read_word(Addr::new(0x000), &mut bus).unwrap(); // miss
        cache.read_word(Addr::new(0x020), &mut bus).unwrap(); // miss, same set other way

        // Should hit because 2-way can hold both lines in the set.
        let word = cache.read_word(Addr::new(0x000), &mut bus).unwrap();
        assert_eq!(word.raw(), 0x1111);
        assert_eq!(cache.stats().evictions, 0);
        assert_eq!(cache.stats().read_hits, 1);
    }

    #[test]
    fn test_direct_mapped_conflict_eviction_still_possible() {
        let mut bus = setup_bus();
        let cfg = CacheConfig {
            associativity: 1,
            replacement: ReplacementPolicy::RoundRobin,
            write_policy: WritePolicy::WriteThrough,
            write_allocate: WriteAllocatePolicy::NoWriteAllocate,
        };
        let mut cache = Cache::new_with_config(CacheType::Instruction, 4096, cfg);

        bus.write_word(Addr::new(0x000), Word::new(0x1111)).unwrap();
        bus.write_word(Addr::new(0x1000), Word::new(0x2222))
            .unwrap();

        cache.read_word(Addr::new(0x000), &mut bus).unwrap();
        cache.read_word(Addr::new(0x1000), &mut bus).unwrap();

        assert_eq!(cache.stats().evictions, 1);
    }

    #[test]
    fn test_write_back_delays_bus_update_until_flush() {
        let mut bus = setup_bus();
        let mut cache = Cache::new(CacheType::Data, 4096);

        bus.write_word(Addr::new(0x200), Word::new(0x0)).unwrap();
        cache.read_word(Addr::new(0x200), &mut bus).unwrap();

        cache
            .write_word(Addr::new(0x200), Word::new(0x12345678), &mut bus)
            .unwrap();

        // Write-back: memory is still old value before flush/eviction.
        assert_eq!(bus.read_word(Addr::new(0x200)).unwrap().raw(), 0x0);

        cache.flush_all_with_bus(&mut bus).unwrap();
        assert_eq!(bus.read_word(Addr::new(0x200)).unwrap().raw(), 0x12345678);
        assert!(cache.stats().writebacks >= 1);
    }

    #[test]
    fn test_write_allocate_on_store_miss() {
        let mut bus = setup_bus();
        let mut cache = Cache::new(CacheType::Data, 4096);

        bus.write_word(Addr::new(0x240), Word::new(0xAAAA5555))
            .unwrap();

        // Store miss -> allocate line then update line (dirty).
        cache
            .write_word(Addr::new(0x240), Word::new(0xDEADBEEF), &mut bus)
            .unwrap();

        assert_eq!(cache.stats().write_misses, 1);
        assert!(cache.valid_line_count() > 0);

        // Not yet visible in memory until flush/eviction.
        assert_eq!(bus.read_word(Addr::new(0x240)).unwrap().raw(), 0xAAAA5555);

        let cached = cache.read_word(Addr::new(0x240), &mut bus).unwrap();
        assert_eq!(cached.raw(), 0xDEADBEEF);

        cache.flush_all_with_bus(&mut bus).unwrap();
        assert_eq!(bus.read_word(Addr::new(0x240)).unwrap().raw(), 0xDEADBEEF);
    }

    #[test]
    fn test_dirty_eviction_triggers_writeback() {
        let mut bus = setup_bus();
        let cfg = CacheConfig {
            associativity: 1,
            replacement: ReplacementPolicy::RoundRobin,
            write_policy: WritePolicy::WriteBack,
            write_allocate: WriteAllocatePolicy::WriteAllocate,
        };
        let mut cache = Cache::new_with_config(CacheType::Data, 64, cfg);

        // Same set for direct-mapped 64B cache (4 sets): 0x000 and 0x040
        bus.write_word(Addr::new(0x000), Word::new(0x0)).unwrap();
        bus.write_word(Addr::new(0x040), Word::new(0x0)).unwrap();

        cache.read_word(Addr::new(0x000), &mut bus).unwrap();
        cache
            .write_word(Addr::new(0x000), Word::new(0xCAFEBABE), &mut bus)
            .unwrap();

        // Access aliasing line to evict dirty 0x000 line.
        cache.read_word(Addr::new(0x040), &mut bus).unwrap();

        assert_eq!(cache.stats().evictions, 1);
        assert_eq!(cache.stats().writebacks, 1);
        assert_eq!(bus.read_word(Addr::new(0x000)).unwrap().raw(), 0xCAFEBABE);
    }

    #[test]
    fn test_write_through_policy_updates_bus_immediately() {
        let mut bus = setup_bus();
        let cfg = CacheConfig {
            associativity: 2,
            replacement: ReplacementPolicy::RoundRobin,
            write_policy: WritePolicy::WriteThrough,
            write_allocate: WriteAllocatePolicy::WriteAllocate,
        };
        let mut cache = Cache::new_with_config(CacheType::Data, 4096, cfg);

        cache
            .write_word(Addr::new(0x300), Word::new(0x55AA33CC), &mut bus)
            .unwrap();

        assert_eq!(bus.read_word(Addr::new(0x300)).unwrap().raw(), 0x55AA33CC);
    }

    #[test]
    fn test_no_write_allocate_bypasses_cache_on_store_miss() {
        let mut bus = setup_bus();
        let cfg = CacheConfig {
            associativity: 2,
            replacement: ReplacementPolicy::RoundRobin,
            write_policy: WritePolicy::WriteBack,
            write_allocate: WriteAllocatePolicy::NoWriteAllocate,
        };
        let mut cache = Cache::new_with_config(CacheType::Data, 4096, cfg);

        cache
            .write_word(Addr::new(0x340), Word::new(0xABCD1234), &mut bus)
            .unwrap();

        assert_eq!(cache.valid_line_count(), 0);
        assert_eq!(bus.read_word(Addr::new(0x340)).unwrap().raw(), 0xABCD1234);
    }

    #[test]
    fn test_cache_invalidate_with_bus_writes_back_dirty_line() {
        let mut bus = setup_bus();
        let mut cache = Cache::new(CacheType::Data, 4096);

        bus.write_word(Addr::new(0x500), Word::new(0x0)).unwrap();
        cache.read_word(Addr::new(0x500), &mut bus).unwrap();
        cache
            .write_word(Addr::new(0x500), Word::new(0x87654321), &mut bus)
            .unwrap();

        cache
            .invalidate_with_bus(Addr::new(0x500), &mut bus)
            .unwrap();
        assert_eq!(bus.read_word(Addr::new(0x500)).unwrap().raw(), 0x87654321);
    }

    #[test]
    fn test_cache_read_byte_half() {
        let mut bus = setup_bus();
        let mut cache = Cache::new(CacheType::Data, 4096);

        bus.write_word(Addr::new(0x100), Word::new(0x12345678))
            .unwrap();

        let byte = cache.read_byte(Addr::new(0x100), &mut bus).unwrap();
        assert_eq!(byte.raw(), 0x78);

        let half = cache.read_half(Addr::new(0x100), &mut bus).unwrap();
        assert_eq!(half.raw(), 0x5678);
        assert_eq!(cache.stats().read_hits, 1);
    }

    #[test]
    fn test_cache_stats_hit_rate() {
        let mut bus = setup_bus();
        let mut cache = Cache::new(CacheType::Instruction, 4096);

        assert!(cache.stats().hit_rate().is_none());

        bus.write_word(Addr::new(0x100), Word::new(0xAA)).unwrap();
        cache.read_word(Addr::new(0x100), &mut bus).unwrap(); // miss
        cache.read_word(Addr::new(0x100), &mut bus).unwrap(); // hit
        cache.read_word(Addr::new(0x100), &mut bus).unwrap(); // hit

        let rate = cache.stats().hit_rate().unwrap();
        assert!((rate - 66.667).abs() < 0.1);
    }
}
