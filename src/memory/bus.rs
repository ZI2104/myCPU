//! System Bus implementation.
//!
//! This module provides the system bus that connects memory and peripherals
//! and routes memory accesses to the appropriate components.

use crate::error::{check_alignment, Result, SimError};
use crate::peripheral::VirtioBlock;
use crate::traits::{Memory, Peripheral};
use crate::types::{Addr, Byte, Half, Word};
use std::fmt;

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
}

impl fmt::Debug for Bus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bus")
            .field("ram_regions", &self.ram_regions.len())
            .field("peripheral_regions", &self.peripheral_regions.len())
            .field("memory_map", &self.memory_map)
            .finish()
    }
}

impl Bus {
    const VIRTIO_IRQ_SOURCE: usize = 1;
    const UART_IRQ_SOURCE: usize = 10;

    /// Create a new empty system bus.
    pub fn new() -> Self {
        Self {
            ram_regions: Vec::new(),
            peripheral_regions: Vec::new(),
            memory_map: Vec::new(),
        }
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

        Err(SimError::MemoryOutOfBounds { addr, size: 1 })
    }

    /// Write a byte to the bus.
    ///
    /// Routes the request to the appropriate memory or peripheral region.
    pub fn write_byte(&mut self, addr: Addr, value: Byte) -> Result<()> {
        // Check peripheral regions first
        for (base, size, peripheral) in &mut self.peripheral_regions {
            let base_addr = base.raw() as usize;
            let target = addr.raw() as usize;
            if target >= base_addr && target < base_addr + *size {
                let offset = target - base_addr;
                peripheral.write(Addr::new(offset as u32), value.raw())?;

                if let Some(virtio_block) = peripheral.as_any_mut().downcast_mut::<VirtioBlock>() {
                    if virtio_block.has_pending_descriptor_notify() {
                        virtio_block.process_pending_descriptor_notify(&mut self.ram_regions)?;
                    }
                }

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

        Err(SimError::MemoryOutOfBounds { addr, size: 1 })
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
    pub fn sync_plic_pending_from_peripherals(&mut self) {
        let mut virtio_pending = false;
        let mut uart_pending = false;

        for (_, _, peripheral) in &self.peripheral_regions {
            if peripheral.name() == "VirtIO-Block" && peripheral.has_interrupt() {
                virtio_pending = true;
            }
            if peripheral.name() == "UART" && peripheral.has_interrupt() {
                uart_pending = true;
            }
        }

        if !virtio_pending && !uart_pending {
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
    use crate::peripheral::{VirtioBlock, VIRTIO_BLK_BASE};

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
}
