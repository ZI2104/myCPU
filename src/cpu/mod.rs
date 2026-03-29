//! CPU module for the RISC-V simulator.
//!
//! This module contains the CPU core implementation, including
//! registers, PC, execution models, and the main execution loop.

mod core;
mod execution_model;
mod pc;
mod registers;
mod state;

pub mod pipeline;

pub use core::Cpu;
pub use execution_model::ExecutionModel;
pub use pc::ProgramCounter;
pub use registers::Registers;
pub use state::{ComparisonLevel, CpuState};
