//! Pipeline stage implementations.
//!
//! This module provides the individual pipeline stage implementations for the
//! 5-stage pipeline: IF, ID, EX, MEM, WB.

pub mod decode;
pub mod execute;
pub mod fetch;
pub mod memory;
pub mod writeback;

pub use decode::DecodeStage;
pub use execute::ExecuteStage;
pub use fetch::FetchStage;
pub use memory::MemoryStage;
pub use writeback::WritebackStage;
