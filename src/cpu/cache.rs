//! Cache simulation for the RISC-V pipeline.
//!
//! Implements direct-mapped I-Cache and D-Cache with configurable sizes.
//! D-Cache uses write-through policy (writes go to both cache and bus).

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

/// Cache statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
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
}

/// A single cache line.
#[derive(Debug, Clone)]
struct CacheLine {
    valid: bool,
    dirty: bool, // D-Cache only
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

/// Direct-mapped cache with write-through for data.
#[derive(Debug, Clone)]
pub struct Cache {
    cache_type: CacheType,
    num_lines: usize,
    line_mask: usize,
    index_bits: usize,
    lines: Vec<CacheLine>,
    stats: CacheStats,
}

impl Cache {
    /// Create a new cache of the given type and size.
    /// Size must be a power of 2 and >= CACHE_LINE_SIZE.
    pub fn new(cache_type: CacheType, size: usize) -> Self {
        let num_lines = size / CACHE_LINE_SIZE;
        let index_bits = num_lines.trailing_zeros() as usize;
        let line_mask = CACHE_LINE_SIZE - 1;
        Self {
            cache_type,
            num_lines,
            line_mask,
            index_bits,
            lines: vec![CacheLine::default(); num_lines],
            stats: CacheStats::default(),
        }
    }

    /// Compute cache line index from physical address.
    #[inline]
    fn index(&self, paddr: Addr) -> usize {
        ((paddr.raw() as usize) >> 4) & (self.num_lines - 1)
    }

