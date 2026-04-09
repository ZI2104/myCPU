//! Core traits for the RISC-V simulator.
//!
//! This module defines the fundamental abstractions used throughout
//! the simulator, including memory and peripheral interfaces.

mod accelerator;
mod interrupt;
mod memory;
mod peripheral;

pub use accelerator::{
    Accelerator, AcceleratorPerfCounters, AcceleratorType, KernelType, Precision, TensorDescriptor,
};
pub use interrupt::InterruptSource;
pub use memory::Memory;
pub use peripheral::Peripheral;
