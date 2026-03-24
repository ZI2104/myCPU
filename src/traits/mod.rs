//! Core traits for the RISC-V simulator.
//!
//! This module defines the fundamental abstractions used throughout
//! the simulator, including memory and peripheral interfaces.

mod memory;
mod peripheral;

pub use memory::Memory;
pub use peripheral::Peripheral;
