//! UART (NS16550A compatible) Serial Port implementation
//!
//! This module provides an NS16550A-compatible UART serial port:
//! - Transmit Holding Register (THR)
//! - Receive Holding Register (RBR)
//! - Line Status Register (LSR)
//! - Interrupt Enable Register (IER)
//! - FIFO Control Register (FCR)
//! - Line Control Register (LCR)
//! - Modem Control Register (MCR)
//! - Interrupt Identification Register (IIR)
//! - Scratch Register (SCR)
//! - Divisor Latch (DLL/DLM) for baud rate
//!
//! Memory-mapped I/O at 0x1000_0000 (QEMU virt machine compatible)

use crate::error::{Result, SimError};
use crate::traits::{Memory, Peripheral};
use crate::types::{Addr, Byte, Half, Word};
use std::collections::VecDeque;

// ============================================================================
// Register Offsets
// ============================================================================

/// Receive Holding Register (read) / Transmit Holding Register (write)
const RBR_THR: u32 = 0x00;
/// Interrupt Enable Register
const IER: u32 = 0x01;
/// Interrupt Identification Register (read) / FIFO Control Register (write)
const IIR_FCR: u32 = 0x02;
/// Line Control Register
const LCR: u32 = 0x03;
/// Modem Control Register
const MCR: u32 = 0x04;
/// Line Status Register
const LSR: u32 = 0x05;
/// Modem Status Register
const MSR: u32 = 0x06;
/// Scratch Register
const SCR: u32 = 0x07;

// ============================================================================
// Register Bit Definitions
// ============================================================================

/// IER bits
mod ier {
    pub const RX_ENABLE: u8 = 0x01; // Enable receive data interrupt
    pub const TX_ENABLE: u8 = 0x02; // Enable transmit empty interrupt
    #[allow(dead_code)]
    pub const LS_ENABLE: u8 = 0x04; // Enable line status interrupt
    #[allow(dead_code)]
    pub const MS_ENABLE: u8 = 0x08; // Enable modem status interrupt
}/// IIR bits
mod iir {
    pub const NO_INTERRUPT: u8 = 0x01; // No interrupt pending (bit 0)
    #[allow(dead_code)]
    pub const ID_MASK: u8 = 0x0E; // Interrupt ID mask (bits 3-1)
    pub const FIFO_ENABLE: u8 = 0xC0; // FIFO enabled (bits 7-6)

    // Interrupt IDs (bits 3-1)
    #[allow(dead_code)]
    pub const ID_LINE_STATUS: u8 = 0x06; // Overrun/parity/framing error
    pub const ID_RX_READY: u8 = 0x04; // Data available
    #[allow(dead_code)]
    pub const ID_TIMEOUT: u8 = 0x0C; // Character timeout
    pub const ID_TX_EMPTY: u8 = 0x02; // Transmitter empty
    #[allow(dead_code)]
    pub const ID_MODEM_STATUS: u8 = 0x00; // Modem status change
}

/// FCR bits
mod fcr {
    pub const FIFO_ENABLE: u8 = 0x01; // Enable FIFO
    pub const RX_FIFO_RESET: u8 = 0x02; // Clear receive FIFO
    pub const TX_FIFO_RESET: u8 = 0x04; // Clear transmit FIFO
    #[allow(dead_code)]
    pub const DMA_MODE: u8 = 0x08; // DMA mode select

    // Trigger levels (bits 7-6)
    #[allow(dead_code)]
    pub const TRIGGER_1: u8 = 0x00; // 1 byte
    #[allow(dead_code)]
    pub const TRIGGER_4: u8 = 0x40; // 4 bytes
    #[allow(dead_code)]
    pub const TRIGGER_8: u8 = 0x80; // 8 bytes
    #[allow(dead_code)]
    pub const TRIGGER_14: u8 = 0xC0; // 14 bytes
}

/// LCR bits
#[allow(dead_code)]
mod lcr {
    pub const WORD_LENGTH_5: u8 = 0x00; // 5 bits
    pub const WORD_LENGTH_6: u8 = 0x01; // 6 bits
    pub const WORD_LENGTH_7: u8 = 0x02; // 7 bits
    pub const WORD_LENGTH_8: u8 = 0x03; // 8 bits
    pub const STOP_BITS: u8 = 0x04; // 2 stop bits
    pub const PARITY_ENABLE: u8 = 0x08; // Enable parity
    pub const EVEN_PARITY: u8 = 0x10; // Even parity
    pub const STICK_PARITY: u8 = 0x20; // Stick parity
    pub const BREAK_CONTROL: u8 = 0x40; // Break control
    pub const DLAB: u8 = 0x80; // Divisor Latch Access Bit
}