    /// Compute tag from physical address.
    #[inline]
    fn tag(&self, paddr: Addr) -> u32 {
        paddr.raw() >> (4 + self.index_bits)
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

    /// Fill a cache line from bus.
    fn fill_line(&mut self, paddr: Addr, bus: &mut Bus) -> Result<()> {
        let idx = self.index(paddr);
        let tag = self.tag(paddr);
        let base = self.line_base(paddr);

        // Count replacement when a valid line is overwritten.
        if self.lines[idx].valid {
            self.stats.evictions += 1;
        }

        // Read entire line from bus
        let mut data = [0u8; CACHE_LINE_SIZE];
        for i in 0..CACHE_LINE_SIZE {
            data[i] = bus.read_byte(Addr::new(base.raw() + i as u32))?.raw();
        }

        self.lines[idx] = CacheLine {
            valid: true,
            dirty: false,
            tag,
            data,
        };

        Ok(())
    }

    /// Read a word (4 bytes) from cache, filling from bus on miss.
    pub fn read_word(&mut self, paddr: Addr, bus: &mut Bus) -> Result<Word> {
        let idx = self.index(paddr);
        let tag = self.tag(paddr);
        let offset = self.line_offset(paddr);

        if self.lines[idx].valid && self.lines[idx].tag == tag {
            // Hit
            self.stats.hits += 1;
            let data = &self.lines[idx].data;
            let word = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            return Ok(Word::new(word));
        }

        // Miss — fill line then read
        self.stats.misses += 1;
        self.fill_line(paddr, bus)?;
        let data = &self.lines[idx].data;
        let word = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        Ok(Word::new(word))
    }

    /// Read a byte from cache.
    pub fn read_byte(&mut self, paddr: Addr, bus: &mut Bus) -> Result<Byte> {
        let idx = self.index(paddr);
        let tag = self.tag(paddr);

        if self.lines[idx].valid && self.lines[idx].tag == tag {
            self.stats.hits += 1;
            return Ok(Byte::new(self.lines[idx].data[self.line_offset(paddr)]));
        }

        self.stats.misses += 1;
        self.fill_line(paddr, bus)?;
        Ok(Byte::new(self.lines[idx].data[self.line_offset(paddr)]))
    }

    /// Read a half-word from cache.
    pub fn read_half(&mut self, paddr: Addr, bus: &mut Bus) -> Result<Half> {
        let idx = self.index(paddr);
        let tag = self.tag(paddr);
        let offset = self.line_offset(paddr);

        if self.lines[idx].valid && self.lines[idx].tag == tag {
            self.stats.hits += 1;
            let half = u16::from_le_bytes([
                self.lines[idx].data[offset],
                self.lines[idx].data[offset + 1],
            ]);
            return Ok(Half::new(half));
        }

        self.stats.misses += 1;
        self.fill_line(paddr, bus)?;
        let half = u16::from_le_bytes([
            self.lines[idx].data[offset],
            self.lines[idx].data[offset + 1],
        ]);
        Ok(Half::new(half))
    }

    /// Write a byte (write-through: updates both cache and bus).
    pub fn write_byte(&mut self, paddr: Addr, value: Byte, bus: &mut Bus) -> Result<()> {
        // Always write through to bus first
        bus.write_byte(paddr, value)?;

        // Also update cache if the line is present
        let idx = self.index(paddr);
        let tag = self.tag(paddr);
        let offset = self.line_offset(paddr);
        if self.lines[idx].valid && self.lines[idx].tag == tag {
            self.lines[idx].data[offset] = value.raw();
            self.lines[idx].dirty = true;
        }
        Ok(())
    }

    /// Write a half-word (write-through).
    pub fn write_half(&mut self, paddr: Addr, value: Half, bus: &mut Bus) -> Result<()> {
        bus.write_half(paddr, value)?;

        let idx = self.index(paddr);
        let tag = self.tag(paddr);
        let offset = self.line_offset(paddr);
        if self.lines[idx].valid && self.lines[idx].tag == tag {
            let bytes = value.raw().to_le_bytes();
            self.lines[idx].data[offset] = bytes[0];
            self.lines[idx].data[offset + 1] = bytes[1];
            self.lines[idx].dirty = true;
        }
        Ok(())
    }

    /// Write a word (write-through).
    pub fn write_word(&mut self, paddr: Addr, value: Word, bus: &mut Bus) -> Result<()> {
        bus.write_word(paddr, value)?;

        let idx = self.index(paddr);
        let tag = self.tag(paddr);
        let offset = self.line_offset(paddr);
        if self.lines[idx].valid && self.lines[idx].tag == tag {
            let bytes = value.raw().to_le_bytes();
            self.lines[idx].data[offset] = bytes[0];
            self.lines[idx].data[offset + 1] = bytes[1];
            self.lines[idx].data[offset + 2] = bytes[2];
            self.lines[idx].data[offset + 3] = bytes[3];
            self.lines[idx].dirty = true;
        }
        Ok(())
    }

    /// Invalidate the cache line containing `paddr`.
    pub fn invalidate(&mut self, paddr: Addr) {
        let idx = self.index(paddr);
        let tag = self.tag(paddr);
        if self.lines[idx].valid && self.lines[idx].tag == tag {
            self.lines[idx].valid = false;
        }
    }

    /// Flush all cache lines.
    pub fn flush_all(&mut self) {
        for line in &mut self.lines {
            *line = CacheLine::default();
        }
    }

    /// Reset cache state and statistics.
    pub fn reset(&mut self) {
        self.flush_all();
        self.stats = CacheStats::default();
    }

    /// Get cache statistics.
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Get the cache type.
    pub fn cache_type(&self) -> CacheType {
        self.cache_type
    }

    /// Get the number of valid lines (for visualization).
    pub fn valid_line_count(&self) -> usize {
        self.lines.iter().filter(|l| l.valid).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Ram;

    fn setup_bus() -> Bus {
        let mut bus = Bus::new();
        let ram = Ram::new(0x10000);
        bus.attach_memory(Addr::new(0), ram, "RAM");
        bus
    }

    #[test]
    fn test_cache_initial_miss_then_hit() {
        let mut bus = setup_bus();
        let mut cache = Cache::new(CacheType::Instruction, 4096);

        // Write test data to bus
        bus.write_word(Addr::new(0x100), Word::new(0xDEADBEEF))
            .unwrap();

        // First read → miss
        let word = cache.read_word(Addr::new(0x100), &mut bus).unwrap();
        assert_eq!(word.raw(), 0xDEADBEEF);
        assert_eq!(cache.stats().misses, 1);
        assert_eq!(cache.stats().hits, 0);

        // Second read → hit
        let word = cache.read_word(Addr::new(0x100), &mut bus).unwrap();
        assert_eq!(word.raw(), 0xDEADBEEF);
        assert_eq!(cache.stats().hits, 1);
    }

    #[test]
    fn test_cache_line_fill_reads_neighbors() {
        let mut bus = setup_bus();
        let mut cache = Cache::new(CacheType::Data, 4096);

        // Write two words within the same cache line (16 bytes)
        bus.write_word(Addr::new(0x100), Word::new(0xAAAA)).unwrap();
        bus.write_word(Addr::new(0x104), Word::new(0xBBBB)).unwrap();

        // Read first word → miss, fills entire line
        cache.read_word(Addr::new(0x100), &mut bus).unwrap();
        assert_eq!(cache.stats().misses, 1);

        // Read second word → hit (same line)
        let word = cache.read_word(Addr::new(0x104), &mut bus).unwrap();
        assert_eq!(word.raw(), 0xBBBB);
        assert_eq!(cache.stats().hits, 1);
    }

    #[test]
    fn test_cache_write_through() {
        let mut bus = setup_bus();
        let mut cache = Cache::new(CacheType::Data, 4096);

        // Write through cache
        cache
            .write_word(Addr::new(0x200), Word::new(0x1234), &mut bus)
            .unwrap();

        // Verify data is on the bus
        assert_eq!(bus.read_word(Addr::new(0x200)).unwrap().raw(), 0x1234);

        // Read from another cache instance (cold) should see the write
        let mut cache2 = Cache::new(CacheType::Data, 4096);
        let word = cache2.read_word(Addr::new(0x200), &mut bus).unwrap();
        assert_eq!(word.raw(), 0x1234);
    }

    #[test]
    fn test_cache_aliasing_eviction() {
        let mut bus = setup_bus();
        let mut cache = Cache::new(CacheType::Instruction, 4096);
        // 4096 / 16 = 256 lines
        // index = (addr >> 4) & 0xFF
        // Two addresses mapping to same index: 0x000 and 0x000 + 256*16 = 0x1000
        bus.write_word(Addr::new(0x000), Word::new(0x1111)).unwrap();
        bus.write_word(Addr::new(0x1000), Word::new(0x2222))
            .unwrap();

        cache.read_word(Addr::new(0x000), &mut bus).unwrap();
        assert_eq!(cache.stats().misses, 1);

        // Access same line → hit
        cache.read_word(Addr::new(0x000), &mut bus).unwrap();
        assert_eq!(cache.stats().hits, 1);

        // Access aliasing address → evicts first
        let word = cache.read_word(Addr::new(0x1000), &mut bus).unwrap();
        assert_eq!(word.raw(), 0x2222);
        assert_eq!(cache.stats().misses, 2);
        assert_eq!(cache.stats().evictions, 1);
    }

    #[test]
    fn test_cache_invalidate() {
        let mut bus = setup_bus();
        let mut cache = Cache::new(CacheType::Data, 4096);

        bus.write_word(Addr::new(0x100), Word::new(0x42)).unwrap();
        cache.read_word(Addr::new(0x100), &mut bus).unwrap();
        assert_eq!(cache.stats().hits, 0);

        // Hit on second read
        cache.read_word(Addr::new(0x100), &mut bus).unwrap();
        assert_eq!(cache.stats().hits, 1);

        // Invalidate
        cache.invalidate(Addr::new(0x100));

        // Next read → miss (line invalidated)
        cache.read_word(Addr::new(0x100), &mut bus).unwrap();
        assert_eq!(cache.stats().misses, 2);
    }

    #[test]
    fn test_cache_flush_all() {
        let mut bus = setup_bus();
        let mut cache = Cache::new(CacheType::Instruction, 4096);

        bus.write_word(Addr::new(0x100), Word::new(0xAA)).unwrap();
        cache.read_word(Addr::new(0x100), &mut bus).unwrap();
        assert!(cache.valid_line_count() > 0);

        cache.flush_all();
        assert_eq!(cache.valid_line_count(), 0);
    }

    #[test]
    fn test_cache_reset_clears_stats() {
        let mut bus = setup_bus();
        let mut cache = Cache::new(CacheType::Instruction, 4096);

        bus.write_word(Addr::new(0x100), Word::new(0xAA)).unwrap();
        cache.read_word(Addr::new(0x100), &mut bus).unwrap();

        cache.reset();
        assert_eq!(cache.stats().hits, 0);
        assert_eq!(cache.stats().misses, 0);
        assert_eq!(cache.valid_line_count(), 0);
    }

    #[test]
    fn test_cache_read_byte_half() {
        let mut bus = setup_bus();
        let mut cache = Cache::new(CacheType::Data, 4096);

        bus.write_word(Addr::new(0x100), Word::new(0x12345678))
            .unwrap();

        let byte = cache.read_byte(Addr::new(0x100), &mut bus).unwrap();
        assert_eq!(byte.raw(), 0x78); // Little-endian LSB

        // Same line → hit
        let half = cache.read_half(Addr::new(0x100), &mut bus).unwrap();
        assert_eq!(half.raw(), 0x5678);
        assert_eq!(cache.stats().hits, 1);
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
