//! Error types for the RISC-V simulator.
//!
//! This module defines all error types used throughout the simulator.

use thiserror::Error;

use crate::types::Addr;

/// The main error type for the simulator.
#[derive(Debug, Error)]
pub enum SimError {
    /// Memory access error
    #[error("Memory error at address {addr}: {message}")]
    Memory {
        /// The address where the error occurred
        addr: Addr,
        /// Error message
        message: String,
    },

    /// Invalid memory access (out of bounds)
    #[error("Memory access out of bounds: address {addr}, size {size}")]
    MemoryOutOfBounds {
        /// The address that was accessed
        addr: Addr,
        /// The size of the access
        size: usize,
    },

    /// Invalid instruction
    #[error("Invalid instruction at PC {pc}: 0x{instruction:08x}")]
    InvalidInstruction {
        /// The PC where the invalid instruction was found
        pc: Addr,
        /// The raw instruction word
        instruction: u32,
    },

    /// Unsupported instruction
    #[error("Unsupported instruction at PC {pc}: {message}")]
    UnsupportedInstruction {
        /// The PC where the instruction was found
        pc: Addr,
        /// Error message
        message: String,
    },

    /// Unimplemented feature
    #[error("Unimplemented: {0}")]
    Unimplemented(String),

    /// Breakpoint hit
    #[error("Breakpoint hit at PC {0}")]
    Breakpoint(Addr),

    /// Environment call (ecall)
    #[error("Environment call from {mode} mode")]
    Ecall {
        /// The privilege mode when ecall was executed
        mode: String,
    },

    /// Breakpoint instruction (ebreak)
    #[error("Breakpoint instruction at PC {0}")]
    Ebreak(Addr),

    /// Invalid CSR address
    #[error("Invalid CSR address: 0x{0:04x}")]
    InvalidCsr(u16),

    /// CSR access not allowed
    #[error("CSR 0x{csr:04x} access not allowed from {mode} mode")]
    CsrAccessDenied {
        /// CSR address
        csr: u16,
        /// Current privilege mode
        mode: String,
    },

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// ELF loading error
    #[error("ELF loading error: {0}")]
    ElfLoad(String),

    /// Peripheral error
    #[error("Peripheral error: {0}")]
    Peripheral(String),

    /// CPU halted
    #[error("CPU halted")]
    Halted,
}

/// Result type alias for simulator operations.
pub type Result<T> = std::result::Result<T, SimError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = SimError::Memory {
            addr: Addr::new(0x1000),
            message: "read error".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Memory error at address 0x00001000: read error"
        );
    }

    #[test]
    fn test_invalid_instruction_display() {
        let err = SimError::InvalidInstruction {
            pc: Addr::new(0x80000000),
            instruction: 0xDEADBEEF,
        };
        assert_eq!(
            err.to_string(),
            "Invalid instruction at PC 0x80000000: 0xdeadbeef"
        );
    }
}
