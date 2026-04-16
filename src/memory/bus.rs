//! System Bus implementation.
//!
//! This module provides the system bus that connects memory and peripherals
//! and routes memory accesses to the appropriate components.

use crate::error::{check_alignment, Result, SimError};
use crate::peripheral::{
    Gpu, GpuSnapshot, InputDevice, Lpu, LpuSnapshot, Npu, NpuSnapshot, Tpu, TpuSnapshot, Uart,
    GPU_BASE, LPU_BASE, NPU_BASE, TPU_BASE,
};
use crate::traits::{Memory, Peripheral};
use crate::types::{Addr, Byte, Half, Word};
use std::fmt;

const ACC_CTRL_ROOT_BASE: u32 = 0x2000_0000;
const ACC_DOORBELL_BASE: u32 = 0x2000_1000;
const ACC_REGION_SIZE: u32 = 0x1000;

const ACC_ENGINE_NPU: u32 = 0;
const ACC_ENGINE_LPU: u32 = 1;
const ACC_ENGINE_GPU: u32 = 2;
const ACC_ENGINE_TPU: u32 = 3;
const ACC_ENGINE_MASK: u32 = (1 << ACC_ENGINE_NPU)
    | (1 << ACC_ENGINE_LPU)
    | (1 << ACC_ENGINE_GPU)
    | (1 << ACC_ENGINE_TPU);

const ACC_MODE_V2_OVERLAY_ENABLE: u32 = 1 << 0;

mod acc_root_regs {
    pub const VERSION: u32 = 0x100;
    pub const ENGINE_MASK: u32 = 0x104;
    pub const IRQ_SUMMARY: u32 = 0x108;
    pub const DOORBELL_SUBMITS_LOW: u32 = 0x10C;
    pub const DOORBELL_SUBMITS_HIGH: u32 = 0x110;
    pub const DOORBELL_COMPLETES_LOW: u32 = 0x114;
    pub const DOORBELL_COMPLETES_HIGH: u32 = 0x118;
    pub const MODE: u32 = 0x11C;
    pub const DOORBELL_ERRORS_LOW: u32 = 0x120;
    pub const DOORBELL_ERRORS_HIGH: u32 = 0x124;
}

mod acc_doorbell_regs {
    pub const CONTROL: u32 = 0x100;
    pub const STATUS: u32 = 0x104;
    pub const ENGINE: u32 = 0x108;
    pub const DESC_ADDR_LOW: u32 = 0x10C;
    pub const DESC_ADDR_HIGH: u32 = 0x110;
    pub const DESC_LEN: u32 = 0x114;
    pub const NOTIFY: u32 = 0x118;
    pub const SUBMIT_COUNT_LOW: u32 = 0x11C;
    pub const SUBMIT_COUNT_HIGH: u32 = 0x120;
    pub const ERROR_COUNT_LOW: u32 = 0x124;
    pub const ERROR_COUNT_HIGH: u32 = 0x128;
}

mod acc_doorbell_status_bits {
    pub const BUSY: u32 = 1 << 0;
    pub const DONE: u32 = 1 << 1;
    pub const ERROR: u32 = 1 << 2;
}

#[derive(Debug, Clone, Copy)]
struct AccRootState {
    version: u32,
    mode: u32,
    doorbell_submits: u64,
    doorbell_completes: u64,
    doorbell_errors: u64,
}

impl Default for AccRootState {
    fn default() -> Self {
        Self {
            version: 0x0002_0000,
            mode: ACC_MODE_V2_OVERLAY_ENABLE,
            doorbell_submits: 0,
            doorbell_completes: 0,
            doorbell_errors: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct AccDoorbellState {
    control: u32,
    status: u32,
    engine: u32,
    desc_addr: u64,
    desc_len: u32,
    submit_count: u64,
    error_count: u64,
}

/// A memory region mapping in the bus.
#[derive(Debug)]
struct MemoryRegion {
    base: Addr,
    size: usize,
    name: String,
}

impl MemoryRegion {
    #[allow(dead_code)]
    fn contains(&self, addr: Addr) -> bool {
        let base = self.base.raw() as usize;
        let target = addr.raw() as usize;
        target >= base && target < base + self.size
    }
}

/// System Bus.
///
/// The system bus connects memory and peripheral devices and routes
/// memory accesses to the appropriate component based on address ranges.
///
/// # Memory Map
/// The bus maintains a memory map that assigns address ranges to specific
/// components. When an access occurs, the bus finds the appropriate component
/// and forwards the request.
pub struct Bus {
    /// RAM regions
    ram_regions: Vec<(Addr, usize, Box<dyn Memory>)>,
    /// Peripheral regions
    peripheral_regions: Vec<(Addr, usize, Box<dyn Peripheral>)>,
    /// Memory map for debugging
    memory_map: Vec<MemoryRegion>,
    /// Unified accelerator control root register state.
    acc_root: AccRootState,
    /// Unified accelerator doorbell register state.
    acc_doorbell: AccDoorbellState,
}

impl fmt::Debug for Bus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bus")
            .field("ram_regions", &self.ram_regions.len())
            .field("peripheral_regions", &self.peripheral_regions.len())
            .field("memory_map", &self.memory_map)
            .field("acc_root", &self.acc_root)
            .field("acc_doorbell", &self.acc_doorbell)
            .finish()
    }
}

impl Bus {
    const VIRTIO_IRQ_SOURCE: usize = 1;
    const UART_IRQ_SOURCE: usize = 10;
    const NPU_IRQ_SOURCE: usize = 11;
    const LPU_IRQ_SOURCE: usize = 12;
    const GPU_IRQ_SOURCE: usize = 13;
    const TPU_IRQ_SOURCE: usize = 14;

    /// Create a new empty system bus.
    pub fn new() -> Self {
        Self {
            ram_regions: Vec::new(),
            peripheral_regions: Vec::new(),
            memory_map: Vec::new(),
            acc_root: AccRootState::default(),
            acc_doorbell: AccDoorbellState::default(),
        }
    }

    fn reg_u8(value: u32, off: u32) -> u8 {
        ((value >> ((off & 0x3) * 8)) & 0xFF) as u8
    }

    fn set_reg_u8(value: &mut u32, off: u32, byte: u8) {
        let shift = (off & 0x3) * 8;
        *value &= !(0xFF << shift);
        *value |= (byte as u32) << shift;
    }

    fn build_acc_irq_summary(&self) -> u32 {
        let mut summary = 0u32;
        if self.has_peripheral_interrupt("NPU") {
            summary |= 1 << ACC_ENGINE_NPU;
        }
        if self.has_peripheral_interrupt("LPU") {
            summary |= 1 << ACC_ENGINE_LPU;
        }
        if self.has_peripheral_interrupt("GPU") {
            summary |= 1 << ACC_ENGINE_GPU;
        }
        if self.has_peripheral_interrupt("TPU") {
            summary |= 1 << ACC_ENGINE_TPU;
        }
        summary
    }

    fn read_acc_root_u32(&self, reg: u32) -> u32 {
        match reg {
            acc_root_regs::VERSION => self.acc_root.version,
            acc_root_regs::ENGINE_MASK => ACC_ENGINE_MASK,
            acc_root_regs::IRQ_SUMMARY => self.build_acc_irq_summary(),
            acc_root_regs::DOORBELL_SUBMITS_LOW => self.acc_root.doorbell_submits as u32,
            acc_root_regs::DOORBELL_SUBMITS_HIGH => (self.acc_root.doorbell_submits >> 32) as u32,
            acc_root_regs::DOORBELL_COMPLETES_LOW => self.acc_root.doorbell_completes as u32,
            acc_root_regs::DOORBELL_COMPLETES_HIGH => {
                (self.acc_root.doorbell_completes >> 32) as u32
            }
            acc_root_regs::MODE => self.acc_root.mode,
            acc_root_regs::DOORBELL_ERRORS_LOW => self.acc_root.doorbell_errors as u32,
            acc_root_regs::DOORBELL_ERRORS_HIGH => (self.acc_root.doorbell_errors >> 32) as u32,
            _ => 0,
        }
    }