/// LSR bits
#[allow(dead_code)]
mod lsr {
    pub const DATA_READY: u8 = 0x01; // Data ready in RBR
    pub const OVERRUN_ERROR: u8 = 0x02; // Overrun error
    pub const PARITY_ERROR: u8 = 0x04; // Parity error
    pub const FRAMING_ERROR: u8 = 0x08; // Framing error
    pub const BREAK_INDICATOR: u8 = 0x10; // Break indicator
    pub const TX_EMPTY: u8 = 0x20; // Transmitter holding register empty
    pub const TX_IDLE: u8 = 0x40; // Transmitter empty (both THR and shift reg)
    pub const FIFO_ERROR: u8 = 0x80; // Error in receive FIFO
}

/// MCR bits
#[allow(dead_code)]
mod mcr {
    pub const DTR: u8 = 0x01; // Data Terminal Ready
    pub const RTS: u8 = 0x02; // Request To Send
    pub const OUT1: u8 = 0x04; // Output 1
    pub const OUT2: u8 = 0x08; // Output 2 (enables interrupts on PC)
    pub const LOOPBACK: u8 = 0x10; // Loopback mode
}

/// MSR bits
#[allow(dead_code)]
mod msr {
    pub const DELTA_CTS: u8 = 0x01; // CTS changed
    pub const DELTA_DSR: u8 = 0x02; // DSR changed
    pub const RING_INDICATOR: u8 = 0x04; // RI trailing edge
    pub const DELTA_DCD: u8 = 0x08; // DCD changed
    pub const CTS: u8 = 0x10; // Clear To Send
    pub const DSR: u8 = 0x20; // Data Set Ready
    pub const RI: u8 = 0x40; // Ring Indicator
    pub const DCD: u8 = 0x80; // Data Carrier Detect
}

// ============================================================================
// Constants
// ============================================================================

/// UART base address (QEMU virt machine)
pub const UART_BASE: u32 = 0x1000_0000;
/// UART address space size
pub const UART_SIZE: usize = 0x100;
/// UART IRQ number (PLIC source)
pub const UART_IRQ: usize = 10;
/// FIFO size
const FIFO_SIZE: usize = 16;

// ============================================================================
// Output Callback Type
// ============================================================================

/// Type for output callback function
pub type OutputCallback = Box<dyn FnMut(u8) + Send + Sync>;

// ============================================================================
// Uart Structure
// ============================================================================

/// NS16550A compatible UART
pub struct Uart {
    /// Base address
    base: Addr,
    /// Divisor Latch Low (when DLAB=1)
    dll: u8,
    /// Divisor Latch High (when DLAB=1)
    dlm: u8,
    /// Interrupt Enable Register
    ier: u8,
    /// Interrupt Identification Register
    iir: u8,
    /// FIFO Control Register
    fcr: u8,
    /// Line Control Register
    lcr: u8,
    /// Modem Control Register
    mcr: u8,
    /// Line Status Register
    lsr: u8,
    /// Modem Status Register
    msr: u8,
    /// Scratch Register
    scr: u8,
    /// Receive FIFO
    rx_fifo: VecDeque<u8>,
    /// Transmit FIFO
    tx_fifo: VecDeque<u8>,
    /// Output callback for transmitted bytes
    output_cb: Option<OutputCallback>,
}

impl Default for Uart {
    fn default() -> Self {
        Self::new()
    }
}

impl Uart {
    /// Create a new UART instance
    pub fn new() -> Self {
        Self {
            base: Addr::new(UART_BASE),
            dll: 0x00,
            dlm: 0x00,
            ier: 0x00,
            iir: iir::NO_INTERRUPT, // No interrupt pending
            fcr: 0x00,
            lcr: 0x00,
            mcr: 0x00,
            lsr: lsr::TX_EMPTY | lsr::TX_IDLE, // Transmitter ready
            msr: msr::CTS | msr::DSR | msr::DCD, // Modem signals ready
            scr: 0x00,
            rx_fifo: VecDeque::with_capacity(FIFO_SIZE),
            tx_fifo: VecDeque::with_capacity(FIFO_SIZE),
            output_cb: None,
        }
    }

    /// Create a new UART with custom base address
    pub fn with_base(base: Addr) -> Self {
        let mut uart = Self::new();
        uart.base = base;
        uart
    }

    /// Set output callback for transmitted bytes
    pub fn set_output_callback(&mut self, cb: OutputCallback) {
        self.output_cb = Some(cb);
    }

