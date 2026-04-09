//! Performance Monitor CSRs (RISC-V HPM extension).
//!
//! This module implements the Hardware Performance Monitor counters
//! as defined in the RISC-V Privileged Architecture specification.
//!
//! # CSR Address Map
//!
//! | Address  | Name        | Description                    |
//! |----------|-------------|--------------------------------|
//! | 0xB00    | mcycle      | Cycle counter (low 32 bits)    |
//! | 0xB80    | mcycleh     | Cycle counter (high 32 bits)   |
//! | 0xB02    | minstret    | Instruction counter (low)      |
//! | 0xB82    | minstreth   | Instruction counter (high)     |
//! | 0xB03-1F | mhpmcounter3-31 | HPM counters (low)        |
//! | 0xB83-9F | mhpmcounter3h-31h| HPM counters (high)      |
//! | 0x323-3F | mhpmevent3-31    | HPM event selectors       |
//! | 0x320    | mcountinhibit    | Counter inhibit register  |

use super::csr_trait::CsrRegister;
use crate::types::PrivilegeLevel;

// ============================================================================
// Constants
// ============================================================================

/// Number of HPM counters (counters 3-31, total 29)
pub const HPM_COUNTER_COUNT: usize = 29;

/// Base index for HPM counters (counter 3 is the first programmable counter)
pub const HPM_COUNTER_BASE: usize = 3;

/// Bit masks for mcountinhibit register
pub mod inhibit_bits {
    /// Cycle counter inhibit (bit 0)
    pub const CY: u32 = 1 << 0;
    /// Instruction counter inhibit (bit 1)
    pub const IR: u32 = 1 << 1;
    /// All HPM counters inhibit (bit 2)
    pub const HPMS: u32 = 1 << 2;
}

// ============================================================================
// Counter64 - Generic 64-bit counter split into high/low for RV32
// ============================================================================

/// A 64-bit counter split into two 32-bit registers for RV32.
///
/// This is the core abstraction used by mcycle, minstret, and mhpmcounter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Counter64 {
    low: u32,
    high: u32,
}

impl Counter64 {
    /// Create a new counter initialized to zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the 64-bit counter value.
    #[inline]
    pub fn get(&self) -> u64 {
        ((self.high as u64) << 32) | (self.low as u64)
    }

    /// Set the 64-bit counter value.
    #[inline]
    pub fn set(&mut self, value: u64) {
        self.low = value as u32;
        self.high = (value >> 32) as u32;
    }

    /// Increment the counter by a given amount with wrapping.
    #[inline]
    pub fn increment(&mut self, amount: u64) {
        let current = self.get();
        self.set(current.wrapping_add(amount));
    }

    /// Increment by 1 (optimized for hot path).
    #[inline]
    pub fn increment_one(&mut self) {
        let (new_low, carry) = self.low.overflowing_add(1);
        self.low = new_low;
        if carry {
            self.high = self.high.wrapping_add(1);
        }
    }

    /// Read the low 32 bits.
    #[inline]
    pub fn read_low(&self) -> u32 {
        self.low
    }

    /// Read the high 32 bits.
    #[inline]
    pub fn read_high(&self) -> u32 {
        self.high
    }

    /// Write the low 32 bits.
    #[inline]
    pub fn write_low(&mut self, value: u32) {
        self.low = value;
    }

    /// Write the high 32 bits.
    #[inline]
    pub fn write_high(&mut self, value: u32) {
        self.high = value;
    }
}

/// CSR address constants for Performance Monitor.
pub mod csr_addr {
    // Cycle counter
    pub const MCYCLE: u16 = 0xB00;
    pub const MCYCLEH: u16 = 0xB80;

    // Instruction counter
    pub const MINSTRET: u16 = 0xB02;
    pub const MINSTRETH: u16 = 0xB82;

    // Hardware Performance Monitor counters (3-31)
    pub const MHPMCOUNTER_BASE: u16 = 0xB03;
    pub const MHPMCOUNTER_END: u16 = 0xB1F;
    pub const MHPMCOUNTERH_BASE: u16 = 0xB83;
    pub const MHPMCOUNTERH_END: u16 = 0xB9F;

