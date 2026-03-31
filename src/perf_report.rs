//! Performance report generation for myCPU.
//!
//! This module provides utilities for generating human-readable performance
//! reports from collected performance statistics.

use crate::cpu::PerfCollector;
use std::fmt;

/// Performance report containing execution statistics.
#[derive(Debug, Clone)]
pub struct PerfReport {
    /// Total cycles executed
    pub cycles: u64,
    /// Total instructions executed
    pub instructions: u64,
    /// Instructions Per Cycle
    pub ipc: f64,
    /// Cycles Per Instruction
    pub cpi: f64,
    /// Load-use stalls
    pub load_use_stalls: u64,
    /// Control hazards
    pub control_hazards: u64,
    /// Total stalls
    pub total_stalls: u64,
    /// Stall rate (percentage)
    pub stall_rate: f64,
    /// Load-use stall rate among cycles (percentage)
    pub load_use_stall_rate: f64,
    /// Control hazard rate among cycles (percentage)
    pub control_hazard_rate: f64,
    /// Load-use share among total stalls (percentage)
    pub load_use_stall_share: f64,
    /// Control hazard share among total stalls (percentage)
    pub control_hazard_share: f64,
    /// Branch statistics
    pub branches: BranchStats,
    /// Memory statistics
    pub memory: MemoryStats,
    /// Pipeline efficiency (ideal CPI / actual CPI)
    pub efficiency: f64,
}

/// Branch prediction statistics.
#[derive(Debug, Clone)]
pub struct BranchStats {
    /// Total branches executed
    pub total: u64,
    /// Branches taken
    pub taken: u64,
    /// Branches not taken
    pub not_taken: u64,
    /// Prediction accuracy (for static "predict not taken")
    pub accuracy: Option<f64>,
}

/// Memory access statistics.
#[derive(Debug, Clone)]
pub struct MemoryStats {
    /// Memory reads
    pub reads: u64,
    /// Memory writes
    pub writes: u64,
    /// Total memory operations
    pub total: u64,
    /// Memory operations per instruction
    pub mops_per_instruction: f64,
}

impl PerfReport {
    /// Generate a performance report from a PerfCollector.
    pub fn from_collector(collector: &PerfCollector) -> Self {
        let cycles = collector.cycles;
        let instructions = collector.instructions_retired;

        // Calculate IPC and CPI
        let ipc = collector.ipc();
        let cpi = collector.cpi();

        // Calculate stall statistics
        let load_use_stalls = collector.load_use_stalls;
        let control_hazards = collector.control_hazards;
        let total_stalls = collector.total_stalls();
        let stall_rate = collector.stall_rate() * 100.0;
        let load_use_stall_rate = if cycles > 0 {
            load_use_stalls as f64 / cycles as f64 * 100.0
        } else {
            0.0
        };
        let control_hazard_rate = if cycles > 0 {
            control_hazards as f64 / cycles as f64 * 100.0
        } else {
            0.0
        };
        let load_use_stall_share = if total_stalls > 0 {
            load_use_stalls as f64 / total_stalls as f64 * 100.0
        } else {
            0.0
        };
        let control_hazard_share = if total_stalls > 0 {
            control_hazards as f64 / total_stalls as f64 * 100.0
        } else {
            0.0
        };

        // Calculate branch statistics
        let branches = BranchStats {
            total: collector.branches_executed,
            taken: collector.branches_taken,
            not_taken: collector.branches_not_taken,
            accuracy: collector.branch_accuracy(),
        };

        // Calculate memory statistics
        let memory = MemoryStats {
            reads: collector.memory_reads,
            writes: collector.memory_writes,
            total: collector.memory_reads + collector.memory_writes,
            mops_per_instruction: collector.mops_per_instruction(),
        };

        // Calculate efficiency (ideal CPI = 1.0 for a perfect pipeline)
        let efficiency = if cpi > 0.0 { 1.0 / cpi * 100.0 } else { 0.0 };

        Self {
            cycles,
            instructions,
            ipc,
            cpi,
            load_use_stalls,
            control_hazards,
            total_stalls,
            stall_rate,
            load_use_stall_rate,
            control_hazard_rate,
            load_use_stall_share,
            control_hazard_share,
            branches,
            memory,
            efficiency,
        }
    }

    /// Format a large number with thousand separators.
    fn format_number(n: u64) -> String {
        let s = n.to_string();
        let mut result = String::new();
        for (i, c) in s.chars().enumerate() {
            if i > 0 && (s.len() - i) % 3 == 0 {
                result.push(',');
            }
            result.push(c);
        }
        result
    }
}

