//! Performance event collector for tracking CPU execution statistics.
//!
//! This module provides a centralized way to track performance events
//! across both single-cycle and pipeline CPU implementations.

use crate::cpu::csr::PerfEvent;

/// Performance statistics collected during execution.
///
/// This struct tracks various performance metrics that can be used
/// to analyze CPU behavior and identify bottlenecks.
#[derive(Debug, Clone, Default)]
pub struct PerfCollector {
    /// Total cycles executed
    pub cycles: u64,
    /// Total instructions retired (completed)
    pub instructions_retired: u64,
    /// Load-use stalls (pipeline bubbles due to data hazard)
    pub load_use_stalls: u64,
    /// Control hazards (pipeline flushes due to branches)
    pub control_hazards: u64,
    /// Branch instructions executed
    pub branches_executed: u64,
    /// Branches taken
    pub branches_taken: u64,
    /// Branches not taken
    pub branches_not_taken: u64,
    /// Memory reads
    pub memory_reads: u64,
    /// Memory writes
    pub memory_writes: u64,
    /// ALU operations
    pub alu_operations: u64,
    /// CSR accesses
    pub csr_accesses: u64,
    /// Pipeline flushes
    pub pipeline_flushes: u64,
    /// Interrupts taken
    pub interrupts_taken: u64,
    /// Exceptions taken
    pub exceptions_taken: u64,
    /// Branch mispredictions (dynamic predictor)
    pub branch_mispredictions: u64,
    /// Cache hits (if cache simulation enabled)
    pub cache_hits: u64,
    /// Cache misses (if cache simulation enabled)
    pub cache_misses: u64,
    /// TLB hits (if TLB simulation enabled)
    pub tlb_hits: u64,
    /// TLB misses (if TLB simulation enabled)
    pub tlb_misses: u64,
}

impl PerfCollector {
    /// Create a new performance collector initialized to zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a performance event.
    ///
    /// This method should be called at the appropriate points in the
    /// CPU pipeline to track performance events.
    #[inline]
    pub fn record(&mut self, event: PerfEvent) {
        match event {
            PerfEvent::Cycles => self.cycles += 1,
            PerfEvent::InstructionsRetired => self.instructions_retired += 1,
            PerfEvent::LoadUseStalls => self.load_use_stalls += 1,
            PerfEvent::ControlHazards => self.control_hazards += 1,
            PerfEvent::BranchExecuted => self.branches_executed += 1,
            PerfEvent::BranchTaken => self.branches_taken += 1,
            PerfEvent::BranchNotTaken => self.branches_not_taken += 1,
            PerfEvent::MemoryReads => self.memory_reads += 1,
            PerfEvent::MemoryWrites => self.memory_writes += 1,
            PerfEvent::AluOperations => self.alu_operations += 1,
            PerfEvent::CsrAccesses => self.csr_accesses += 1,
            PerfEvent::PipelineFlushes => self.pipeline_flushes += 1,
            PerfEvent::InterruptsTaken => self.interrupts_taken += 1,
            PerfEvent::ExceptionsTaken => self.exceptions_taken += 1,
            PerfEvent::BranchMispredictions => self.branch_mispredictions += 1,
            PerfEvent::None => {}
        }
    }

    /// Record cache hit event (convenience API - avoids adding new PerfEvent codes)
    #[inline]
    pub fn record_cache_hit(&mut self) {
        self.cache_hits += 1;
    }

    /// Record cache miss event (convenience API - avoids adding new PerfEvent codes)
    #[inline]
    pub fn record_cache_miss(&mut self) {
        self.cache_misses += 1;
    }

    /// Record TLB hit
    #[inline]
    pub fn record_tlb_hit(&mut self) {
        self.tlb_hits += 1;
    }

    /// Record TLB miss
    #[inline]
    pub fn record_tlb_miss(&mut self) {
        self.tlb_misses += 1;
    }

    /// Reset all counters to zero.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Calculate Instructions Per Cycle (IPC).
    ///
    /// IPC is a key metric for pipeline efficiency.
    /// An ideal 5-stage pipeline should achieve IPC close to 1.0
    /// when there are no stalls.
    pub fn ipc(&self) -> f64 {
        if self.cycles == 0 {
            0.0
        } else {
            self.instructions_retired as f64 / self.cycles as f64
        }
    }