    // Hardware Performance Event selectors
    pub const MHPMEVENT_BASE: u16 = 0x323;
    pub const MHPMEVENT_END: u16 = 0x33F;

    // Counter inhibit register
    pub const MCOUNTINHIBIT: u16 = 0x320;

    /// Convert counter index (3-31) to mhpmcounter address.
    pub fn mhpmcounter_addr(index: usize) -> Option<u16> {
        if (3..=31).contains(&index) {
            Some(MHPMCOUNTER_BASE + (index - 3) as u16)
        } else {
            None
        }
    }

    /// Convert counter index (3-31) to mhpmcounterh address.
    pub fn mhpmcounterh_addr(index: usize) -> Option<u16> {
        if (3..=31).contains(&index) {
            Some(MHPMCOUNTERH_BASE + (index - 3) as u16)
        } else {
            None
        }
    }

    /// Convert counter index (3-31) to mhpmevent address.
    pub fn mhpmevent_addr(index: usize) -> Option<u16> {
        if (3..=31).contains(&index) {
            Some(MHPMEVENT_BASE + (index - 3) as u16)
        } else {
            None
        }
    }
}

/// Performance events that can be monitored by HPM counters.
///
/// These event codes correspond to the values written to mhpmevent registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum PerfEvent {
    /// No event selected (counter disabled)
    #[default]
    None = 0,
    /// CPU cycles
    Cycles = 1,
    /// Instructions retired (completed)
    InstructionsRetired = 2,
    /// Load-use stalls
    LoadUseStalls = 3,
    /// Control hazards (branch mispredictions)
    ControlHazards = 4,
    /// Branch instructions executed
    BranchExecuted = 5,
    /// Branch taken
    BranchTaken = 6,
    /// Branch not taken
    BranchNotTaken = 7,
    /// Memory reads
    MemoryReads = 8,
    /// Memory writes
    MemoryWrites = 9,
    /// ALU operations
    AluOperations = 10,
    /// CSR accesses
    CsrAccesses = 11,
    /// Pipeline flushes
    PipelineFlushes = 12,
    /// Interrupts taken
    InterruptsTaken = 13,
    /// Exceptions taken
    ExceptionsTaken = 14,
    /// Branch mispredictions (dynamic predictor)
    BranchMispredictions = 15,
}

impl PerfEvent {
    /// Try to convert a raw event code to PerfEvent.
    pub fn from_code(code: u8) -> Self {
        match code {
            0 => PerfEvent::None,
            1 => PerfEvent::Cycles,
            2 => PerfEvent::InstructionsRetired,
            3 => PerfEvent::LoadUseStalls,
            4 => PerfEvent::ControlHazards,
            5 => PerfEvent::BranchExecuted,
            6 => PerfEvent::BranchTaken,
            7 => PerfEvent::BranchNotTaken,
            8 => PerfEvent::MemoryReads,
            9 => PerfEvent::MemoryWrites,
            10 => PerfEvent::AluOperations,
            11 => PerfEvent::CsrAccesses,
            12 => PerfEvent::PipelineFlushes,
            13 => PerfEvent::InterruptsTaken,
            14 => PerfEvent::ExceptionsTaken,
            15 => PerfEvent::BranchMispredictions,
            _ => PerfEvent::None,
        }
    }

    /// Get the event code.
    pub fn code(&self) -> u8 {
        *self as u8
    }
}

/// Machine Cycle Counter (mcycle/mcycleh).
///
/// A 64-bit counter that increments every clock cycle.
/// Split into two 32-bit registers for RV32.
#[derive(Debug, Clone, Copy, Default)]
pub struct Mcycle {
    counter: Counter64,
}

impl Mcycle {
    /// Create a new cycle counter initialized to zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Increment the counter by a given amount.
    #[inline]
    pub fn increment(&mut self, amount: u64) {
        self.counter.increment(amount);
    }

    /// Increment by 1 (optimized for hot path).
    #[inline]
    pub fn tick(&mut self) {
        self.counter.increment_one();
    }

    /// Get the 64-bit counter value.
    #[inline]
    pub fn get(&self) -> u64 {
        self.counter.get()
    }