    /// Push a byte into the receive FIFO (for external input)
    pub fn receive_byte(&mut self, byte: u8) {
        if self.rx_fifo.len() < FIFO_SIZE {
            self.rx_fifo.push_back(byte);
            self.lsr |= lsr::DATA_READY;
            self.update_iir();
        } else {
            // FIFO overflow - set overrun error
            self.lsr |= lsr::OVERRUN_ERROR;
        }
    }

    /// Check if DLAB (Divisor Latch Access Bit) is set
    fn dlab(&self) -> bool {
        (self.lcr & lcr::DLAB) != 0
    }

    /// Update IIR based on current state
    fn update_iir(&mut self) {
        let rx_pending = !self.rx_fifo.is_empty() && (self.ier & ier::RX_ENABLE) != 0;
        let tx_pending = self.tx_fifo.len() < FIFO_SIZE && (self.ier & ier::TX_ENABLE) != 0;

        if rx_pending {
            self.iir = iir::ID_RX_READY; // Receive data available
        } else if tx_pending {
            self.iir = iir::ID_TX_EMPTY; // Transmitter empty
        } else {
            self.iir = iir::NO_INTERRUPT; // No interrupt pending
        }

        // Set FIFO enabled flag if FIFO is enabled
        if (self.fcr & fcr::FIFO_ENABLE) != 0 {
            self.iir |= iir::FIFO_ENABLE;
        }
    }

    /// Check if there's a pending interrupt
    pub fn has_interrupt(&self) -> bool {
        (self.iir & iir::NO_INTERRUPT) == 0
    }

    /// Get the IRQ number
    pub fn irq(&self) -> usize {
        UART_IRQ
    }

    /// Read from a register
    fn read_register(&mut self, offset: u32) -> u8 {
        match offset {
            RBR_THR if self.dlab() => self.dll,
            IER if self.dlab() => self.dlm,
            RBR_THR => {
                // Read from RBR (receive FIFO)
                if let Some(byte) = self.rx_fifo.pop_front() {
                    if self.rx_fifo.is_empty() {
                        self.lsr &= !lsr::DATA_READY;
                    }
                    self.update_iir();
                    byte
                } else {
                    0
                }
            }
            IER => self.ier,
            IIR_FCR => {
                // IIR is read-only
                let iir = self.iir;
                // Reading IIR doesn't clear it, but acknowledges the interrupt
                iir
            }
            LCR => self.lcr,
            MCR => self.mcr,
            LSR => {
                // Reading LSR clears some error bits
                let lsr = self.lsr;
                self.lsr &= !(lsr::OVERRUN_ERROR | lsr::PARITY_ERROR | lsr::FRAMING_ERROR | lsr::BREAK_INDICATOR | lsr::FIFO_ERROR);
                lsr
            }
            MSR => {
                // Reading MSR clears delta bits
                let msr = self.msr;
                self.msr &= !(msr::DELTA_CTS | msr::DELTA_DSR | msr::RING_INDICATOR | msr::DELTA_DCD);
                msr
            }
            SCR => self.scr,
            _ => 0,
        }
    }

    /// Write to a register
    fn write_register(&mut self, offset: u32, value: u8) {
        match offset {
            RBR_THR if self.dlab() => {
                self.dll = value;
            }
            IER if self.dlab() => {
                self.dlm = value;
            }
            RBR_THR => {
                // Write to THR (transmit)
                if self.tx_fifo.len() < FIFO_SIZE {
                    self.tx_fifo.push_back(value);
                }
                // Immediately output the byte
                if let Some(ref mut cb) = self.output_cb {
                    cb(value);
                }
                // Update LSR - THR is empty after write (simplified)
                self.lsr |= lsr::TX_EMPTY | lsr::TX_IDLE;
                self.update_iir();
            }
            IER => {
                self.ier = value & 0x0F; // Only lower 4 bits are valid
                self.update_iir();
            }
            IIR_FCR => {
                // Write to FCR
                self.fcr = value;
                // Clear FIFOs if requested
                if (value & fcr::RX_FIFO_RESET) != 0 {
                    self.rx_fifo.clear();
                    self.lsr &= !lsr::DATA_READY;
                }
                if (value & fcr::TX_FIFO_RESET) != 0 {
                    self.tx_fifo.clear();
                }
            }
            LCR => {
                self.lcr = value;
            }
            MCR => {
                self.mcr = value & 0x1F; // Only lower 5 bits are valid
            }
            LSR => {
                // LSR is read-only
            }
            MSR => {
                // MSR is read-only
            }
            SCR => {
                self.scr = value;
            }
            _ => {}
        }
    }
}

