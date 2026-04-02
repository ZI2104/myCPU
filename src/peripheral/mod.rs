//! Peripheral module
//!
//! This module provides peripheral implementations for the RISC-V simulator:
//! - **UART**: NS16550A compatible serial port
//!
//! Memory map (QEMU virt machine compatible):
//! - UART: 0x1000_0000 - 0x1000_00FF
//!
//! # Example
//!
//! ```rust,no_run
//! use mycpu::peripheral::Uart;
//! use mycpu::memory::Bus;
//! use mycpu::types::Addr;
//!
//! let mut bus = Bus::new();
//! let uart = Uart::new();
//! bus.attach_peripheral(Addr::new(0x1000_0000), Box::new(uart));
//! ```

mod input;
mod lpu;
mod npu;
mod uart;
mod virtio_block;

pub use input::{InputDevice, INPUT_BASE, INPUT_SIZE};
pub use lpu::{Lpu, LpuSnapshot, LPU_BASE, LPU_SIZE};
pub use npu::{Npu, NpuSnapshot, NPU_BASE, NPU_SIZE};
pub use uart::{OutputCallback, Uart, UART_BASE, UART_IRQ, UART_SIZE};
pub use virtio_block::{VirtioBlock, VirtioGuestMemory, VIRTIO_BLK_BASE, VIRTIO_BLK_SIZE};