    /// Set the 64-bit counter value.
    #[inline]
    pub fn set(&mut self, value: u64) {
        self.counter.set(value);
    }

    /// Read the low 32 bits.
    #[inline]
    pub fn read_low(&self) -> u32 {
        self.counter.read_low()
    }

    /// Read the high 32 bits.
    #[inline]
    pub fn read_high(&self) -> u32 {
        self.counter.read_high()
    }

    /// Write the low 32 bits.
    #[inline]
    pub fn write_low(&mut self, value: u32) {
        self.counter.write_low(value);
    }

    /// Write the high 32 bits.
    #[inline]
    pub fn write_high(&mut self, value: u32) {
        self.counter.write_high(value);
    }
}

impl CsrRegister for Mcycle {
    fn address(&self) -> u16 {
        csr_addr::MCYCLE
    }

    fn name(&self) -> &'static str {
        "mcycle"
    }

    fn read(&self) -> u32 {
        self.counter.read_low()
    }

    fn write(&mut self, value: u32) {
        self.counter.write_low(value);
    }

    fn min_privilege(&self) -> PrivilegeLevel {
        PrivilegeLevel::Machine
    }
}

/// Machine Cycle Counter High (mcycleh).
///
/// High 32 bits of the 64-bit cycle counter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Mcycleh {
    value: u32,
}

impl Mcycleh {
    /// Create a new high cycle counter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Read the value.
    pub fn read(&self) -> u32 {
        self.value
    }

    /// Write the value.
    pub fn write(&mut self, value: u32) {
        self.value = value;
    }
}

impl CsrRegister for Mcycleh {
    fn address(&self) -> u16 {
        csr_addr::MCYCLEH
    }

    fn name(&self) -> &'static str {
        "mcycleh"
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

/// Machine Instructions-Retired Counter (minstret/minstreth).
///
/// A 64-bit counter that increments for every instruction that completes.
/// Split into two 32-bit registers for RV32.
#[derive(Debug, Clone, Copy, Default)]
pub struct Minstret {
    counter: Counter64,
}

impl Minstret {
    /// Create a new instruction counter initialized to zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Increment the counter by a given amount.
    #[inline]
    pub fn increment(&mut self, amount: u64) {
        self.counter.increment(amount);
    }

    /// Increment by 1 (optimized for hot path).
    #[inline]
    pub fn increment_one(&mut self) {
        self.counter.increment_one();
    }

    /// Get the 64-bit counter value.
    #[inline]
    pub fn get(&self) -> u64 {
        self.counter.get()
    }

    /// Set the 64-bit counter value.
    #[inline]
    pub fn set(&mut self, value: u64) {
        self.counter.set(value);
    }

    /// Read the low 32 bits.
    #[inline]
    pub fn read_low(&self) -> u32 {
        self.counter.read_low()
    }

    /// Read the high 32 bits.
    #[inline]
    pub fn read_high(&self) -> u32 {
        self.counter.read_high()
    }

    /// Write the low 32 bits.
    #[inline]
    pub fn write_low(&mut self, value: u32) {
        self.counter.write_low(value);
    }

    /// Write the high 32 bits.
    #[inline]
    pub fn write_high(&mut self, value: u32) {
        self.counter.write_high(value);
    }
}

impl CsrRegister for Minstret {
    fn address(&self) -> u16 {
        csr_addr::MINSTRET
    }

    fn name(&self) -> &'static str {
        "minstret"
    }

    fn read(&self) -> u32 {
        self.counter.read_low()
    }

    fn write(&mut self, value: u32) {
        self.counter.write_low(value);
    }

    fn min_privilege(&self) -> PrivilegeLevel {
        PrivilegeLevel::Machine
    }
}

/// Machine Instructions-Retired Counter High (minstreth).
///
/// High 32 bits of the 64-bit instruction counter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Minstreth {
    value: u32,
}

impl Minstreth {
    /// Create a new high instruction counter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Read the value.
    pub fn read(&self) -> u32 {
        self.value
    }

    /// Write the value.
    pub fn write(&mut self, value: u32) {
        self.value = value;
    }
}

