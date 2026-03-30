//! DiffTest Framework for RISC-V Simulator
//!
//! This module implements differential testing by comparing the simulator's
//! state against QEMU's execution on an instruction-by-instruction basis.
//!
//! # Overview
//!
//! DiffTest runs the same program in both myCPU and QEMU, comparing their
//! states after each instruction. Any discrepancy indicates a potential bug
//! in the simulator implementation.
//!
//! # Usage
//!
//! ```rust,no_run
//! use mycpu::difftest::DiffTest;
//! use mycpu::cpu::Cpu;
//! use mycpu::memory::Bus;
//!
//! let mut cpu = Cpu::new(bus);
//! let mut difftest = DiffTest::connect("localhost:1234")?;
//!
//! loop {
//!     cpu.step()?;
//!     difftest.step_and_compare(&cpu)?;
//! }
//! ```
//!
//! # QEMU Setup
//!
//! Start QEMU with GDB server:
//! ```bash
//! qemu-system-riscv32 -M virt -nographic -bios none \
//!   -kernel program.elf -s -S
//! ```
//!
//! The `-s` flag opens a GDB server on port 1234, and `-S` pauses at startup.

mod protocol;

use crate::cpu::{ComparisonLevel, CpuState};
use crate::error::{Result, SimError};
use crate::types::Addr;
use std::collections::VecDeque;
use std::net::TcpStream;

pub use protocol::GdbProtocol;

/// DiffTest runner for comparing simulator with QEMU
pub struct DiffTest {
    /// GDB protocol handler
    protocol: GdbProtocol,
    /// Number of instructions compared
    instructions_compared: u64,
    /// Comparison level
    level: ComparisonLevel,
    /// Enable verbose logging
    verbose: bool,
    /// Last known good PC
    last_pc: Addr,
    /// Instruction history for debugging
    history: VecDeque<HistoryEntry>,
}

/// History entry for debugging
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    /// Instruction number
    pub instr_num: u64,
    /// PC at this instruction
    pub pc: Addr,
    /// Raw instruction bytes
    pub instruction: u32,
    /// Whether comparison passed
    pub passed: bool,
}

/// DiffTest error with detailed information
#[derive(Debug, Clone)]
pub struct DiffTestError {
    /// Instruction number where mismatch occurred
    pub instruction: u64,
    /// myCPU state
    pub mycpu_state: CpuState,
    /// QEMU state
    pub qemu_state: CpuState,
    /// PC where mismatch occurred
    pub pc: Addr,
    /// Register that differs (if any)
    pub differing_reg: Option<usize>,
    /// Human-readable description
    pub description: String,
}

impl std::fmt::Display for DiffTestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "DiffTest mismatch at instruction {}:", self.instruction)?;
        writeln!(f, "  PC: {}", self.pc)?;
        if let Some(reg) = self.differing_reg {
            writeln!(
                f,
                "  Register x{}: myCPU=0x{:08x} QEMU=0x{:08x}",
                reg, self.mycpu_state.regs[reg], self.qemu_state.regs[reg]
            )?;
        }
        writeln!(f, "\nmyCPU state:")?;
        writeln!(f, "  PC: 0x{:08x}", self.mycpu_state.pc.raw())?;
        for i in 0..32 {
            writeln!(f, "  x{:02}: 0x{:08x}", i, self.mycpu_state.regs[i])?;
        }
        writeln!(f, "\nQEMU state:")?;
        writeln!(f, "  PC: 0x{:08x}", self.qemu_state.pc.raw())?;
        for i in 0..32 {
            writeln!(f, "  x{:02}: 0x{:08x}", i, self.qemu_state.regs[i])?;
        }
        Ok(())
    }
}

impl DiffTest {
    /// Connect to QEMU's GDB server
    pub fn connect(addr: &str) -> Result<Self> {
        let stream = TcpStream::connect(addr)
            .map_err(|e| SimError::IoError(format!("Failed to connect to QEMU at {}: {}", addr, e)))?;

        let mut protocol = GdbProtocol::new(stream);

        // Wait for initial acknowledgment
        protocol.handshake()
            .map_err(|e| SimError::IoError(format!("Handshake failed: {}", e)))?;

        log::info!("Connected to QEMU GDB server at {}", addr);

        Ok(Self {
            protocol,
            instructions_compared: 0,
            level: ComparisonLevel::Standard,
            verbose: false,
            last_pc: Addr::new(0),
            history: VecDeque::new(),
        })
    }

    /// Set comparison level
    pub fn set_level(&mut self, level: ComparisonLevel) {
        self.level = level;
    }

    /// Enable verbose logging
    pub fn set_verbose(&mut self, verbose: bool) {
        self.verbose = verbose;
    }

