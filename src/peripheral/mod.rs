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

mod uart;

pub use uart::{OutputCallback, Uart, UART_BASE, UART_IRQ, UART_SIZE};