impl CsrRegister for Minstreth {
    fn address(&self) -> u16 {
        csr_addr::MINSTRETH
    }

    fn name(&self) -> &'static str {
        "minstreth"
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

/// Machine Counter Inhibit Register (mcountinhibit).
///
/// Controls which performance counters are enabled.
/// Bit 0: cy (cycle counter inhibit)
/// Bit 1: ir (instruction counter inhibit)
/// Bit 2: hpms (all HPM counters inhibit)
/// Bits 3-31: hpm3-31 (individual HPM counter inhibit)
#[derive(Debug, Clone, Copy, Default)]
pub struct Mcountinhibit {
    value: u32,
}

impl Mcountinhibit {
    /// Create a new counter inhibit register.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if cycle counter is inhibited.
    #[inline]
    pub fn cy(&self) -> bool {
        self.value & inhibit_bits::CY != 0
    }

    /// Check if instruction counter is inhibited.
    #[inline]
    pub fn ir(&self) -> bool {
        self.value & inhibit_bits::IR != 0
    }

    /// Check if all HPM counters are inhibited.
    #[inline]
    pub fn hpms(&self) -> bool {
        self.value & inhibit_bits::HPMS != 0
    }

    /// Check if a specific HPM counter is inhibited.
    pub fn hpm(&self, index: usize) -> bool {
        if index < 3 || index > 31 {
            return false;
        }
        self.value & (1 << index) != 0
    }

    /// Read the raw value.
    pub fn read(&self) -> u32 {
        self.value
    }

    /// Write the raw value.
    pub fn write(&mut self, value: u32) {
        self.value = value;
    }
}

impl CsrRegister for Mcountinhibit {
    fn address(&self) -> u16 {
        csr_addr::MCOUNTINHIBIT
    }

    fn name(&self) -> &'static str {
        "mcountinhibit"
    }

    fn read(&self) -> u32 {
        self.value
    }

    fn write(&mut self, value: u32) {
        // Only bits 0-31 are writable, but we only have 32 bits anyway
        self.value = value;
    }

    fn min_privilege(&self) -> PrivilegeLevel {
        PrivilegeLevel::Machine
    }
}

/// Hardware Performance Monitor Counter (mhpmcounter3-31).
///
/// Each counter is 64-bit, split into low/high registers.
/// The counter increments when the configured event occurs.
#[derive(Debug, Clone, Copy, Default)]
pub struct Mhpmcounter {
    counter: Counter64,
    index: usize, // 3-31
}

impl Mhpmcounter {
    /// Create a new HPM counter for the given index (3-31).
    pub fn new(index: usize) -> Self {
        debug_assert!((HPM_COUNTER_BASE..=31).contains(&index));
        Self {
            counter: Counter64::new(),
            index: index.clamp(HPM_COUNTER_BASE, 31),
        }
    }

    /// Get the counter index (3-31).
    #[inline]
    pub fn index(&self) -> usize {
        self.index
    }

    /// Get the 64-bit counter value.
    #[inline]
    pub fn get(&self) -> u64 {
        self.counter.get()
    }

    /// Set the 64-bit counter value.
    #[inline]
    pub fn set(&mut self, value: u64) {
        self.counter.set(value);
    }

    /// Increment the counter by 1.
    #[inline]
    pub fn increment(&mut self) {
        self.counter.increment_one();
    }

    /// Read the low 32 bits.
    #[inline]
    pub fn read_low(&self) -> u32 {
        self.counter.read_low()
    }

    /// Read the high 32 bits.
    #[inline]
    pub fn read_high(&self) -> u32 {
        self.counter.read_high()
    }

    /// Write the low 32 bits.
    #[inline]
    pub fn write_low(&mut self, value: u32) {
        self.counter.write_low(value);
    }

    /// Write the high 32 bits.
    #[inline]
    pub fn write_high(&mut self, value: u32) {
        self.counter.write_high(value);
    }
}

impl CsrRegister for Mhpmcounter {
    fn address(&self) -> u16 {
        csr_addr::mhpmcounter_addr(self.index).unwrap_or(0xB03)
    }