    /// Get number of instructions compared
    pub fn instructions_compared(&self) -> u64 {
        self.instructions_compared
    }

    /// Get instruction history
    pub fn history(&self) -> &VecDeque<HistoryEntry> {
        &self.history
    }

    /// Read current state from QEMU
    pub fn read_qemu_state(&mut self) -> Result<CpuState> {
        let mut state = CpuState::new();

        // Read PC
        let pc = self.protocol.read_register(0x21) // 0x21 = PC in GDB
            .map_err(|e| SimError::IoError(format!("Read PC failed: {}", e)))?;
        state.pc = Addr::new(pc);

        // Read general-purpose registers (x0-x31)
        for i in 0..32 {
            let reg_idx = i as u8;
            state.regs[i] = self.protocol.read_register(reg_idx)
                .map_err(|e| SimError::IoError(format!("Read reg {} failed: {}", i, e)))?;
        }

        Ok(state)
    }

    /// Single step QEMU
    pub fn step_qemu(&mut self) -> Result<()> {
        self.protocol.step()
            .map_err(|e| SimError::IoError(format!("Step failed: {}", e)))?;
        Ok(())
    }

    /// Step QEMU and compare states
    pub fn step_and_compare(&mut self, mycpu_state: &CpuState) -> std::result::Result<(), DiffTestError> {
        // Step QEMU
        if let Err(e) = self.protocol.step() {
            return Err(DiffTestError {
                instruction: self.instructions_compared,
                mycpu_state: mycpu_state.clone(),
                qemu_state: CpuState::new(),
                pc: self.last_pc,
                differing_reg: None,
                description: format!("QEMU step failed: {}", e),
            });
        }

        // Read QEMU state
        let qemu_state = match self.read_qemu_state() {
            Ok(s) => s,
            Err(e) => {
                return Err(DiffTestError {
                    instruction: self.instructions_compared,
                    mycpu_state: mycpu_state.clone(),
                    qemu_state: CpuState::new(),
                    pc: self.last_pc,
                    differing_reg: None,
                    description: format!("Failed to read QEMU state: {}", e),
                });
            }
        };

        // Compare states
        let passed = mycpu_state.compare(&qemu_state, self.level);

        // Record history
        let entry = HistoryEntry {
            instr_num: self.instructions_compared,
            pc: mycpu_state.pc,
            instruction: 0, // Would need to fetch from memory
            passed,
        };
        self.history.push_back(entry);

        // Keep history bounded (O(1) with VecDeque)
        if self.history.len() > 1000 {
            self.history.pop_front();
        }

        self.instructions_compared += 1;
        self.last_pc = mycpu_state.pc;

        if self.verbose {
            log::debug!(
                "Instruction {}: PC=0x{:08x} {}",
                self.instructions_compared,
                mycpu_state.pc.raw(),
                if passed { "OK" } else { "MISMATCH" }
            );
        }

        if !passed {
            // Find differing register
            let differing_reg = (0..32).find(|&i| mycpu_state.regs[i] != qemu_state.regs[i]);

            return Err(DiffTestError {
                instruction: self.instructions_compared,
                mycpu_state: mycpu_state.clone(),
                qemu_state,
                pc: mycpu_state.pc,
                differing_reg,
                description: "State mismatch".to_string(),
            });
        }

        Ok(())
    }

    /// Load program into QEMU
    pub fn load_program(&mut self, _path: &str) -> Result<()> {
        // This would use GDB's 'load' command
        // For now, we assume QEMU was started with the program already loaded
        log::info!("Program should be pre-loaded in QEMU");
        Ok(())
    }

    /// Continue QEMU execution
    pub fn continue_qemu(&mut self) -> Result<()> {
        self.protocol.continue_exec()
            .map_err(|e| SimError::IoError(format!("Continue failed: {}", e)))?;
        Ok(())
    }

    /// Close connection
    pub fn close(mut self) -> Result<()> {
        self.protocol.disconnect()
            .map_err(|e| SimError::IoError(format!("Disconnect failed: {}", e)))?;
        Ok(())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_entry() {
        let entry = HistoryEntry {
            instr_num: 100,
            pc: Addr::new(0x80000100),
            instruction: 0x12345678,
            passed: true,
        };
        assert!(entry.passed);
        assert_eq!(entry.instr_num, 100);
    }

    #[test]
    fn test_difftest_error_display() {
        let error = DiffTestError {
            instruction: 50,
            mycpu_state: CpuState::new(),
            qemu_state: CpuState::new(),
            pc: Addr::new(0x80000000),
            differing_reg: Some(5),
            description: "Test error".to_string(),
        };
        let s = format!("{}", error);
        assert!(s.contains("instruction 50"));
        assert!(s.contains("x5"));
    }
}
