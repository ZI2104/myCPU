//! RISC-V instruction module.
//!
//! This module provides instruction decoding and execution for the RV32I base instruction set.

pub mod decoder;
pub mod execute;
pub mod format;
pub mod opcode;

pub use decoder::{decode, DecodedInstr, Decoder};
pub use format::*;