    fn name(&self) -> &'static str {
        // This is a bit of a hack since we need a static str
        // In practice, we use different instances for each counter
        "mhpmcounter"
    }

    fn read(&self) -> u32 {
        self.counter.read_low()
    }

    fn write(&mut self, value: u32) {
        self.counter.write_low(value);
    }

    fn min_privilege(&self) -> PrivilegeLevel {
        PrivilegeLevel::Machine
    }
}

/// Hardware Performance Event Selector (mhpmevent3-31).
///
/// Controls which event each HPM counter monitors.
/// The event code is stored in the lower bits.
#[derive(Debug, Clone, Copy, Default)]
pub struct Mhpmevent {
    value: u32,
    index: usize, // 3-31
}

impl Mhpmevent {
    /// Create a new event selector for the given index (3-31).
    pub fn new(index: usize) -> Self {
        debug_assert!((3..=31).contains(&index));
        Self {
            value: 0,
            index: index.clamp(3, 31),
        }
    }

    /// Get the event selector index (3-31).
    pub fn index(&self) -> usize {
        self.index
    }

    /// Get the selected event.
    pub fn event(&self) -> PerfEvent {
        PerfEvent::from_code(self.value as u8)
    }

    /// Set the event to monitor.
    pub fn set_event(&mut self, event: PerfEvent) {
        self.value = event.code() as u32;
    }

    /// Read the raw value.
    pub fn read(&self) -> u32 {
        self.value
    }

    /// Write the raw value.
    pub fn write(&mut self, value: u32) {
        // Only lower 16 bits are used for event selection
        self.value = value & 0xFFFF;
    }
}

impl CsrRegister for Mhpmevent {
    fn address(&self) -> u16 {
        csr_addr::mhpmevent_addr(self.index).unwrap_or(0x323)
    }

    fn name(&self) -> &'static str {
        "mhpmevent"
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

/// Collection of all performance counters.
#[derive(Debug, Clone)]
pub struct PerfCounters {
    /// Cycle counter
    pub mcycle: Mcycle,
    /// Cycle counter high
    pub mcycleh: Mcycleh,
    /// Instruction counter
    pub minstret: Minstret,
    /// Instruction counter high
    pub minstreth: Minstreth,
    /// Counter inhibit register
    pub mcountinhibit: Mcountinhibit,
    /// HPM counters (indices 3-31, stored as array of HPM_COUNTER_COUNT elements)
    pub mhpmcounters: [Mhpmcounter; HPM_COUNTER_COUNT],
    /// HPM event selectors (indices 3-31)
    pub mhpmevents: [Mhpmevent; HPM_COUNTER_COUNT],
}

impl Default for PerfCounters {
    fn default() -> Self {
        Self::new()
    }
}

impl PerfCounters {
    /// Create a new performance counters collection.
    pub fn new() -> Self {
        // Initialize HPM counters with their indices (3-31)
        let mhpmcounters: [Mhpmcounter; HPM_COUNTER_COUNT] =
            std::array::from_fn(|i| Mhpmcounter::new(i + HPM_COUNTER_BASE));
        let mhpmevents: [Mhpmevent; HPM_COUNTER_COUNT] =
            std::array::from_fn(|i| Mhpmevent::new(i + HPM_COUNTER_BASE));

        Self {
            mcycle: Mcycle::new(),
            mcycleh: Mcycleh::new(),
            minstret: Minstret::new(),
            minstreth: Minstreth::new(),
            mcountinhibit: Mcountinhibit::new(),
            mhpmcounters,
            mhpmevents,
        }
    }

    /// Increment cycle counter if not inhibited.
    pub fn tick(&mut self) {
        if !self.mcountinhibit.cy() {
            self.mcycle.increment(1);
        }
    }

    /// Increment instruction counter if not inhibited.
    pub fn instruction_retired(&mut self) {
        if !self.mcountinhibit.ir() {
            self.minstret.increment(1);
        }
    }