    /// Calculate Cycles Per Instruction (CPI).
    ///
    /// CPI is the inverse of IPC and represents the average
    /// number of cycles needed to complete one instruction.
    pub fn cpi(&self) -> f64 {
        if self.instructions_retired == 0 {
            0.0
        } else {
            self.cycles as f64 / self.instructions_retired as f64
        }
    }

    /// Calculate the stall rate (percentage of cycles spent stalling).
    ///
    /// This metric shows how much of the execution time is lost
    /// due to pipeline hazards.
    pub fn stall_rate(&self) -> f64 {
        if self.cycles == 0 {
            0.0
        } else {
            let total_stalls = self.load_use_stalls + self.control_hazards;
            total_stalls as f64 / self.cycles as f64
        }
    }

    /// Calculate branch prediction accuracy.
    ///
    /// Since we use static prediction (predict not taken),
    /// accuracy depends on the branch pattern of the program.
    pub fn branch_accuracy(&self) -> Option<f64> {
        if self.branches_executed == 0 {
            None
        } else {
            // Static "predict not taken" is correct when branch is not taken
            let correct = self.branches_not_taken;
            Some(correct as f64 / self.branches_executed as f64 * 100.0)
        }
    }

    /// Calculate memory operations per instruction.
    pub fn mops_per_instruction(&self) -> f64 {
        if self.instructions_retired == 0 {
            0.0
        } else {
            (self.memory_reads + self.memory_writes) as f64 / self.instructions_retired as f64
        }
    }

    /// Get total stalls (load-use + control hazards).
    pub fn total_stalls(&self) -> u64 {
        self.load_use_stalls + self.control_hazards
    }

    /// Merge another collector's counts into this one.
    pub fn merge(&mut self, other: &PerfCollector) {
        self.cycles += other.cycles;
        self.instructions_retired += other.instructions_retired;
        self.load_use_stalls += other.load_use_stalls;
        self.control_hazards += other.control_hazards;
        self.branches_executed += other.branches_executed;
        self.branches_taken += other.branches_taken;
        self.branches_not_taken += other.branches_not_taken;
        self.memory_reads += other.memory_reads;
        self.memory_writes += other.memory_writes;
        self.alu_operations += other.alu_operations;
        self.csr_accesses += other.csr_accesses;
        self.pipeline_flushes += other.pipeline_flushes;
        self.interrupts_taken += other.interrupts_taken;
        self.exceptions_taken += other.exceptions_taken;
        self.branch_mispredictions += other.branch_mispredictions;
        self.cache_hits += other.cache_hits;
        self.cache_misses += other.cache_misses;
        self.tlb_hits += other.tlb_hits;
        self.tlb_misses += other.tlb_misses;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perf_collector_basic() {
        let mut collector = PerfCollector::new();

        // Record some events
        for _ in 0..100 {
            collector.record(PerfEvent::Cycles);
        }
        for _ in 0..80 {
            collector.record(PerfEvent::InstructionsRetired);
        }

        assert_eq!(collector.cycles, 100);
        assert_eq!(collector.instructions_retired, 80);
        assert!((collector.ipc() - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_perf_collector_stalls() {
        let mut collector = PerfCollector::new();

        collector.cycles = 100;
        collector.load_use_stalls = 10;
        collector.control_hazards = 5;

        assert!((collector.stall_rate() - 0.15).abs() < 0.001);
        assert_eq!(collector.total_stalls(), 15);
    }

    #[test]
    fn test_perf_collector_branches() {
        let mut collector = PerfCollector::new();

        collector.branches_executed = 100;
        collector.branches_taken = 40;
        collector.branches_not_taken = 60;

        // Static prediction accuracy = not_taken / total
        let accuracy = collector.branch_accuracy().unwrap();
        assert!((accuracy - 60.0).abs() < 0.001);
    }

    #[test]
    fn test_perf_collector_merge() {
        let mut a = PerfCollector::new();
        let mut b = PerfCollector::new();

        a.cycles = 100;
        a.instructions_retired = 80;
        b.cycles = 50;
        b.instructions_retired = 40;

        a.merge(&b);

        assert_eq!(a.cycles, 150);
        assert_eq!(a.instructions_retired, 120);
    }

    #[test]
    fn test_perf_collector_reset() {
        let mut collector = PerfCollector::new();
        collector.cycles = 1000;
        collector.instructions_retired = 800;

        collector.reset();

        assert_eq!(collector.cycles, 0);
        assert_eq!(collector.instructions_retired, 0);
    }
}