    fn write_acc_root_u32(&mut self, reg: u32, value: u32) {
        if reg == acc_root_regs::MODE {
            let _ = value;
            self.acc_root.mode = ACC_MODE_V2_OVERLAY_ENABLE;
        }
    }

    fn read_acc_doorbell_u32(&self, reg: u32) -> u32 {
        match reg {
            acc_doorbell_regs::CONTROL => self.acc_doorbell.control,
            acc_doorbell_regs::STATUS => self.acc_doorbell.status,
            acc_doorbell_regs::ENGINE => self.acc_doorbell.engine,
            acc_doorbell_regs::DESC_ADDR_LOW => self.acc_doorbell.desc_addr as u32,
            acc_doorbell_regs::DESC_ADDR_HIGH => (self.acc_doorbell.desc_addr >> 32) as u32,
            acc_doorbell_regs::DESC_LEN => self.acc_doorbell.desc_len,
            acc_doorbell_regs::SUBMIT_COUNT_LOW => self.acc_doorbell.submit_count as u32,
            acc_doorbell_regs::SUBMIT_COUNT_HIGH => (self.acc_doorbell.submit_count >> 32) as u32,
            acc_doorbell_regs::ERROR_COUNT_LOW => self.acc_doorbell.error_count as u32,
            acc_doorbell_regs::ERROR_COUNT_HIGH => (self.acc_doorbell.error_count >> 32) as u32,
            _ => 0,
        }
    }

    fn read_engine_task_counters(&self, engine: u32) -> Result<(u32, u32)> {
        let (base, done_off, err_off) = match engine {
            ACC_ENGINE_NPU => (NPU_BASE, 0x30, 0x34),
            ACC_ENGINE_LPU => (LPU_BASE, 0x30, 0x34),
            ACC_ENGINE_GPU => (GPU_BASE, 0x84, 0x88),
            ACC_ENGINE_TPU => (TPU_BASE, 0x94, 0x98),
            _ => {
                return Err(SimError::Peripheral(format!(
                    "Invalid accelerator engine id: {}",
                    engine
                )));
            }
        };

        let done = self.read_word(Addr::new(base + done_off))?.raw();
        let err = self.read_word(Addr::new(base + err_off))?.raw();
        Ok((done, err))
    }

    fn submit_acc_doorbell(&mut self) -> Result<()> {
        let engine = self.acc_doorbell.engine;
        let desc_addr = self.acc_doorbell.desc_addr;
        let desc_len = self.acc_doorbell.desc_len;

        self.acc_doorbell.status = acc_doorbell_status_bits::BUSY;
        self.acc_doorbell.submit_count = self.acc_doorbell.submit_count.wrapping_add(1);
        self.acc_root.doorbell_submits = self.acc_root.doorbell_submits.wrapping_add(1);

        if engine > ACC_ENGINE_TPU {
            self.acc_doorbell.status = acc_doorbell_status_bits::ERROR;
            self.acc_doorbell.error_count = self.acc_doorbell.error_count.wrapping_add(1);
            self.acc_root.doorbell_errors = self.acc_root.doorbell_errors.wrapping_add(1);
            return Err(SimError::Peripheral(format!(
                "Invalid accelerator engine id: {}",
                engine
            )));
        }

        let before = self.read_engine_task_counters(engine)?;

        match engine {
            ACC_ENGINE_NPU | ACC_ENGINE_LPU => {
                let base = if engine == ACC_ENGINE_NPU {
                    NPU_BASE
                } else {
                    LPU_BASE
                };
                self.write_word(Addr::new(base + 0x20), Word::new(desc_addr as u32))?;
                self.write_word(Addr::new(base + 0x24), Word::new((desc_addr >> 32) as u32))?;
                self.write_word(Addr::new(base + 0x28), Word::new(desc_len))?;
                self.write_word(Addr::new(base + 0x2C), Word::new(1))?;
            }
            ACC_ENGINE_GPU => {
                self.write_word(Addr::new(GPU_BASE + 0x50), Word::new(desc_addr as u32))?;
                self.write_word(Addr::new(GPU_BASE + 0x54), Word::new((desc_addr >> 32) as u32))?;
                self.write_word(Addr::new(GPU_BASE + 0x58), Word::new(desc_len))?;
                self.write_word(Addr::new(GPU_BASE + 0x5C), Word::new(1))?;
            }
            ACC_ENGINE_TPU => {
                self.write_word(Addr::new(TPU_BASE + 0x80), Word::new(desc_addr as u32))?;
                self.write_word(Addr::new(TPU_BASE + 0x84), Word::new((desc_addr >> 32) as u32))?;
                self.write_word(Addr::new(TPU_BASE + 0x88), Word::new(desc_len))?;
                self.write_word(Addr::new(TPU_BASE + 0x8C), Word::new(1))?;
            }
            _ => unreachable!("invalid engine id is guarded above"),
        }

        let after = self.read_engine_task_counters(engine)?;
        let done_delta = after.0.wrapping_sub(before.0) as u64;
        let err_delta = after.1.wrapping_sub(before.1) as u64;

        self.acc_root.doorbell_completes = self.acc_root.doorbell_completes.wrapping_add(done_delta);
        self.acc_root.doorbell_errors = self.acc_root.doorbell_errors.wrapping_add(err_delta);
        self.acc_doorbell.error_count = self.acc_doorbell.error_count.wrapping_add(err_delta);

        self.acc_doorbell.status = if err_delta > 0 {
            acc_doorbell_status_bits::DONE | acc_doorbell_status_bits::ERROR
        } else {
            acc_doorbell_status_bits::DONE
        };

        Ok(())
    }