impl fmt::Display for PerfReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f)?;
        writeln!(
            f,
            "╔══════════════════════════════════════════════════════════════╗"
        )?;
        writeln!(
            f,
            "║                    Performance Report                         ║"
        )?;
        writeln!(
            f,
            "╠══════════════════════════════════════════════════════════════╣"
        )?;
        writeln!(
            f,
            "║  Execution Summary                                            ║"
        )?;
        writeln!(
            f,
            "╠══════════════════════════════════════════════════════════════╣"
        )?;
        writeln!(
            f,
            "║  Cycles:          {:>42} ║",
            Self::format_number(self.cycles)
        )?;
        writeln!(
            f,
            "║  Instructions:    {:>42} ║",
            Self::format_number(self.instructions)
        )?;
        writeln!(f, "║  IPC:             {:>42.4} ║", self.ipc)?;
        writeln!(f, "║  CPI:             {:>42.4} ║", self.cpi)?;
        writeln!(f, "║  Efficiency:      {:>41.1}% ║", self.efficiency)?;
        writeln!(
            f,
            "╠══════════════════════════════════════════════════════════════╣"
        )?;
        writeln!(
            f,
            "║  Pipeline Hazards                                             ║"
        )?;
        writeln!(
            f,
            "╠══════════════════════════════════════════════════════════════╣"
        )?;
        writeln!(
            f,
            "║  Load-Use Stalls: {:>42} ║",
            Self::format_number(self.load_use_stalls)
        )?;
        writeln!(
            f,
            "║    └─ Cycle Rate: {:>39.1}% ║",
            self.load_use_stall_rate
        )?;
        writeln!(
            f,
            "║    └─ Stall Share:{:>39.1}% ║",
            self.load_use_stall_share
        )?;
        writeln!(
            f,
            "║  Control Hazards: {:>42} ║",
            Self::format_number(self.control_hazards)
        )?;
        writeln!(
            f,
            "║    └─ Cycle Rate: {:>39.1}% ║",
            self.control_hazard_rate
        )?;
        writeln!(
            f,
            "║    └─ Stall Share:{:>39.1}% ║",
            self.control_hazard_share
        )?;
        writeln!(
            f,
            "║  Total Stalls:    {:>42} ║",
            Self::format_number(self.total_stalls)
        )?;
        writeln!(f, "║  Stall Rate:      {:>41.1}% ║", self.stall_rate)?;
        writeln!(
            f,
            "╠══════════════════════════════════════════════════════════════╣"
        )?;
        writeln!(
            f,
            "║  Branch Statistics                                            ║"
        )?;
        writeln!(
            f,
            "╠══════════════════════════════════════════════════════════════╣"
        )?;
        writeln!(
            f,
            "║  Total Branches:  {:>42} ║",
            Self::format_number(self.branches.total)
        )?;
        writeln!(
            f,
            "║  Taken:           {:>42} ║",
            Self::format_number(self.branches.taken)
        )?;
        writeln!(
            f,
            "║  Not Taken:       {:>42} ║",
            Self::format_number(self.branches.not_taken)
        )?;
        if let Some(accuracy) = self.branches.accuracy {
            writeln!(f, "║  Prediction Acc:  {:>41.1}% ║", accuracy)?;
        } else {
            writeln!(f, "║  Prediction Acc:  {:>42} ║", "N/A")?;
        }
        writeln!(
            f,
            "╠══════════════════════════════════════════════════════════════╣"
        )?;
        writeln!(
            f,
            "║  Memory Statistics                                            ║"
        )?;
        writeln!(
            f,
            "╠══════════════════════════════════════════════════════════════╣"
        )?;
        writeln!(
            f,
            "║  Memory Reads:    {:>42} ║",
            Self::format_number(self.memory.reads)
        )?;
        writeln!(
            f,
            "║  Memory Writes:   {:>42} ║",
            Self::format_number(self.memory.writes)
        )?;
        writeln!(
            f,
            "║  Total Mem Ops:   {:>42} ║",
            Self::format_number(self.memory.total)
        )?;
        writeln!(
            f,
            "║  Mem Ops/Instr:   {:>42.2} ║",
            self.memory.mops_per_instruction
        )?;
        writeln!(
            f,
            "╚══════════════════════════════════════════════════════════════╝"
        )?;

        Ok(())
    }
}

impl BranchStats {
    /// Create empty branch statistics.
    pub fn empty() -> Self {
        Self {
            total: 0,
            taken: 0,
            not_taken: 0,
            accuracy: None,
        }
    }
}

impl MemoryStats {
    /// Create empty memory statistics.
    pub fn empty() -> Self {
        Self {
            reads: 0,
            writes: 0,
            total: 0,
            mops_per_instruction: 0.0,
        }
    }
}

/// Generate a brief one-line performance summary.
pub fn format_brief(collector: &PerfCollector) -> String {
    format!(
        "{} instructions, {} cycles, IPC={:.2}, stalls={:.0}%",
        PerfReport::format_number(collector.instructions_retired),
        PerfReport::format_number(collector.cycles),
        collector.ipc(),
        collector.stall_rate() * 100.0
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::csr::PerfEvent;

    #[test]
    fn test_perf_report_generation() {
        let mut collector = PerfCollector::new();

        // Simulate some execution
        for _ in 0..1000 {
            collector.record(PerfEvent::Cycles);
        }
        for _ in 0..800 {
            collector.record(PerfEvent::InstructionsRetired);
        }
        for _ in 0..50 {
            collector.record(PerfEvent::LoadUseStalls);
        }
        for _ in 0..30 {
            collector.record(PerfEvent::ControlHazards);
        }

        let report = PerfReport::from_collector(&collector);

        assert_eq!(report.cycles, 1000);
        assert_eq!(report.instructions, 800);
        assert!((report.ipc - 0.8).abs() < 0.01);
        assert_eq!(report.total_stalls, 80);
    }

    #[test]
    fn test_perf_report_display() {
        let mut collector = PerfCollector::new();
        collector.cycles = 1000;
        collector.instructions_retired = 800;

        let report = PerfReport::from_collector(&collector);
        let output = format!("{}", report);

        assert!(output.contains("Performance Report"));
        assert!(output.contains("IPC"));
        assert!(output.contains("CPI"));
    }
}
