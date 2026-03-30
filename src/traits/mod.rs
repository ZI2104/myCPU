//! Core traits for the RISC-V simulator.
//!
//! This module defines the fundamental abstractions used throughout
//! the simulator, including memory and peripheral interfaces.

mod interrupt;
mod memory;
mod peripheral;

pub use interrupt::InterruptSource;
pub use memory::Memory;
pub use peripheral::Peripheral;