    fn write_acc_doorbell_u32(&mut self, reg: u32, value: u32) -> Result<()> {
        match reg {
            acc_doorbell_regs::CONTROL => {
                self.acc_doorbell.control = value;
                Ok(())
            }
            acc_doorbell_regs::STATUS => {
                if (value & acc_doorbell_status_bits::DONE) != 0 {
                    self.acc_doorbell.status &= !acc_doorbell_status_bits::DONE;
                }
                if (value & acc_doorbell_status_bits::ERROR) != 0 {
                    self.acc_doorbell.status &= !acc_doorbell_status_bits::ERROR;
                }
                Ok(())
            }
            acc_doorbell_regs::ENGINE => {
                self.acc_doorbell.engine = value;
                Ok(())
            }
            acc_doorbell_regs::DESC_ADDR_LOW => {
                self.acc_doorbell.desc_addr =
                    (self.acc_doorbell.desc_addr & 0xFFFF_FFFF_0000_0000) | value as u64;
                Ok(())
            }
            acc_doorbell_regs::DESC_ADDR_HIGH => {
                self.acc_doorbell.desc_addr =
                    (self.acc_doorbell.desc_addr & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
                Ok(())
            }
            acc_doorbell_regs::DESC_LEN => {
                self.acc_doorbell.desc_len = value;
                Ok(())
            }
            acc_doorbell_regs::NOTIFY => {
                if value != 0 {
                    self.submit_acc_doorbell()?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn try_read_acc_mmio(&self, addr: Addr) -> Option<Result<Byte>> {
        let raw = addr.raw();

        if (ACC_CTRL_ROOT_BASE..(ACC_CTRL_ROOT_BASE + ACC_REGION_SIZE)).contains(&raw) {
            let off = raw - ACC_CTRL_ROOT_BASE;
            let reg = off & !0x3;
            let val = self.read_acc_root_u32(reg);
            return Some(Ok(Byte::new(Self::reg_u8(val, off))));
        }

        if (ACC_DOORBELL_BASE..(ACC_DOORBELL_BASE + ACC_REGION_SIZE)).contains(&raw) {
            let off = raw - ACC_DOORBELL_BASE;
            let reg = off & !0x3;
            let val = self.read_acc_doorbell_u32(reg);
            return Some(Ok(Byte::new(Self::reg_u8(val, off))));
        }

        None
    }

    fn try_write_acc_mmio(&mut self, addr: Addr, value: Byte) -> Option<Result<()>> {
        let raw = addr.raw();

        if (ACC_CTRL_ROOT_BASE..(ACC_CTRL_ROOT_BASE + ACC_REGION_SIZE)).contains(&raw) {
            let off = raw - ACC_CTRL_ROOT_BASE;
            let reg = off & !0x3;
            let mut current = self.read_acc_root_u32(reg);
            Self::set_reg_u8(&mut current, off, value.raw());
            self.write_acc_root_u32(reg, current);
            return Some(Ok(()));
        }

        if (ACC_DOORBELL_BASE..(ACC_DOORBELL_BASE + ACC_REGION_SIZE)).contains(&raw) {
            let off = raw - ACC_DOORBELL_BASE;
            let reg = off & !0x3;
            let mut current = self.read_acc_doorbell_u32(reg);
            Self::set_reg_u8(&mut current, off, value.raw());
            return Some(self.write_acc_doorbell_u32(reg, current));
        }

        None
    }

    /// Attach a memory region to the bus.
    ///
    /// # Arguments
    /// * `base` - The base address for this memory region
    /// * `memory` - The memory implementation
    /// * `name` - A name for this region (for debugging)
    pub fn attach_memory<M: Memory + 'static>(&mut self, base: Addr, memory: M, name: &str) {
        let size = memory.size();
        self.memory_map.push(MemoryRegion {
            base,
            size,
            name: name.to_string(),
        });
        self.ram_regions.push((base, size, Box::new(memory)));
    }

    /// Attach a peripheral to the bus.
    ///
    /// # Arguments
    /// * `peripheral` - The peripheral implementation
    pub fn attach_peripheral<P: Peripheral + 'static>(&mut self, peripheral: P) {
        let base = peripheral.base_addr();
        let size = peripheral.size();
        let name = peripheral.name().to_string();
        self.memory_map.push(MemoryRegion { base, size, name });
        self.peripheral_regions
            .push((base, size, Box::new(peripheral)));
    }

    /// Read a byte from the bus.
    ///
    /// Routes the request to the appropriate memory or peripheral region.
    pub fn read_byte(&self, addr: Addr) -> Result<Byte> {
        if let Some(result) = self.try_read_acc_mmio(addr) {
            return result;
        }

        // Check peripheral regions first
        for (base, size, peripheral) in &self.peripheral_regions {
            let base_addr = base.raw() as usize;
            let target = addr.raw() as usize;
            if target >= base_addr && target < base_addr + *size {
                let offset = target - base_addr;
                return peripheral.read(Addr::new(offset as u32)).map(Byte::new);
            }
        }

        // Then check memory regions
        for (base, size, memory) in &self.ram_regions {
            let base_addr = base.raw() as usize;
            let target = addr.raw() as usize;
            if target >= base_addr && target < base_addr + *size {
                // Pass address relative to the memory's base
                let relative_addr = Addr::new((target - base_addr) as u32);
                return memory.read_byte(relative_addr);
            }
        }

        Err(SimError::MemoryOutOfBounds {
            addr,
            size: 1,
        })
    }

    /// Write a byte to the bus.
    ///
    /// Routes the request to the appropriate memory or peripheral region.
    pub fn write_byte(&mut self, addr: Addr, value: Byte) -> Result<()> {
        if let Some(result) = self.try_write_acc_mmio(addr, value) {
            return result;
        }

        // Check peripheral regions first
        for (base, size, peripheral) in &mut self.peripheral_regions {
            let base_addr = base.raw() as usize;
            let target = addr.raw() as usize;
            if target >= base_addr && target < base_addr + *size {
                let offset = target - base_addr;
                peripheral.write(Addr::new(offset as u32), value.raw())?;

                // Dispatch pending work via the virtual method on Peripheral.
                // Each accelerator overrides try_execute_pending to handle
                // start-bit / descriptor-notify triggers without the Bus
                // needing to know concrete types.
                peripheral.try_execute_pending(&mut self.ram_regions)?;

                return Ok(());
            }
        }

        // Then check memory regions
        for (base, size, memory) in &mut self.ram_regions {
            let base_addr = base.raw() as usize;
            let target = addr.raw() as usize;
            if target >= base_addr && target < base_addr + *size {
                // Pass address relative to the memory's base
                let relative_addr = Addr::new((target - base_addr) as u32);
                return memory.write_byte(relative_addr, value);
            }
        }

        Err(SimError::MemoryOutOfBounds {
            addr,
            size: 1,
        })
    }

    /// Read a half-word from the bus.
    ///
    /// # Errors
    /// Returns `SimError::MemoryAlignment` if the address is not 2-byte aligned.
    pub fn read_half(&self, addr: Addr) -> Result<Half> {
        check_alignment(addr, 2)?;
        let b0 = self.read_byte(addr)?.raw();
        let b1 = self.read_byte(addr.add(1))?.raw();
        Ok(Half::new(((b1 as u16) << 8) | (b0 as u16)))
    }

    /// Write a half-word to the bus.
    ///
    /// # Errors
    /// Returns `SimError::MemoryAlignment` if the address is not 2-byte aligned.
    pub fn write_half(&mut self, addr: Addr, value: Half) -> Result<()> {
        check_alignment(addr, 2)?;
        let raw = value.raw();
        self.write_byte(addr, Byte::new(raw as u8))?;
        self.write_byte(addr.add(1), Byte::new((raw >> 8) as u8))
    }

    /// Read a word from the bus.
    ///
    /// # Errors
    /// Returns `SimError::MemoryAlignment` if the address is not 4-byte aligned.
    pub fn read_word(&self, addr: Addr) -> Result<Word> {
        check_alignment(addr, 4)?;
        let b0 = self.read_byte(addr)?.raw() as u32;
        let b1 = self.read_byte(addr.add(1))?.raw() as u32;
        let b2 = self.read_byte(addr.add(2))?.raw() as u32;
        let b3 = self.read_byte(addr.add(3))?.raw() as u32;
        Ok(Word::new(b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)))
    }

    /// Write a word to the bus.
    ///
    /// # Errors
    /// Returns `SimError::MemoryAlignment` if the address is not 4-byte aligned.
    pub fn write_word(&mut self, addr: Addr, value: Word) -> Result<()> {
        check_alignment(addr, 4)?;
        let raw = value.raw();
        self.write_byte(addr, Byte::new(raw as u8))?;
        self.write_byte(addr.add(1), Byte::new((raw >> 8) as u8))?;
        self.write_byte(addr.add(2), Byte::new((raw >> 16) as u8))?;
        self.write_byte(addr.add(3), Byte::new((raw >> 24) as u8))
    }

    /// Read bytes from the bus.
    pub fn read_bytes(&self, addr: Addr, buf: &mut [u8]) -> Result<usize> {
        for (i, byte) in buf.iter_mut().enumerate() {
            *byte = self.read_byte(addr.add(i as u32))?.raw();
        }
        Ok(buf.len())
    }

    /// Write bytes to the bus.
    pub fn write_bytes(&mut self, addr: Addr, data: &[u8]) -> Result<usize> {
        for (i, &byte) in data.iter().enumerate() {
            self.write_byte(addr.add(i as u32), Byte::new(byte))?;
        }
        Ok(data.len())
    }

    /// Print the memory map for debugging.
    pub fn print_memory_map(&self) {
        println!("Memory Map:");
        for region in &self.memory_map {
            println!(
                "  {:08x}-{:08x}: {}",
                region.base.raw(),
                region.base.raw() as usize + region.size - 1,
                region.name
            );
        }
    }

    /// Check if any peripheral has a pending interrupt.
    pub fn has_pending_interrupt(&self) -> bool {
        self.peripheral_regions
            .iter()
            .any(|(_, _, p)| p.has_interrupt())
    }

    /// Check if a specific peripheral has a pending interrupt.
    ///
    /// # Arguments
    /// * `name` - The name of the peripheral to check
    ///
    /// # Returns
    /// `true` if the peripheral exists and has a pending interrupt.
    pub fn has_peripheral_interrupt(&self, name: &str) -> bool {
        self.peripheral_regions
            .iter()
            .any(|(_, _, p)| p.name() == name && p.has_interrupt())
    }

    /// Inject one byte into UART receive FIFO.
    ///
    /// Returns `true` if a UART peripheral exists and byte is injected.
    pub fn inject_uart_byte(&mut self, byte: u8) -> bool {
        for (_, _, peripheral) in &mut self.peripheral_regions {
            if peripheral.name() != "UART" {
                continue;
            }

            if let Some(uart) = peripheral.as_any_mut().downcast_mut::<Uart>() {
                uart.receive_byte(byte);
                return true;
            }
        }

        false
    }

    /// Inject one key event into Input peripheral.
    ///
    /// Returns `true` if Input peripheral exists.
    pub fn inject_input_key(&mut self, key_code: u8, pressed: bool) -> bool {
        for (_, _, peripheral) in &mut self.peripheral_regions {
            if peripheral.name() != "Input" {
                continue;
            }

            if let Some(input) = peripheral.as_any_mut().downcast_mut::<InputDevice>() {
                input.set_key(key_code, pressed);
                return true;
            }
        }

        false
    }

    /// Clear all input key states.
    ///
    /// Returns `true` if Input peripheral exists.
    pub fn clear_input_keys(&mut self) -> bool {
        for (_, _, peripheral) in &mut self.peripheral_regions {
            if peripheral.name() != "Input" {
                continue;
            }

            if let Some(input) = peripheral.as_any_mut().downcast_mut::<InputDevice>() {
                input.clear_keys();
                return true;
            }
        }

        false
    }

    /// Get input snapshot if Input peripheral is attached.
    ///
    /// Returns `(key_state, last_event, event_count, irq_pending)`.
    pub fn get_input_snapshot(&self) -> Option<(u32, u32, u32, bool)> {
        for (_, _, peripheral) in &self.peripheral_regions {
            if peripheral.name() != "Input" {
                continue;
            }

            if let Some(input) = peripheral.as_any().downcast_ref::<InputDevice>() {
                return Some(input.snapshot());
            }
        }

        None
    }

    /// Get NPU snapshot if NPU peripheral is attached.
    pub fn get_npu_snapshot(&self) -> Option<NpuSnapshot> {
        for (_, _, peripheral) in &self.peripheral_regions {
            if peripheral.name() != "NPU" {
                continue;
            }

            if let Some(npu) = peripheral.as_any().downcast_ref::<Npu>() {
                return Some(npu.snapshot());
            }
        }

        None
    }

    /// Get LPU snapshot if LPU peripheral is attached.
    pub fn get_lpu_snapshot(&self) -> Option<LpuSnapshot> {
        for (_, _, peripheral) in &self.peripheral_regions {
            if peripheral.name() != "LPU" {
                continue;
            }

            if let Some(lpu) = peripheral.as_any().downcast_ref::<Lpu>() {
                return Some(lpu.snapshot());
            }
        }

        None
    }

    /// Get GPU snapshot if GPU peripheral is attached.
    pub fn get_gpu_snapshot(&self) -> Option<GpuSnapshot> {
        for (_, _, peripheral) in &self.peripheral_regions {
            if peripheral.name() != "GPU" {
                continue;
            }

            if let Some(gpu) = peripheral.as_any().downcast_ref::<Gpu>() {
                return Some(gpu.snapshot());
            }
        }

        None
    }

    /// Get TPU snapshot if TPU peripheral is attached.
    pub fn get_tpu_snapshot(&self) -> Option<TpuSnapshot> {
        for (_, _, peripheral) in &self.peripheral_regions {
            if peripheral.name() != "TPU" {
                continue;
            }

            if let Some(tpu) = peripheral.as_any().downcast_ref::<Tpu>() {
                return Some(tpu.snapshot());
            }
        }

        None
    }

    /// Get timer and software interrupt status from CLINT.
    ///
    /// Returns (mtip, msip) where:
    /// - mtip: Machine Timer Interrupt Pending (mtime >= mtimecmp)
    /// - msip: Machine Software Interrupt Pending
    ///
    /// Returns (false, false) if CLINT is not found.
    pub fn get_clint_interrupt_status(&self) -> (bool, bool) {
        use crate::interrupt::Clint;

        // Look for CLINT peripheral and get its interrupt status
        for (_, _, peripheral) in &self.peripheral_regions {
            if peripheral.name() == "CLINT" {
                // Downcast to concrete Clint type to access detailed interrupt status
                let any = peripheral.as_any();
                if let Some(clint) = any.downcast_ref::<Clint>() {
                    return (clint.mtip(), clint.msip());
                }
            }
        }
        (false, false)
    }

    /// Advance CLINT timer by a given number of cycles.
    ///
    /// This updates `mtime` for any attached CLINT peripheral so MTIP can
    /// become pending when `mtime >= mtimecmp`.
    pub fn tick_clint(&mut self, cycles: u64) {
        use crate::interrupt::Clint;

        for (_, _, peripheral) in &mut self.peripheral_regions {
            if peripheral.name() == "CLINT" {
                let any = peripheral.as_any_mut();
                if let Some(clint) = any.downcast_mut::<Clint>() {
                    clint.tick(cycles);
                    return;
                }
            }
        }
    }

    /// Get external interrupt status from PLIC.
    ///
    /// Returns (meip, seip) where:
    /// - meip: Machine External Interrupt Pending
    /// - seip: Supervisor External Interrupt Pending
    ///
    /// Returns (false, false) if PLIC is not found.
    pub fn get_plic_interrupt_status(&self) -> (bool, bool) {
        use crate::interrupt::Plic;

        // Look for PLIC peripheral and get its interrupt status
        for (_, _, peripheral) in &self.peripheral_regions {
            if peripheral.name() == "PLIC" {
                let any = peripheral.as_any();
                if let Some(plic) = any.downcast_ref::<Plic>() {
                    // For now, we only support one hart.
                    let meip = plic.has_pending_interrupt_machine(0);
                    let seip = plic.has_pending_interrupt_supervisor(0);
                    return (meip, seip);
                }
            }
        }
        (false, false)
    }

    /// Reflect peripheral IRQ lines into PLIC pending bits.
    ///
    /// Current source mapping follows xv6-rv32 memlayout:
    /// - VirtIO block -> source 1
    /// - UART        -> source 10
    /// - NPU         -> source 11
    /// - LPU         -> source 12
    /// - GPU         -> source 13
    /// - TPU         -> source 14
    pub fn sync_plic_pending_from_peripherals(&mut self) {
        let mut virtio_pending = false;
        let mut uart_pending = false;
        let mut npu_pending = false;
        let mut lpu_pending = false;
        let mut gpu_pending = false;
        let mut tpu_pending = false;

        for (_, _, peripheral) in &self.peripheral_regions {
            if peripheral.name() == "VirtIO-Block" && peripheral.has_interrupt() {
                virtio_pending = true;
            }
            if peripheral.name() == "UART" && peripheral.has_interrupt() {
                uart_pending = true;
            }
            if peripheral.name() == "NPU" && peripheral.has_interrupt() {
                npu_pending = true;
            }
            if peripheral.name() == "LPU" && peripheral.has_interrupt() {
                lpu_pending = true;
            }
            if peripheral.name() == "GPU" && peripheral.has_interrupt() {
                gpu_pending = true;
            }
            if peripheral.name() == "TPU" && peripheral.has_interrupt() {
                tpu_pending = true;
            }
        }

        if !virtio_pending
            && !uart_pending
            && !npu_pending
            && !lpu_pending
            && !gpu_pending
            && !tpu_pending
        {
            return;
        }

        use crate::interrupt::Plic;
        for (_, _, peripheral) in &mut self.peripheral_regions {
            if peripheral.name() == "PLIC" {
                let any = peripheral.as_any_mut();
                if let Some(plic) = any.downcast_mut::<Plic>() {
                    if virtio_pending {
                        plic.set_pending(Self::VIRTIO_IRQ_SOURCE);
                    }
                    if uart_pending {
                        plic.set_pending(Self::UART_IRQ_SOURCE);
                    }
                    if npu_pending {
                        plic.set_pending(Self::NPU_IRQ_SOURCE);
                    }
                    if lpu_pending {
                        plic.set_pending(Self::LPU_IRQ_SOURCE);
                    }
                    if gpu_pending {
                        plic.set_pending(Self::GPU_IRQ_SOURCE);
                    }
                    if tpu_pending {
                        plic.set_pending(Self::TPU_IRQ_SOURCE);
                    }
                }
                break;
            }
        }

        // xv6-rv32 compatibility: treat VirtIO IRQ line as edge-latched into PLIC.
        // After latching source 1 pending in PLIC, auto-lower the device line so
        // unacked level state does not continuously reassert pending every cycle.
        if virtio_pending {
            for (_, _, peripheral) in &mut self.peripheral_regions {
                if peripheral.name() == "VirtIO-Block" {
                    peripheral.acknowledge_interrupt();
                    break;
                }
            }
        }
    }

    /// Claim the highest priority external interrupt from PLIC.
    ///
    /// Returns the interrupt source ID, or 0 if none.
    pub fn claim_plic_interrupt(&mut self) -> u32 {
        use crate::interrupt::Plic;

        // Look for PLIC peripheral and claim interrupt
        for (_, _, peripheral) in &mut self.peripheral_regions {
            if peripheral.name() == "PLIC" {
                let any = peripheral.as_any_mut();
                if let Some(plic) = any.downcast_mut::<Plic>() {
                    return plic.claim(0);
                }
            }
        }
        0
    }

    /// Complete (finish processing) an external interrupt in PLIC.
    pub fn complete_plic_interrupt(&mut self, source: u32) {
        use crate::interrupt::Plic;

        // Look for PLIC peripheral and complete interrupt
        for (_, _, peripheral) in &mut self.peripheral_regions {
            if peripheral.name() == "PLIC" {
                let any = peripheral.as_any_mut();
                if let Some(plic) = any.downcast_mut::<Plic>() {
                    plic.complete(0, source);
                    return;
                }
            }
        }
    }

    /// Get VirtIO activity counters if VirtIO-Block is attached.
    ///
    /// Returns `(command_exec_count, queue_notify_count, descriptor_notify_count, irq_raised_count, irq_ack_count, descriptor_not_ready_count, descriptor_no_avail_count, descriptor_success_count, descriptor_error_count)`.
    pub fn get_virtio_activity_counters(
        &self,
    ) -> Option<(u64, u64, u64, u64, u64, u64, u64, u64, u64)> {
        use crate::peripheral::VirtioBlock;

        for (_, _, peripheral) in &self.peripheral_regions {
            if peripheral.name() == "VirtIO-Block" {
                let any = peripheral.as_any();
                if let Some(virtio) = any.downcast_ref::<VirtioBlock>() {
                    return Some((
                        virtio.command_exec_count(),
                        virtio.queue_notify_count(),
                        virtio.descriptor_notify_count(),
                        virtio.irq_raised_count(),
                        virtio.irq_ack_count(),
                        virtio.descriptor_not_ready_count(),
                        virtio.descriptor_no_avail_count(),
                        virtio.descriptor_success_count(),
                        virtio.descriptor_error_count(),
                    ));
                }
            }
        }

        None
    }

    /// Acknowledge interrupts from all peripherals.
    pub fn acknowledge_interrupts(&mut self) {
        for (_, _, peripheral) in &mut self.peripheral_regions {
            peripheral.acknowledge_interrupt();
        }
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Ram;
    use crate::peripheral::{
        Lpu, Npu, VirtioBlock, LPU_BASE, LPU_OPCODE_BYTE_TOKENIZE, LPU_OPCODE_EMBEDDING_BAG,
        LPU_OPCODE_GREEDY_DECODE, LPU_OPCODE_TOPK_SAMPLE_DECODE, LPU_OPCODE_TOPP_SAMPLE_DECODE,
        NPU_BASE, VIRTIO_BLK_BASE,
    };

    #[test]
    fn test_bus_basic() {
        let mut bus = Bus::new();
        let ram = Ram::new(1024);
        bus.attach_memory(Addr::new(0), ram, "RAM");

        bus.write_byte(Addr::new(0), Byte::new(0x42)).unwrap();
        assert_eq!(bus.read_byte(Addr::new(0)).unwrap().raw(), 0x42);
    }

    #[test]
    fn test_bus_multiple_regions() {
        let mut bus = Bus::new();
        let ram1 = Ram::new(1024);
        let ram2 = Ram::new(512);
        bus.attach_memory(Addr::new(0x0000), ram1, "RAM1");
        bus.attach_memory(Addr::new(0x1000), ram2, "RAM2");

        // Write to first region
        bus.write_byte(Addr::new(0x0100), Byte::new(0x11)).unwrap();
        assert_eq!(bus.read_byte(Addr::new(0x0100)).unwrap().raw(), 0x11);

        // Write to second region
        bus.write_byte(Addr::new(0x1100), Byte::new(0x22)).unwrap();
        assert_eq!(bus.read_byte(Addr::new(0x1100)).unwrap().raw(), 0x22);

        // Gap between regions should fail
        assert!(bus.read_byte(Addr::new(0x0800)).is_err());
    }

    #[test]
    fn test_bus_word_operations() {
        let mut bus = Bus::new();
        let ram = Ram::new(1024);
        bus.attach_memory(Addr::new(0), ram, "RAM");

        bus.write_word(Addr::new(0), Word::new(0xDEADBEEF)).unwrap();
        assert_eq!(bus.read_word(Addr::new(0)).unwrap().raw(), 0xDEADBEEF);
    }

    fn write_u16(bus: &mut Bus, addr: u32, value: u16) {
        bus.write_byte(Addr::new(addr), Byte::new((value & 0xFF) as u8))
            .unwrap();
        bus.write_byte(Addr::new(addr + 1), Byte::new((value >> 8) as u8))
            .unwrap();
    }

    fn read_u16(bus: &Bus, addr: u32) -> u16 {
        let b0 = bus.read_byte(Addr::new(addr)).unwrap().raw() as u16;
        let b1 = bus.read_byte(Addr::new(addr + 1)).unwrap().raw() as u16;
        b0 | (b1 << 8)
    }

    fn write_u32(bus: &mut Bus, addr: u32, value: u32) {
        bus.write_word(Addr::new(addr), Word::new(value)).unwrap();
    }

    fn write_u64(bus: &mut Bus, addr: u32, value: u64) {
        write_u32(bus, addr, value as u32);
        write_u32(bus, addr + 4, (value >> 32) as u32);
    }

    fn read_u32(bus: &Bus, addr: u32) -> u32 {
        bus.read_word(Addr::new(addr)).unwrap().raw()
    }

    #[test]
    fn test_bus_virtio_descriptor_notify_bridge() {
        const RAM_BASE: u32 = 0x8000_0000;
        const RAM_SIZE: usize = 0x20_000;
        const SECTOR_SIZE: u32 = 512;

        let mut bus = Bus::new();
        bus.attach_memory(Addr::new(RAM_BASE), Ram::new(RAM_SIZE), "RAM");

        let mut virtio = VirtioBlock::with_disk_sectors(16);
        virtio.preload_sector(3, &[0xCA, 0xFE, 0xBA, 0xBE]).unwrap();
        bus.attach_peripheral(virtio);

        let desc = RAM_BASE + 0x1000;
        let avail = RAM_BASE + 0x2000;
        let used = RAM_BASE + 0x3000;
        let req = RAM_BASE + 0x4000;
        let data = RAM_BASE + 0x5000;
        let status = RAM_BASE + 0x6000;

        write_u64(&mut bus, desc, req as u64);
        write_u32(&mut bus, desc + 8, 16);
        write_u16(&mut bus, desc + 12, 1);
        write_u16(&mut bus, desc + 14, 1);

        write_u64(&mut bus, desc + 16, data as u64);
        write_u32(&mut bus, desc + 24, SECTOR_SIZE);
        write_u16(&mut bus, desc + 28, 1);
        write_u16(&mut bus, desc + 30, 2);

        write_u64(&mut bus, desc + 32, status as u64);
        write_u32(&mut bus, desc + 40, 1);
        write_u16(&mut bus, desc + 44, 0);
        write_u16(&mut bus, desc + 46, 0);

        write_u32(&mut bus, req, 0);
        write_u64(&mut bus, req + 8, 3);

        write_u16(&mut bus, avail + 2, 1);
        write_u16(&mut bus, avail + 4, 0);

        write_u32(&mut bus, VIRTIO_BLK_BASE + 0x110, 1);
        write_u32(&mut bus, VIRTIO_BLK_BASE + 0x0F4, 8);
        write_u32(&mut bus, VIRTIO_BLK_BASE + 0x0F8, 1);
        write_u32(&mut bus, VIRTIO_BLK_BASE + 0x080, desc);
        write_u32(&mut bus, VIRTIO_BLK_BASE + 0x084, 0);
        write_u32(&mut bus, VIRTIO_BLK_BASE + 0x090, avail);
        write_u32(&mut bus, VIRTIO_BLK_BASE + 0x094, 0);
        write_u32(&mut bus, VIRTIO_BLK_BASE + 0x0A0, used);
        write_u32(&mut bus, VIRTIO_BLK_BASE + 0x0A4, 0);

        write_u32(&mut bus, VIRTIO_BLK_BASE + 0x0FC, 0);

        assert_eq!(bus.read_byte(Addr::new(data)).unwrap().raw(), 0xCA);
        assert_eq!(bus.read_byte(Addr::new(data + 1)).unwrap().raw(), 0xFE);
        assert_eq!(bus.read_byte(Addr::new(data + 2)).unwrap().raw(), 0xBA);
        assert_eq!(bus.read_byte(Addr::new(data + 3)).unwrap().raw(), 0xBE);
        assert_eq!(bus.read_byte(Addr::new(status)).unwrap().raw(), 0);

        assert_eq!(read_u16(&bus, used + 2), 1);
    }

    #[test]
    fn test_bus_virtio_descriptor_notify_bridge_out_then_in() {
        const RAM_BASE: u32 = 0x8000_0000;
        const RAM_SIZE: usize = 0x20_000;
        const SECTOR_SIZE: u32 = 512;

        let mut bus = Bus::new();
        bus.attach_memory(Addr::new(RAM_BASE), Ram::new(RAM_SIZE), "RAM");

        let virtio = VirtioBlock::with_disk_sectors(16);
        bus.attach_peripheral(virtio);

        let desc = RAM_BASE + 0x1000;
        let avail = RAM_BASE + 0x2000;
        let used = RAM_BASE + 0x3000;
        let req = RAM_BASE + 0x4000;
        let data = RAM_BASE + 0x5000;
        let status = RAM_BASE + 0x6000;

        write_u64(&mut bus, desc, req as u64);
        write_u32(&mut bus, desc + 8, 16);
        write_u16(&mut bus, desc + 12, 1);
        write_u16(&mut bus, desc + 14, 1);

        write_u64(&mut bus, desc + 16, data as u64);
        write_u32(&mut bus, desc + 24, SECTOR_SIZE);
        write_u16(&mut bus, desc + 28, 1);
        write_u16(&mut bus, desc + 30, 2);

        write_u64(&mut bus, desc + 32, status as u64);
        write_u32(&mut bus, desc + 40, 1);
        write_u16(&mut bus, desc + 44, 0);
        write_u16(&mut bus, desc + 46, 0);

        write_u32(&mut bus, VIRTIO_BLK_BASE + 0x0F4, 8);
        write_u32(&mut bus, VIRTIO_BLK_BASE + 0x0F8, 1);
        write_u32(&mut bus, VIRTIO_BLK_BASE + 0x080, desc);
        write_u32(&mut bus, VIRTIO_BLK_BASE + 0x084, 0);
        write_u32(&mut bus, VIRTIO_BLK_BASE + 0x090, avail);
        write_u32(&mut bus, VIRTIO_BLK_BASE + 0x094, 0);
        write_u32(&mut bus, VIRTIO_BLK_BASE + 0x0A0, used);
        write_u32(&mut bus, VIRTIO_BLK_BASE + 0x0A4, 0);

        bus.write_byte(Addr::new(data), Byte::new(0x11)).unwrap();
        bus.write_byte(Addr::new(data + 1), Byte::new(0x22))
            .unwrap();
        bus.write_byte(Addr::new(data + 2), Byte::new(0x33))
            .unwrap();
        bus.write_byte(Addr::new(data + 3), Byte::new(0x44))
            .unwrap();

        write_u32(&mut bus, req, 1);
        write_u64(&mut bus, req + 8, 6);
        write_u16(&mut bus, avail + 2, 1);
        write_u16(&mut bus, avail + 4, 0);
        write_u32(&mut bus, VIRTIO_BLK_BASE + 0x0FC, 0);

        assert_eq!(bus.read_byte(Addr::new(status)).unwrap().raw(), 0);
        assert_eq!(read_u16(&bus, used + 2), 1);

        bus.write_byte(Addr::new(data), Byte::new(0x00)).unwrap();
        bus.write_byte(Addr::new(data + 1), Byte::new(0x00))
            .unwrap();
        bus.write_byte(Addr::new(data + 2), Byte::new(0x00))
            .unwrap();
        bus.write_byte(Addr::new(data + 3), Byte::new(0x00))
            .unwrap();

        write_u32(&mut bus, req, 0);
        write_u64(&mut bus, req + 8, 6);
        write_u16(&mut bus, avail + 2, 2);
        write_u16(&mut bus, avail + 6, 0);
        write_u32(&mut bus, VIRTIO_BLK_BASE + 0x0FC, 0);

        assert_eq!(bus.read_byte(Addr::new(data)).unwrap().raw(), 0x11);
        assert_eq!(bus.read_byte(Addr::new(data + 1)).unwrap().raw(), 0x22);
        assert_eq!(bus.read_byte(Addr::new(data + 2)).unwrap().raw(), 0x33);
        assert_eq!(bus.read_byte(Addr::new(data + 3)).unwrap().raw(), 0x44);
        assert_eq!(bus.read_byte(Addr::new(status)).unwrap().raw(), 0);
        assert_eq!(read_u16(&bus, used + 2), 2);
        assert_eq!(read_u32(&bus, used + 4), 0);
        assert_eq!(read_u32(&bus, used + 16), SECTOR_SIZE);
    }

    #[test]
    fn test_bus_input_injection_and_clear() {
        use crate::peripheral::InputDevice;

        let mut bus = Bus::new();
        bus.attach_peripheral(InputDevice::new());

        assert!(bus.inject_input_key(0, true));
        let (state, _last, count, _irq) = bus.get_input_snapshot().unwrap();
        assert_eq!(state & 1, 1);
        assert_eq!(count, 1);

        assert!(bus.clear_input_keys());
        let (state_after, _last_after, count_after, _irq_after) = bus.get_input_snapshot().unwrap();
        assert_eq!(state_after, 0);
        assert_eq!(count_after, 1);
    }

    #[test]
    fn test_bus_npu_descriptor_notify_bridge() {
        const RAM_BASE: u32 = 0x8000_0000;

        let mut bus = Bus::new();
        bus.attach_memory(Addr::new(RAM_BASE), Ram::new(0x8000), "RAM");
        bus.attach_peripheral(Npu::new());

        let desc_addr = RAM_BASE + 0x1000;
        let op_a = RAM_BASE + 0x2000;
        let op_b = RAM_BASE + 0x2004;
        let out = RAM_BASE + 0x2008;

        write_u32(&mut bus, desc_addr, 0); // Add
        write_u32(&mut bus, desc_addr + 4, op_a);
        write_u32(&mut bus, desc_addr + 8, op_b);
        write_u32(&mut bus, desc_addr + 12, out);

        write_u32(&mut bus, op_a, 11);
        write_u32(&mut bus, op_b, 31);

        write_u32(&mut bus, NPU_BASE + 0x20, desc_addr);
        write_u32(&mut bus, NPU_BASE + 0x28, 1);
        write_u32(&mut bus, NPU_BASE + 0x00, 0x2); // IRQ_EN
        write_u32(&mut bus, NPU_BASE + 0x2C, 1); // DESC_NOTIFY

        assert_eq!(read_u32(&bus, out), 42);
        assert!(bus.has_peripheral_interrupt("NPU"));
    }

    #[test]
    fn test_bus_lpu_invalid_descriptor_opcode_counts_error() {
        const RAM_BASE: u32 = 0x8000_0000;

        let mut bus = Bus::new();
        bus.attach_memory(Addr::new(RAM_BASE), Ram::new(0x8000), "RAM");
        bus.attach_peripheral(Lpu::new());

        let desc_addr = RAM_BASE + 0x1100;
        let out = RAM_BASE + 0x2108;

        write_u32(&mut bus, desc_addr, 2); // removed legacy opcode
        write_u32(&mut bus, desc_addr + 4, 0);
        write_u32(&mut bus, desc_addr + 8, 0);
        write_u32(&mut bus, desc_addr + 12, out);

        write_u32(&mut bus, LPU_BASE + 0x20, desc_addr);
        write_u32(&mut bus, LPU_BASE + 0x28, 1);
        write_u32(&mut bus, LPU_BASE + 0x00, 0x2); // IRQ_EN
        write_u32(&mut bus, LPU_BASE + 0x2C, 1); // DESC_NOTIFY

        assert_eq!(read_u32(&bus, out), 0);
        assert_eq!(read_u32(&bus, LPU_BASE + 0x34), 1);
        assert!(bus.has_peripheral_interrupt("LPU"));
    }

    #[test]
    fn test_bus_lpu_byte_tokenize_descriptor_notify_bridge() {
        const RAM_BASE: u32 = 0x8000_0000;

        let mut bus = Bus::new();
        bus.attach_memory(Addr::new(RAM_BASE), Ram::new(0x8000), "RAM");
        bus.attach_peripheral(Lpu::new());

        let desc_addr = RAM_BASE + 0x1200;
        let input_addr = RAM_BASE + 0x2200;
        let out_addr = RAM_BASE + 0x2300;

        // descriptor: [opcode, input_addr, input_len, out_addr]
        write_u32(&mut bus, desc_addr, LPU_OPCODE_BYTE_TOKENIZE);
        write_u32(&mut bus, desc_addr + 4, input_addr);
        write_u32(&mut bus, desc_addr + 8, 4);
        write_u32(&mut bus, desc_addr + 12, out_addr);

        bus.write_byte(Addr::new(input_addr), Byte::new(b'A')).unwrap();
        bus.write_byte(Addr::new(input_addr + 1), Byte::new(b'1'))
            .unwrap();
        bus.write_byte(Addr::new(input_addr + 2), Byte::new(b' '))
            .unwrap();
        bus.write_byte(Addr::new(input_addr + 3), Byte::new(b'?'))
            .unwrap();

        write_u32(&mut bus, LPU_BASE + 0x20, desc_addr);
        write_u32(&mut bus, LPU_BASE + 0x28, 1);
        write_u32(&mut bus, LPU_BASE + 0x00, 0x2); // IRQ_EN
        write_u32(&mut bus, LPU_BASE + 0x2C, 1); // DESC_NOTIFY

        assert_eq!(read_u32(&bus, out_addr), 1);
        assert_eq!(read_u32(&bus, out_addr + 4), 2);
        assert_eq!(read_u32(&bus, out_addr + 8), 0);
        assert_eq!(read_u32(&bus, out_addr + 12), 3);
        assert!(bus.has_peripheral_interrupt("LPU"));
    }

    #[test]
    fn test_bus_lpu_embedding_bag_descriptor_notify_bridge() {
        const RAM_BASE: u32 = 0x8000_0000;

        let mut bus = Bus::new();
        bus.attach_memory(Addr::new(RAM_BASE), Ram::new(0x8000), "RAM");
        bus.attach_peripheral(Lpu::new());

        let desc_addr = RAM_BASE + 0x1300;
        let input_addr = RAM_BASE + 0x2400;
        let out_addr = RAM_BASE + 0x2500;

        // descriptor: [opcode, input_addr, bag_len, out_addr]
        write_u32(&mut bus, desc_addr, LPU_OPCODE_EMBEDDING_BAG);
        write_u32(&mut bus, desc_addr + 4, input_addr);
        write_u32(&mut bus, desc_addr + 8, 4);
        write_u32(&mut bus, desc_addr + 12, out_addr);

        // token ids: [1,2,3,1] => embeddings [4,6,3,4] => pooled 17
        write_u32(&mut bus, input_addr, 1);
        write_u32(&mut bus, input_addr + 4, 2);
        write_u32(&mut bus, input_addr + 8, 3);
        write_u32(&mut bus, input_addr + 12, 1);

        write_u32(&mut bus, LPU_BASE + 0x20, desc_addr);
        write_u32(&mut bus, LPU_BASE + 0x28, 1);
        write_u32(&mut bus, LPU_BASE + 0x00, 0x2); // IRQ_EN
        write_u32(&mut bus, LPU_BASE + 0x2C, 1); // DESC_NOTIFY

        assert_eq!(read_u32(&bus, out_addr), 17);
        assert!(bus.has_peripheral_interrupt("LPU"));
    }

    #[test]
    fn test_bus_lpu_greedy_decode_descriptor_notify_bridge() {
        const RAM_BASE: u32 = 0x8000_0000;

        let mut bus = Bus::new();
        bus.attach_memory(Addr::new(RAM_BASE), Ram::new(0x8000), "RAM");
        bus.attach_peripheral(Lpu::new());

        let desc_addr = RAM_BASE + 0x1400;
        let logits_addr = RAM_BASE + 0x2600;
        let out_addr = RAM_BASE + 0x2700;

        // descriptor: [opcode, logits_addr, vocab_size, out_addr]
        write_u32(&mut bus, desc_addr, LPU_OPCODE_GREEDY_DECODE);
        write_u32(&mut bus, desc_addr + 4, logits_addr);
        write_u32(&mut bus, desc_addr + 8, 4);
        write_u32(&mut bus, desc_addr + 12, out_addr);

        // scores: [10, 7, 21, 3] => argmax id 2
        write_u32(&mut bus, logits_addr, 10);
        write_u32(&mut bus, logits_addr + 4, 7);
        write_u32(&mut bus, logits_addr + 8, 21);
        write_u32(&mut bus, logits_addr + 12, 3);

        write_u32(&mut bus, LPU_BASE + 0x20, desc_addr);
        write_u32(&mut bus, LPU_BASE + 0x28, 1);
        write_u32(&mut bus, LPU_BASE + 0x00, 0x2); // IRQ_EN
        write_u32(&mut bus, LPU_BASE + 0x2C, 1); // DESC_NOTIFY

        assert_eq!(read_u32(&bus, out_addr), 2);
        assert!(bus.has_peripheral_interrupt("LPU"));
    }

    #[test]
    fn test_bus_lpu_topk_sample_decode_descriptor_notify_bridge() {
        const RAM_BASE: u32 = 0x8000_0000;

        let mut bus = Bus::new();
        bus.attach_memory(Addr::new(RAM_BASE), Ram::new(0x8000), "RAM");
        bus.attach_peripheral(Lpu::new());

        let desc_addr = RAM_BASE + 0x1500;
        let logits_addr = RAM_BASE + 0x2800;
        let out_addr = RAM_BASE + 0x2900;

        // descriptor: [opcode, logits_addr, vocab_size, out_addr]
        write_u32(&mut bus, desc_addr, LPU_OPCODE_TOPK_SAMPLE_DECODE);
        write_u32(&mut bus, desc_addr + 4, logits_addr);
        write_u32(&mut bus, desc_addr + 8, 4);
        write_u32(&mut bus, desc_addr + 12, out_addr);

        // scores: id0=100, id1=90 are top-2 candidates
        write_u32(&mut bus, logits_addr, 100);
        write_u32(&mut bus, logits_addr + 4, 90);
        write_u32(&mut bus, logits_addr + 8, 80);
        write_u32(&mut bus, logits_addr + 12, 10);

        // deterministic sampling config: seed=5, top_k=2, temperature=10000
        write_u32(&mut bus, LPU_BASE + 0x3C, 2);
        write_u32(&mut bus, LPU_BASE + 0x40, 10_000);
        write_u32(&mut bus, LPU_BASE + 0x44, 5);

        write_u32(&mut bus, LPU_BASE + 0x20, desc_addr);
        write_u32(&mut bus, LPU_BASE + 0x28, 1);
        write_u32(&mut bus, LPU_BASE + 0x00, 0x2); // IRQ_EN
        write_u32(&mut bus, LPU_BASE + 0x2C, 1); // DESC_NOTIFY

        assert_eq!(read_u32(&bus, out_addr), 1);
        assert!(bus.has_peripheral_interrupt("LPU"));
    }

    #[test]
    fn test_bus_lpu_topp_sample_decode_descriptor_notify_bridge() {
        const RAM_BASE: u32 = 0x8000_0000;

        let mut bus = Bus::new();
        bus.attach_memory(Addr::new(RAM_BASE), Ram::new(0x8000), "RAM");
        bus.attach_peripheral(Lpu::new());

        let desc_addr = RAM_BASE + 0x1600;
        let logits_addr = RAM_BASE + 0x2A00;
        let out_addr = RAM_BASE + 0x2B00;

        // descriptor: [opcode, logits_addr, vocab_size, out_addr]
        write_u32(&mut bus, desc_addr, LPU_OPCODE_TOPP_SAMPLE_DECODE);
        write_u32(&mut bus, desc_addr + 4, logits_addr);
        write_u32(&mut bus, desc_addr + 8, 4);
        write_u32(&mut bus, desc_addr + 12, out_addr);

        // scores: id0=100, id1=90, id2=80, id3=10
        write_u32(&mut bus, logits_addr, 100);
        write_u32(&mut bus, logits_addr + 4, 90);
        write_u32(&mut bus, logits_addr + 8, 80);
        write_u32(&mut bus, logits_addr + 12, 10);

        // deterministic sampling config: p=0.5, temperature=10000, seed=5
        write_u32(&mut bus, LPU_BASE + 0x4C, 500);
        write_u32(&mut bus, LPU_BASE + 0x40, 10_000);
        write_u32(&mut bus, LPU_BASE + 0x44, 5);

        write_u32(&mut bus, LPU_BASE + 0x20, desc_addr);
        write_u32(&mut bus, LPU_BASE + 0x28, 1);
        write_u32(&mut bus, LPU_BASE + 0x00, 0x2); // IRQ_EN
        write_u32(&mut bus, LPU_BASE + 0x2C, 1); // DESC_NOTIFY

        assert_eq!(read_u32(&bus, out_addr), 0);
        assert!(bus.has_peripheral_interrupt("LPU"));
    }

    #[test]
    fn test_bus_acc_pure_v2_window_for_npu() {
        const V2_NPU_BASE: u32 = 0x2001_0000;

        let mut bus = Bus::new();
        bus.attach_memory(Addr::new(0x8000_0000), Ram::new(0x4000), "RAM");
        bus.attach_peripheral(Npu::new());

        // V2 window is always enabled in pure topology.
        write_u32(&mut bus, V2_NPU_BASE + 0x08, 11);
        write_u32(&mut bus, V2_NPU_BASE + 0x0C, 31);
        write_u32(&mut bus, V2_NPU_BASE + 0x14, 0); // Add
        write_u32(&mut bus, V2_NPU_BASE + 0x00, 1); // START

        assert_eq!(read_u32(&bus, V2_NPU_BASE + 0x10), 42);
        assert_eq!(read_u32(&bus, NPU_BASE + 0x10), 42);

        // Legacy V1 base no longer aliases NPU registers.
        write_u32(&mut bus, ACC_CTRL_ROOT_BASE + 0x08, 99);
        assert_eq!(read_u32(&bus, NPU_BASE + 0x08), 11);

        // MODE register is fixed to V2 enabled.
        write_u32(&mut bus, ACC_CTRL_ROOT_BASE + acc_root_regs::MODE, 0);
        assert_eq!(
            read_u32(&bus, ACC_CTRL_ROOT_BASE + acc_root_regs::MODE),
            ACC_MODE_V2_OVERLAY_ENABLE
        );
    }

    #[test]
    fn test_bus_acc_doorbell_submit_npu_descriptor() {
        const RAM_BASE: u32 = 0x8000_0000;

        let mut bus = Bus::new();
        bus.attach_memory(Addr::new(RAM_BASE), Ram::new(0x8000), "RAM");
        bus.attach_peripheral(Npu::new());

        let desc_addr = RAM_BASE + 0x1800;
        let op_a = RAM_BASE + 0x1900;
        let op_b = RAM_BASE + 0x1904;
        let out = RAM_BASE + 0x1908;

        write_u32(&mut bus, desc_addr, 0); // Add
        write_u32(&mut bus, desc_addr + 4, op_a);
        write_u32(&mut bus, desc_addr + 8, op_b);
        write_u32(&mut bus, desc_addr + 12, out);
        write_u32(&mut bus, op_a, 7);
        write_u32(&mut bus, op_b, 35);

        write_u32(
            &mut bus,
            ACC_DOORBELL_BASE + acc_doorbell_regs::ENGINE,
            ACC_ENGINE_NPU,
        );
        write_u32(
            &mut bus,
            ACC_DOORBELL_BASE + acc_doorbell_regs::DESC_ADDR_LOW,
            desc_addr,
        );
        write_u32(
            &mut bus,
            ACC_DOORBELL_BASE + acc_doorbell_regs::DESC_ADDR_HIGH,
            0,
        );
        write_u32(
            &mut bus,
            ACC_DOORBELL_BASE + acc_doorbell_regs::DESC_LEN,
            1,
        );
        write_u32(
            &mut bus,
            ACC_DOORBELL_BASE + acc_doorbell_regs::NOTIFY,
            1,
        );

        assert_eq!(read_u32(&bus, out), 42);
        assert_eq!(
            read_u32(&bus, ACC_CTRL_ROOT_BASE + acc_root_regs::DOORBELL_SUBMITS_LOW),
            1
        );
        assert_eq!(
            read_u32(&bus, ACC_CTRL_ROOT_BASE + acc_root_regs::DOORBELL_COMPLETES_LOW),
            1
        );
        assert_eq!(
            read_u32(&bus, ACC_DOORBELL_BASE + acc_doorbell_regs::STATUS)
                & acc_doorbell_status_bits::DONE,
            acc_doorbell_status_bits::DONE
        );
    }

    #[test]
    fn test_bus_acc_doorbell_invalid_engine_records_error() {
        let mut bus = Bus::new();
        bus.attach_memory(Addr::new(0x8000_0000), Ram::new(0x4000), "RAM");
        bus.attach_peripheral(Npu::new());

        write_u32(&mut bus, ACC_DOORBELL_BASE + acc_doorbell_regs::ENGINE, 99);
        write_u32(&mut bus, ACC_DOORBELL_BASE + acc_doorbell_regs::DESC_ADDR_LOW, 0);
        write_u32(&mut bus, ACC_DOORBELL_BASE + acc_doorbell_regs::DESC_LEN, 0);

        let err = bus.write_word(
            Addr::new(ACC_DOORBELL_BASE + acc_doorbell_regs::NOTIFY),
            Word::new(1),
        );
        assert!(err.is_err());

        assert_eq!(
            read_u32(&bus, ACC_DOORBELL_BASE + acc_doorbell_regs::ERROR_COUNT_LOW),
            1
        );
        assert_eq!(
            read_u32(&bus, ACC_CTRL_ROOT_BASE + acc_root_regs::DOORBELL_ERRORS_LOW),
            1
        );
        assert_eq!(
            read_u32(&bus, ACC_DOORBELL_BASE + acc_doorbell_regs::STATUS)
                & acc_doorbell_status_bits::ERROR,
            acc_doorbell_status_bits::ERROR
        );
    }
}
