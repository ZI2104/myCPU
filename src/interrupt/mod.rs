//! Interrupt controllers for RISC-V.
//!
//! This module provides:
//! - CLINT (Core Local Interruptor): Timer and software interrupts
//! - PLIC (Platform Level Interrupt Controller): External interrupts

pub mod clint;
pub mod plic;

pub use clint::{Clint, CLINT_BASE, CLINT_SIZE, MSIP_OFFSET, MTIMECMP_OFFSET, MTIME_OFFSET};
pub use plic::{Plic, MAX_SOURCES, PLIC_BASE, PLIC_SIZE};