    /// Record a performance event.
    ///
    /// This increments all HPM counters that are configured to monitor this event.
    /// Record a performance event to all configured HPM counters.
    ///
    /// Early exit if all HPM counters are globally inhibited.
    pub fn record_event(&mut self, event: PerfEvent) {
        // Early exit if all HPM counters are inhibited
        if self.mcountinhibit.hpms() {
            return;
        }

        // Iterate through all HPM counters
        for i in 0..HPM_COUNTER_COUNT {
            // Skip if this specific counter is inhibited
            if self.mcountinhibit.hpm(i + HPM_COUNTER_BASE) {
                continue;
            }
            // Increment counter if it's configured for this event
            if self.mhpmevents[i].event() == event {
                self.mhpmcounters[i].increment();
            }
        }
    }

    /// Get the 64-bit cycle count.
    pub fn cycle_count(&self) -> u64 {
        ((self.mcycleh.read() as u64) << 32) | (self.mcycle.read() as u64)
    }

    /// Get the 64-bit instruction count.
    pub fn instruction_count(&self) -> u64 {
        ((self.minstreth.read() as u64) << 32) | (self.minstret.read() as u64)
    }

    /// Reset all counters to zero.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcycle() {
        let mut mcycle = Mcycle::new();
        assert_eq!(mcycle.get(), 0);

        mcycle.increment(1);
        assert_eq!(mcycle.get(), 1);

        mcycle.increment(0xFFFFFFFF);
        assert_eq!(mcycle.get(), 0x100000000);
        assert_eq!(mcycle.read_low(), 0);
        assert_eq!(mcycle.read_high(), 1);
    }

    #[test]
    fn test_minstret() {
        let mut minstret = Minstret::new();
        assert_eq!(minstret.get(), 0);

        minstret.increment(1);
        assert_eq!(minstret.get(), 1);

        minstret.increment(100);
        assert_eq!(minstret.get(), 101);
    }

    #[test]
    fn test_perf_counters() {
        let mut counters = PerfCounters::new();

        // Test cycle counting
        counters.tick();
        counters.tick();
        counters.tick();
        assert_eq!(counters.cycle_count(), 3);

        // Test instruction counting
        counters.instruction_retired();
        counters.instruction_retired();
        assert_eq!(counters.instruction_count(), 2);
    }

    #[test]
    fn test_perf_counters_inhibit() {
        let mut counters = PerfCounters::new();

        // Inhibit cycle counter
        counters.mcountinhibit.write(0x01); // bit 0 = cy
        assert!(counters.mcountinhibit.cy());

        counters.tick();
        assert_eq!(counters.cycle_count(), 0); // Should not increment

        // Inhibit instruction counter
        counters.mcountinhibit.write(0x02); // bit 1 = ir
        counters.instruction_retired();
        assert_eq!(counters.instruction_count(), 0); // Should not increment
    }

    #[test]
    fn test_hpm_counters() {
        let mut counters = PerfCounters::new();

        // Configure counter 3 to monitor LoadUseStalls
        counters.mhpmevents[0].set_event(PerfEvent::LoadUseStalls);
        assert_eq!(counters.mhpmevents[0].event(), PerfEvent::LoadUseStalls);

        // Record events
        counters.record_event(PerfEvent::LoadUseStalls);
        counters.record_event(PerfEvent::LoadUseStalls);
        counters.record_event(PerfEvent::Cycles); // Different event

        assert_eq!(counters.mhpmcounters[0].get(), 2);
    }

    #[test]
    fn test_perf_event_codes() {
        assert_eq!(PerfEvent::None.code(), 0);
        assert_eq!(PerfEvent::Cycles.code(), 1);
        assert_eq!(PerfEvent::InstructionsRetired.code(), 2);

        assert_eq!(PerfEvent::from_code(0), PerfEvent::None);
        assert_eq!(PerfEvent::from_code(1), PerfEvent::Cycles);
        assert_eq!(PerfEvent::from_code(255), PerfEvent::None);
    }

    #[test]
    fn test_csr_addr_helpers() {
        assert_eq!(csr_addr::mhpmcounter_addr(3), Some(0xB03));
        assert_eq!(csr_addr::mhpmcounter_addr(31), Some(0xB1F));
        assert_eq!(csr_addr::mhpmcounter_addr(2), None);

        assert_eq!(csr_addr::mhpmevent_addr(3), Some(0x323));
        assert_eq!(csr_addr::mhpmevent_addr(31), Some(0x33F));
    }
}
