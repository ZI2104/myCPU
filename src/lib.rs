//! myCPU - A RISC-V RV32I Instruction Set Simulator
//!
//! This crate provides a complete RISC-V RV32I instruction set simulator
//! with 5-stage pipeline support, designed for educational purposes.
//!
//! # Features
//!
//! - **RV32I Base Instruction Set**: Full support for the 40 base instructions
//! - **5-Stage Pipeline**: IF/ID/EX/MEM/WB with hazard detection and forwarding
//! - **Privilege Modes**: M-mode, S-mode, and U-mode support
//! - **Memory System**: RAM, ROM, and memory-mapped peripherals via system bus
//! - **Debug Support**: GDB Remote Protocol for IDE integration
//! - **DiffTest**: QEMU-based differential testing
//!
//! # Architecture
//!
//! The simulator is organized into the following modules:
//!
//! - [`types`]: Core type definitions (Addr, Word, Byte, etc.)
//! - [`error`]: Error types for the simulator
//! - [`traits`]: Core traits (Memory, Peripheral)
//! - [`memory`]: Memory implementations (RAM, ROM, Bus)
//! - [`cpu`]: CPU core, registers, and state
//!
//! # Example
//!
//! ```rust,no_run
//! use mycpu::cpu::Cpu;
//! use mycpu::memory::{Bus, Ram};
//! use mycpu::types::Addr;
//!
//! // Create a system bus and attach RAM
//! let mut bus = Bus::new();
//! let ram = Ram::new(1024 * 1024); // 1MB RAM
//! bus.attach_memory(Addr::new(0x80000000), ram, "Main RAM");
//!
//! // Create CPU
//! let mut cpu = Cpu::with_pc(bus, Addr::new(0x80000000));
//!
//! // Run 1000 instructions
//! cpu.run(1000).unwrap();
//! ```

pub mod cpu;
pub mod error;
pub mod instruction;
pub mod interrupt;
pub mod memory;
pub mod traits;
pub mod types;

// Re-export commonly used types
pub use cpu::{Cpu, CpuState, ProgramCounter, Registers};
pub use error::{Result, SimError};
pub use memory::{Bus, Ram, Rom};
pub use traits::{Memory, Peripheral};
pub use types::{Addr, Byte, Half, PrivilegeLevel, RegIdx, Word};

/// The version of this crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
