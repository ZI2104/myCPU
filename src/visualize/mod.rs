//! Visualization module for myCPU.
//!
//! This module provides WebSocket-based visualization capabilities,
//! allowing real-time inspection of CPU state via a web frontend.

pub mod server;
pub mod snapshot;

pub use server::{start_visualize_server, VisualizeServer};
pub use snapshot::{
    disassemble, CpuSnapshot, ExStageInfo, IdStageInfo, IfStageInfo, MemStageInfo,
    PerfSnapshot, PipelineSnapshot, WbStageInfo,
};
