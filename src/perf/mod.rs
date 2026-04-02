//! Performance-related modules.
//!
//! This namespace groups performance reporting and formatting utilities.

pub mod report;

pub use report::{format_brief, BranchStats, MemoryStats, PerfReport};