impl Memory for Uart {
    fn read_byte(&self, addr: Addr) -> Result<Byte> {
        let offset = addr.raw().saturating_sub(self.base.raw());
        if offset >= UART_SIZE as u32 {
            return Err(SimError::InvalidAddress(addr));
        }
        // Need mutable access for side effects
        let mut uart = Self {
            base: self.base,
            dll: self.dll,
            dlm: self.dlm,
            ier: self.ier,
            iir: self.iir,
            fcr: self.fcr,
            lcr: self.lcr,
            mcr: self.mcr,
            lsr: self.lsr,
            msr: self.msr,
            scr: self.scr,
            rx_fifo: self.rx_fifo.clone(),
            tx_fifo: self.tx_fifo.clone(),
            output_cb: None,
        };
        Ok(Byte::new(uart.read_register(offset)))
    }

    fn read_half(&self, addr: Addr) -> Result<Half> {
        let offset = addr.raw().saturating_sub(self.base.raw());
        if offset % 2 != 0 {
            return Err(SimError::MemoryAlignment {
                addr,
                size: 2,
                alignment: 2,
            });
        }
        let byte0 = self.read_byte(addr)?;
        let byte1 = self.read_byte(Addr::new(addr.raw() + 1))?;
        Ok(Half::new(byte0.raw() as u16 | ((byte1.raw() as u16) << 8)))
    }

    fn read_word(&self, addr: Addr) -> Result<Word> {
        let offset = addr.raw().saturating_sub(self.base.raw());
        if offset % 4 != 0 {
            return Err(SimError::MemoryAlignment {
                addr,
                size: 4,
                alignment: 4,
            });
        }
        let byte0 = self.read_byte(addr)?;
        let byte1 = self.read_byte(Addr::new(addr.raw() + 1))?;
        let byte2 = self.read_byte(Addr::new(addr.raw() + 2))?;
        let byte3 = self.read_byte(Addr::new(addr.raw() + 3))?;
        Ok(Word::new(
            byte0.raw() as u32
                | ((byte1.raw() as u32) << 8)
                | ((byte2.raw() as u32) << 16)
                | ((byte3.raw() as u32) << 24),
        ))
    }

    fn write_byte(&mut self, addr: Addr, value: Byte) -> Result<()> {
        let offset = addr.raw().saturating_sub(self.base.raw());
        if offset >= UART_SIZE as u32 {
            return Err(SimError::InvalidAddress(addr));
        }
        self.write_register(offset, value.raw());
        Ok(())
    }

    fn write_half(&mut self, addr: Addr, value: Half) -> Result<()> {
        let offset = addr.raw().saturating_sub(self.base.raw());
        if offset % 2 != 0 {
            return Err(SimError::MemoryAlignment {
                addr,
                size: 2,
                alignment: 2,
            });
        }
        let raw = value.raw();
        self.write_byte(addr, Byte::new(raw as u8))?;
        self.write_byte(Addr::new(addr.raw() + 1), Byte::new((raw >> 8) as u8))?;
        Ok(())
    }

    fn write_word(&mut self, addr: Addr, value: Word) -> Result<()> {
        let offset = addr.raw().saturating_sub(self.base.raw());
        if offset % 4 != 0 {
            return Err(SimError::MemoryAlignment {
                addr,
                size: 4,
                alignment: 4,
            });
        }
        let raw = value.raw();
        self.write_byte(addr, Byte::new(raw as u8))?;
        self.write_byte(Addr::new(addr.raw() + 1), Byte::new((raw >> 8) as u8))?;
        self.write_byte(Addr::new(addr.raw() + 2), Byte::new((raw >> 16) as u8))?;
        self.write_byte(Addr::new(addr.raw() + 3), Byte::new((raw >> 24) as u8))?;
        Ok(())
    }

    fn size(&self) -> usize {
        UART_SIZE
    }

    fn contains(&self, addr: Addr) -> bool {
        let base = self.base.raw();
        let end = base.saturating_add(UART_SIZE as u32);
        addr.raw() >= base && addr.raw() < end
    }
}

impl Peripheral for Uart {
    fn read(&self, offset: Addr) -> Result<u8> {
        self.read_byte(Addr::new(self.base.raw() + offset.raw()))
            .map(|b| b.raw())
    }

    fn write(&mut self, offset: Addr, value: u8) -> Result<()> {
        self.write_byte(Addr::new(self.base.raw() + offset.raw()), Byte::new(value))
    }

    fn base_addr(&self) -> Addr {
        self.base
    }

    fn size(&self) -> usize {
        UART_SIZE
    }

    fn name(&self) -> &str {
        "UART"
    }

    fn has_interrupt(&self) -> bool {
        self.has_interrupt()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uart_new() {
        let uart = Uart::new();
        assert_eq!(uart.base, Addr::new(UART_BASE));
        // Use Memory trait's contains
        assert!(Memory::contains(&uart, Addr::new(UART_BASE)));
        assert!(!Memory::contains(&uart, Addr::new(UART_BASE + UART_SIZE as u32)));
    }

    #[test]
    fn test_uart_transmit() {
        let mut uart = Uart::new();
        let mut output: Vec<u8> = Vec::new();
        uart.set_output_callback(Box::new(move |b| {
            output.push(b);
        }));

        // Write a byte to THR
        uart.write_byte(Addr::new(UART_BASE), Byte::new(b'H')).unwrap();

        // Check LSR indicates transmitter empty
        let lsr = uart.read_byte(Addr::new(UART_BASE + LSR)).unwrap();
        assert!(lsr.raw() & lsr::TX_EMPTY != 0);
    }

    #[test]
    fn test_uart_receive() {
        let mut uart = Uart::new();

        // Initially no data
        let lsr = uart.read_byte(Addr::new(UART_BASE + LSR)).unwrap();
        assert!(lsr.raw() & lsr::DATA_READY == 0);

        // Push a byte into receive FIFO
        uart.receive_byte(b'X');

        // Now data should be ready
        let lsr = uart.read_byte(Addr::new(UART_BASE + LSR)).unwrap();
        assert!(lsr.raw() & lsr::DATA_READY != 0);

        // Read the byte
        let byte = uart.read_byte(Addr::new(UART_BASE + RBR_THR)).unwrap();
        assert_eq!(byte.raw(), b'X');
    }

    #[test]
    fn test_uart_dlab() {
        let mut uart = Uart::new();

        // Set DLAB bit
        uart.write_byte(Addr::new(UART_BASE + LCR), Byte::new(lcr::DLAB)).unwrap();

        // Write to DLL (divisor latch low)
        uart.write_byte(Addr::new(UART_BASE), Byte::new(0x0C)).unwrap();

        // Clear DLAB
        uart.write_byte(Addr::new(UART_BASE + LCR), Byte::new(0x00)).unwrap();

        // Read back LCR
        let lcr = uart.read_byte(Addr::new(UART_BASE + LCR)).unwrap();
        assert_eq!(lcr.raw(), 0);
    }

    #[test]
    fn test_uart_interrupt() {
        let mut uart = Uart::new();

        // No interrupt initially
        assert!(!uart.has_interrupt());

        // Enable RX interrupt
        uart.write_byte(Addr::new(UART_BASE + IER), Byte::new(ier::RX_ENABLE)).unwrap();

        // Still no interrupt (no data)
        assert!(!uart.has_interrupt());

        // Push data
        uart.receive_byte(b'A');

        // Now interrupt should be pending
        assert!(uart.has_interrupt());

        // Check IIR
        let iir = uart.read_byte(Addr::new(UART_BASE + IIR_FCR)).unwrap();
        assert_eq!(iir.raw() & iir::ID_MASK, iir::ID_RX_READY);
    }

    #[test]
    fn test_uart_fifo_reset() {
        let mut uart = Uart::new();

        // Add some data
        uart.receive_byte(b'A');
        uart.receive_byte(b'B');

        // Check data ready
        let lsr = uart.read_byte(Addr::new(UART_BASE + LSR)).unwrap();
        assert!(lsr.raw() & lsr::DATA_READY != 0);

        // Reset RX FIFO
        uart.write_byte(Addr::new(UART_BASE + IIR_FCR), Byte::new(fcr::RX_FIFO_RESET)).unwrap();

        // Data should be cleared
        let lsr = uart.read_byte(Addr::new(UART_BASE + LSR)).unwrap();
        assert!(lsr.raw() & lsr::DATA_READY == 0);
    }

    #[test]
    fn test_uart_scratch() {
        let mut uart = Uart::new();

        // Write to scratch register
        uart.write_byte(Addr::new(UART_BASE + SCR), Byte::new(0x42)).unwrap();

        // Read back
        let scr = uart.read_byte(Addr::new(UART_BASE + SCR)).unwrap();
        assert_eq!(scr.raw(), 0x42);
    }
}
