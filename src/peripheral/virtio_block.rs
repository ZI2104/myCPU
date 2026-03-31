//! Minimal VirtIO-Block MMIO skeleton.
//!
//! This device is intentionally lightweight and incremental:
//! - Stage 1: identity/status + command/data-window path.
//! - Stage 2: queue metadata and queue-notify request flow (descriptor queue style).
//!
//! It is still a simplified model (no full guest-RAM DMA walker yet), but now
//! provides a queue-driven path that can be used as a stable bring-up stepping
//! stone before full VirtIO descriptor DMA support.

use crate::error::{Result, SimError};
use crate::traits::Memory;
use crate::traits::Peripheral;
use crate::types::Addr;

/// VirtIO-Block MMIO base address (QEMU virt compatible region).
pub const VIRTIO_BLK_BASE: u32 = 0x1000_1000;
/// MMIO region size.
pub const VIRTIO_BLK_SIZE: usize = 0x1000;

/// Sector size used by this skeleton.
pub const SECTOR_SIZE: usize = 512;
const QUEUE_RING_MAX: usize = 8;

const REG_MAGIC: u32 = 0x000;
const REG_VERSION: u32 = 0x004;
const REG_DEVICE_ID: u32 = 0x008;
const REG_VENDOR_ID: u32 = 0x00C;
const REG_STATUS: u32 = 0x070;

const REG_QUEUE_DESC_LOW: u32 = 0x080;
const REG_QUEUE_DESC_HIGH: u32 = 0x084;
const REG_QUEUE_AVAIL_LOW: u32 = 0x090;
const REG_QUEUE_AVAIL_HIGH: u32 = 0x094;
const REG_QUEUE_USED_LOW: u32 = 0x0A0;
const REG_QUEUE_USED_HIGH: u32 = 0x0A4;

const REG_QUEUE_NUM_MAX: u32 = 0x0F0;
const REG_QUEUE_NUM: u32 = 0x0F4;
const REG_QUEUE_READY: u32 = 0x0F8;
const REG_QUEUE_NOTIFY: u32 = 0x0FC;

const REG_SECTOR_LOW: u32 = 0x100;
const REG_SECTOR_HIGH: u32 = 0x104;
const REG_COMMAND: u32 = 0x108;
const REG_RESULT: u32 = 0x10C;
const REG_CONTROL: u32 = 0x110;
const REG_REQ_TYPE: u32 = 0x114;
const REG_REQ_STATUS: u32 = 0x118;
const REG_QUEUE_HEAD: u32 = 0x11C;
const REG_QUEUE_AVAIL_IDX: u32 = 0x120;
const REG_QUEUE_USED_IDX: u32 = 0x124;
const REG_LAST_USED_HEAD: u32 = 0x128;

const REG_QUEUE_USED_ELEM_BASE: u32 = 0x180;
const REG_QUEUE_USED_ELEM_END: u32 = REG_QUEUE_USED_ELEM_BASE + (QUEUE_RING_MAX as u32) * 4;

const DATA_WINDOW_START: u32 = 0x200;
const DATA_WINDOW_END: u32 = DATA_WINDOW_START + SECTOR_SIZE as u32;

const RESULT_OK: u32 = 0;
const RESULT_IO_ERR: u32 = 1;
const RESULT_BAD_CMD: u32 = 2;

const VIRTQ_DESC_F_NEXT: u16 = 1;

const VIRTIO_BLK_S_OK: u8 = 0;
const VIRTIO_BLK_S_IOERR: u8 = 1;

#[derive(Debug, Clone, Copy)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

mod command {
    pub const READ_SECTOR: u32 = 1;
    pub const WRITE_SECTOR: u32 = 2;
}

mod request_type {
    pub const IN: u32 = 0;
    pub const OUT: u32 = 1;
}

mod control_bits {
    pub const IRQ_EN: u32 = 1 << 0;
}

/// Guest memory access bridge used by VirtIO queue descriptor processing.
pub trait VirtioGuestMemory {
    fn read_byte(&mut self, addr: Addr) -> Result<u8>;
    fn write_byte(&mut self, addr: Addr, value: u8) -> Result<()>;
}

impl VirtioGuestMemory for Vec<(Addr, usize, Box<dyn Memory>)> {
    fn read_byte(&mut self, addr: Addr) -> Result<u8> {
        let target = addr.raw() as usize;
        for (base, size, memory) in self.iter_mut() {
            let base_addr = base.raw() as usize;
            if target >= base_addr && target < base_addr + *size {
                let relative = Addr::new((target - base_addr) as u32);
                return memory.read_byte(relative).map(|b| b.raw());
            }
        }

        Err(SimError::MemoryOutOfBounds { addr, size: 1 })
    }

    fn write_byte(&mut self, addr: Addr, value: u8) -> Result<()> {
        let target = addr.raw() as usize;
        for (base, size, memory) in self.iter_mut() {
            let base_addr = base.raw() as usize;
            if target >= base_addr && target < base_addr + *size {
                let relative = Addr::new((target - base_addr) as u32);
                return memory.write_byte(relative, crate::types::Byte::new(value));
            }
        }

        Err(SimError::MemoryOutOfBounds { addr, size: 1 })
    }
}

/// Minimal VirtIO-Block peripheral model.
#[derive(Debug, Clone)]
pub struct VirtioBlock {
    base: Addr,
    status: u32,
    sector: u64,
    command: u32,
    result: u32,
    control: u32,

    queue_desc_addr: u64,
    queue_avail_addr: u64,
    queue_used_addr: u64,
    queue_num: u16,
    queue_ready: bool,
    queue_avail_idx: u16,
    queue_used_idx: u16,
    queue_head: u16,
    last_used_head: u16,
    queue_used_ring: [u16; QUEUE_RING_MAX],
    pending_queue_notify: Option<u32>,

    req_type: u32,
    req_status: u8,

    irq_pending: bool,
    data_window: [u8; SECTOR_SIZE],
    disk: Vec<u8>,
}

impl Default for VirtioBlock {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtioBlock {
    /// Create a device with 1024 sectors (512 KiB).
    pub fn new() -> Self {
        Self::with_disk_sectors(1024)
    }

    pub fn with_disk_sectors(sectors: usize) -> Self {
        let disk_size = sectors.saturating_mul(SECTOR_SIZE);
        Self {
            base: Addr::new(VIRTIO_BLK_BASE),
            status: 0,
            sector: 0,
            command: 0,
            result: RESULT_OK,
            control: 0,

            queue_desc_addr: 0,
            queue_avail_addr: 0,
            queue_used_addr: 0,
            queue_num: QUEUE_RING_MAX as u16,
            queue_ready: false,
            queue_avail_idx: 0,
            queue_used_idx: 0,
            queue_head: 0,
            last_used_head: 0,
            queue_used_ring: [0; QUEUE_RING_MAX],
            pending_queue_notify: None,

            req_type: request_type::IN,
            req_status: 0,

            irq_pending: false,
            data_window: [0; SECTOR_SIZE],
            disk: vec![0; disk_size],
        }
    }

    pub fn preload_sector(&mut self, sector: u64, data: &[u8]) -> Result<()> {
        if data.len() > SECTOR_SIZE {
            return Err(SimError::Peripheral(
                "preload data exceeds sector size".to_string(),
            ));
        }
        let offset = self
            .sector_offset(sector)
            .ok_or_else(|| SimError::Peripheral("sector out of range".to_string()))?;
        let end = offset + SECTOR_SIZE;
        self.disk[offset..end].fill(0);
        let copy_end = offset + data.len();
        self.disk[offset..copy_end].copy_from_slice(data);
        Ok(())
    }

    fn sector_offset(&self, sector: u64) -> Option<usize> {
        let offset = (sector as usize).checked_mul(SECTOR_SIZE)?;
        let end = offset.checked_add(SECTOR_SIZE)?;
        if end <= self.disk.len() {
            Some(offset)
        } else {
            None
        }
    }

    fn execute_command(&mut self, command_value: u32) {
        self.command = command_value;

        match command_value {
            command::READ_SECTOR => {
                if self.read_sector_to_window().is_ok() {
                    self.result = RESULT_OK;
                    self.status = 1;
                    self.req_status = 0;
                } else {
                    self.result = RESULT_IO_ERR;
                    self.status = 0;
                    self.req_status = 1;
                }
            }
            command::WRITE_SECTOR => {
                if self.write_window_to_sector().is_ok() {
                    self.result = RESULT_OK;
                    self.status = 1;
                    self.req_status = 0;
                } else {
                    self.result = RESULT_IO_ERR;
                    self.status = 0;
                    self.req_status = 1;
                }
            }
            _ => {
                self.result = RESULT_BAD_CMD;
                self.status = 0;
                self.req_status = 1;
            }
        }

        if (self.control & control_bits::IRQ_EN) != 0 {
            self.irq_pending = true;
        }
    }

    fn execute_queue_notify(&mut self, _queue_selector: u32) {
        if self.descriptor_mode_enabled() {
            self.pending_queue_notify = Some(_queue_selector);
            return;
        }

        if !self.queue_ready || self.queue_num == 0 {
            self.result = RESULT_IO_ERR;
            self.status = 0;
            self.req_status = 1;
            return;
        }

        if self.queue_avail_idx == self.queue_used_idx {
            self.result = RESULT_IO_ERR;
            self.status = 0;
            self.req_status = 1;
            return;
        }

        let op_result = match self.req_type {
            request_type::IN => self.read_sector_to_window(),
            request_type::OUT => self.write_window_to_sector(),
            _ => Err(SimError::Peripheral(
                "unsupported virtio-blk request type".to_string(),
            )),
        };

        if op_result.is_ok() {
            self.result = RESULT_OK;
            self.status = 1;
            self.req_status = 0;

            let queue_len = self.queue_num as usize;
            let used_slot = (self.queue_used_idx as usize) % queue_len;
            self.queue_used_ring[used_slot] = self.queue_head;
            self.last_used_head = self.queue_head;
            self.queue_used_idx = self.queue_used_idx.wrapping_add(1);
        } else {
            self.result = RESULT_IO_ERR;
            self.status = 0;
            self.req_status = 1;
        }

        if (self.control & control_bits::IRQ_EN) != 0 {
            self.irq_pending = true;
        }
    }

    fn descriptor_mode_enabled(&self) -> bool {
        self.queue_desc_addr != 0 && self.queue_avail_addr != 0 && self.queue_used_addr != 0
    }

    pub fn has_pending_descriptor_notify(&self) -> bool {
        self.pending_queue_notify.is_some()
    }

    pub fn process_pending_descriptor_notify(
        &mut self,
        guest_memory: &mut dyn VirtioGuestMemory,
    ) -> Result<()> {
        let Some(queue_selector) = self.pending_queue_notify.take() else {
            return Ok(());
        };

        self.execute_descriptor_queue(queue_selector, guest_memory)
    }

    fn execute_descriptor_queue(
        &mut self,
        _queue_selector: u32,
        guest_memory: &mut dyn VirtioGuestMemory,
    ) -> Result<()> {
        if !self.queue_ready || self.queue_num == 0 {
            self.result = RESULT_IO_ERR;
            self.status = 0;
            self.req_status = VIRTIO_BLK_S_IOERR;
            return Ok(());
        }

        let avail_idx = self.read_guest_u16(guest_memory, self.queue_avail_addr + 2)?;
        self.queue_avail_idx = avail_idx;

        if avail_idx == self.queue_used_idx {
            self.result = RESULT_IO_ERR;
            self.status = 0;
            self.req_status = VIRTIO_BLK_S_IOERR;
            return Ok(());
        }

        let queue_len = self.queue_num as usize;
        let ring_slot = (self.queue_used_idx as usize) % queue_len;
        let head_offset = self.queue_avail_addr + 4 + (ring_slot as u64) * 2;
        let head = self.read_guest_u16(guest_memory, head_offset)?;
        self.queue_head = head;

        let processed = self.process_descriptor_chain(head, guest_memory);
        let (status_byte, used_len) = match processed {
            Ok(len) => {
                self.result = RESULT_OK;
                self.status = 1;
                self.req_status = VIRTIO_BLK_S_OK;
                (VIRTIO_BLK_S_OK, len)
            }
            Err(_) => {
                self.result = RESULT_IO_ERR;
                self.status = 0;
                self.req_status = VIRTIO_BLK_S_IOERR;
                (VIRTIO_BLK_S_IOERR, 0)
            }
        };

        let status_desc = self.resolve_status_desc(head, guest_memory)?;
        self.write_guest_u8(guest_memory, status_desc.addr, status_byte)?;

        self.write_used_elem(guest_memory, ring_slot, head as u32, used_len)?;
        self.last_used_head = head;
        self.queue_used_ring[ring_slot] = head;
        self.queue_used_idx = self.queue_used_idx.wrapping_add(1);
        self.write_guest_u16(guest_memory, self.queue_used_addr + 2, self.queue_used_idx)?;

        if (self.control & control_bits::IRQ_EN) != 0 {
            self.irq_pending = true;
        }

        Ok(())
    }

    fn process_descriptor_chain(
        &mut self,
        head: u16,
        guest_memory: &mut dyn VirtioGuestMemory,
    ) -> Result<u32> {
        let request_desc = self.read_descriptor(guest_memory, head)?;
        if (request_desc.flags & VIRTQ_DESC_F_NEXT) == 0 {
            return Err(SimError::Peripheral(
                "virtio-blk request descriptor missing next".to_string(),
            ));
        }

        let data_desc = self.read_descriptor(guest_memory, request_desc.next)?;

        let req_type = self.read_guest_u32(guest_memory, request_desc.addr)?;
        let sector = self.read_guest_u64(guest_memory, request_desc.addr + 8)?;

        self.req_type = req_type;
        self.sector = sector;

        let transfer_len = (data_desc.len as usize).min(SECTOR_SIZE);

        match req_type {
            request_type::IN => {
                let offset = self
                    .sector_offset(sector)
                    .ok_or_else(|| SimError::Peripheral("read sector out of range".to_string()))?;

                for i in 0..transfer_len {
                    let byte = self.disk[offset + i];
                    self.data_window[i] = byte;
                    self.write_guest_u8(guest_memory, data_desc.addr + i as u64, byte)?;
                }
            }
            request_type::OUT => {
                let offset = self
                    .sector_offset(sector)
                    .ok_or_else(|| SimError::Peripheral("write sector out of range".to_string()))?;

                for i in 0..transfer_len {
                    let byte = self.read_guest_u8(guest_memory, data_desc.addr + i as u64)?;
                    self.data_window[i] = byte;
                    self.disk[offset + i] = byte;
                }
            }
            _ => {
                return Err(SimError::Peripheral(
                    "unsupported virtio-blk request type".to_string(),
                ));
            }
        }

        Ok(transfer_len as u32)
    }

    fn resolve_status_desc(
        &mut self,
        head: u16,
        guest_memory: &mut dyn VirtioGuestMemory,
    ) -> Result<VirtqDesc> {
        let request_desc = self.read_descriptor(guest_memory, head)?;
        if (request_desc.flags & VIRTQ_DESC_F_NEXT) == 0 {
            return Err(SimError::Peripheral(
                "virtio-blk request descriptor missing data link".to_string(),
            ));
        }

        let data_desc = self.read_descriptor(guest_memory, request_desc.next)?;
        if (data_desc.flags & VIRTQ_DESC_F_NEXT) == 0 {
            return Err(SimError::Peripheral(
                "virtio-blk data descriptor missing status link".to_string(),
            ));
        }

        self.read_descriptor(guest_memory, data_desc.next)
    }

    fn write_used_elem(
        &mut self,
        guest_memory: &mut dyn VirtioGuestMemory,
        slot: usize,
        id: u32,
        len: u32,
    ) -> Result<()> {
        let elem_addr = self.queue_used_addr + 4 + (slot as u64) * 8;
        self.write_guest_u32(guest_memory, elem_addr, id)?;
        self.write_guest_u32(guest_memory, elem_addr + 4, len)
    }

    fn descriptor_offset(index: u16) -> u64 {
        (index as u64) * 16
    }

    fn read_descriptor(
        &mut self,
        guest_memory: &mut dyn VirtioGuestMemory,
        index: u16,
    ) -> Result<VirtqDesc> {
        let desc_addr = self.queue_desc_addr + Self::descriptor_offset(index);
        Ok(VirtqDesc {
            addr: self.read_guest_u64(guest_memory, desc_addr)?,
            len: self.read_guest_u32(guest_memory, desc_addr + 8)?,
            flags: self.read_guest_u16(guest_memory, desc_addr + 12)?,
            next: self.read_guest_u16(guest_memory, desc_addr + 14)?,
        })
    }

    fn guest_addr(raw: u64) -> Result<Addr> {
        if raw > u32::MAX as u64 {
            return Err(SimError::Peripheral(
                "virtio guest address out of rv32 range".to_string(),
            ));
        }
        Ok(Addr::new(raw as u32))
    }

    fn read_guest_u8(&mut self, guest_memory: &mut dyn VirtioGuestMemory, addr: u64) -> Result<u8> {
        guest_memory.read_byte(Self::guest_addr(addr)?)
    }

    fn write_guest_u8(
        &mut self,
        guest_memory: &mut dyn VirtioGuestMemory,
        addr: u64,
        value: u8,
    ) -> Result<()> {
        guest_memory.write_byte(Self::guest_addr(addr)?, value)
    }

    fn read_guest_u16(
        &mut self,
        guest_memory: &mut dyn VirtioGuestMemory,
        addr: u64,
    ) -> Result<u16> {
        let b0 = self.read_guest_u8(guest_memory, addr)? as u16;
        let b1 = self.read_guest_u8(guest_memory, addr + 1)? as u16;
        Ok(b0 | (b1 << 8))
    }

    fn write_guest_u16(
        &mut self,
        guest_memory: &mut dyn VirtioGuestMemory,
        addr: u64,
        value: u16,
    ) -> Result<()> {
        self.write_guest_u8(guest_memory, addr, (value & 0xFF) as u8)?;
        self.write_guest_u8(guest_memory, addr + 1, (value >> 8) as u8)
    }

    fn read_guest_u32(
        &mut self,
        guest_memory: &mut dyn VirtioGuestMemory,
        addr: u64,
    ) -> Result<u32> {
        let mut value = 0u32;
        for i in 0..4 {
            value |= (self.read_guest_u8(guest_memory, addr + i)? as u32) << (i * 8);
        }
        Ok(value)
    }

    fn write_guest_u32(
        &mut self,
        guest_memory: &mut dyn VirtioGuestMemory,
        addr: u64,
        value: u32,
    ) -> Result<()> {
        for i in 0..4 {
            self.write_guest_u8(guest_memory, addr + i, ((value >> (i * 8)) & 0xFF) as u8)?;
        }
        Ok(())
    }

    fn read_guest_u64(
        &mut self,
        guest_memory: &mut dyn VirtioGuestMemory,
        addr: u64,
    ) -> Result<u64> {
        let mut value = 0u64;
        for i in 0..8 {
            value |= (self.read_guest_u8(guest_memory, addr + i)? as u64) << (i * 8);
        }
        Ok(value)
    }

    fn read_sector_to_window(&mut self) -> Result<()> {
        let offset = self
            .sector_offset(self.sector)
            .ok_or_else(|| SimError::Peripheral("read sector out of range".to_string()))?;
        let end = offset + SECTOR_SIZE;
        self.data_window.copy_from_slice(&self.disk[offset..end]);
        Ok(())
    }

    fn write_window_to_sector(&mut self) -> Result<()> {
        let offset = self
            .sector_offset(self.sector)
            .ok_or_else(|| SimError::Peripheral("write sector out of range".to_string()))?;
        let end = offset + SECTOR_SIZE;
        self.disk[offset..end].copy_from_slice(&self.data_window);
        Ok(())
    }

    fn read_reg_u32(&self, reg: u32) -> u32 {
        match reg {
            REG_MAGIC => 0x7472_6976, // "virt" little-endian
            REG_VERSION => 2,
            REG_DEVICE_ID => 2,           // block device
            REG_VENDOR_ID => 0x4D59_4350, // "MYCP"
            REG_STATUS => self.status,

            REG_QUEUE_DESC_LOW => self.queue_desc_addr as u32,
            REG_QUEUE_DESC_HIGH => (self.queue_desc_addr >> 32) as u32,
            REG_QUEUE_AVAIL_LOW => self.queue_avail_addr as u32,
            REG_QUEUE_AVAIL_HIGH => (self.queue_avail_addr >> 32) as u32,
            REG_QUEUE_USED_LOW => self.queue_used_addr as u32,
            REG_QUEUE_USED_HIGH => (self.queue_used_addr >> 32) as u32,
            REG_QUEUE_NUM_MAX => QUEUE_RING_MAX as u32,
            REG_QUEUE_NUM => self.queue_num as u32,
            REG_QUEUE_READY => self.queue_ready as u32,

            REG_SECTOR_LOW => self.sector as u32,
            REG_SECTOR_HIGH => (self.sector >> 32) as u32,
            REG_COMMAND => self.command,
            REG_RESULT => self.result,
            REG_CONTROL => self.control,
            REG_REQ_TYPE => self.req_type,
            REG_REQ_STATUS => self.req_status as u32,
            REG_QUEUE_HEAD => self.queue_head as u32,
            REG_QUEUE_AVAIL_IDX => self.queue_avail_idx as u32,
            REG_QUEUE_USED_IDX => self.queue_used_idx as u32,
            REG_LAST_USED_HEAD => self.last_used_head as u32,

            reg if (REG_QUEUE_USED_ELEM_BASE..REG_QUEUE_USED_ELEM_END).contains(&reg) => {
                let index = ((reg - REG_QUEUE_USED_ELEM_BASE) / 4) as usize;
                self.queue_used_ring[index] as u32
            }

            _ => 0,
        }
    }

    fn write_reg_u32(&mut self, reg: u32, value: u32) {
        match reg {
            REG_STATUS => self.status = value,

            REG_QUEUE_DESC_LOW => {
                self.queue_desc_addr =
                    (self.queue_desc_addr & 0xFFFF_FFFF_0000_0000) | value as u64;
            }
            REG_QUEUE_DESC_HIGH => {
                self.queue_desc_addr =
                    (self.queue_desc_addr & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
            }
            REG_QUEUE_AVAIL_LOW => {
                self.queue_avail_addr =
                    (self.queue_avail_addr & 0xFFFF_FFFF_0000_0000) | value as u64;
            }
            REG_QUEUE_AVAIL_HIGH => {
                self.queue_avail_addr =
                    (self.queue_avail_addr & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
            }
            REG_QUEUE_USED_LOW => {
                self.queue_used_addr =
                    (self.queue_used_addr & 0xFFFF_FFFF_0000_0000) | value as u64;
            }
            REG_QUEUE_USED_HIGH => {
                self.queue_used_addr =
                    (self.queue_used_addr & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
            }
            REG_QUEUE_NUM => {
                let requested = value as usize;
                self.queue_num = requested.min(QUEUE_RING_MAX) as u16;
            }
            REG_QUEUE_READY => {
                self.queue_ready = (value & 0x1) != 0;
            }
            REG_QUEUE_NOTIFY => {
                self.execute_queue_notify(value);
            }

            REG_SECTOR_LOW => {
                self.sector = (self.sector & 0xFFFF_FFFF_0000_0000) | value as u64;
            }
            REG_SECTOR_HIGH => {
                self.sector = (self.sector & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
            }
            REG_COMMAND => self.execute_command(value),
            REG_RESULT => self.result = value,
            REG_CONTROL => self.control = value,
            REG_REQ_TYPE => self.req_type = value,
            REG_REQ_STATUS => self.req_status = value as u8,
            REG_QUEUE_HEAD => self.queue_head = value as u16,
            REG_QUEUE_AVAIL_IDX => self.queue_avail_idx = value as u16,
            REG_QUEUE_USED_IDX => self.queue_used_idx = value as u16,
            REG_LAST_USED_HEAD => self.last_used_head = value as u16,
            _ => {}
        }
    }

    fn read_u8(&self, offset: u32) -> u8 {
        if (DATA_WINDOW_START..DATA_WINDOW_END).contains(&offset) {
            return self.data_window[(offset - DATA_WINDOW_START) as usize];
        }

        let reg = offset & !0x3;
        let shift = (offset & 0x3) * 8;
        ((self.read_reg_u32(reg) >> shift) & 0xFF) as u8
    }

    fn write_u8(&mut self, offset: u32, value: u8) {
        if (DATA_WINDOW_START..DATA_WINDOW_END).contains(&offset) {
            self.data_window[(offset - DATA_WINDOW_START) as usize] = value;
            return;
        }

        let reg = offset & !0x3;
        let shift = (offset & 0x3) * 8;

        if reg == REG_COMMAND || reg == REG_QUEUE_NOTIFY {
            if shift == 0 {
                self.write_reg_u32(reg, value as u32);
            }
            return;
        }

        let mut current = self.read_reg_u32(reg);
        current &= !(0xFF << shift);
        current |= (value as u32) << shift;
        self.write_reg_u32(reg, current);
    }
}

impl Peripheral for VirtioBlock {
    fn read(&self, offset: Addr) -> Result<u8> {
        let off = offset.raw();
        if off >= VIRTIO_BLK_SIZE as u32 {
            return Err(SimError::InvalidAddress(offset));
        }
        Ok(self.read_u8(off))
    }

    fn write(&mut self, offset: Addr, value: u8) -> Result<()> {
        let off = offset.raw();
        if off >= VIRTIO_BLK_SIZE as u32 {
            return Err(SimError::InvalidAddress(offset));
        }
        self.write_u8(off, value);
        Ok(())
    }

    fn base_addr(&self) -> Addr {
        self.base
    }

    fn size(&self) -> usize {
        VIRTIO_BLK_SIZE
    }

    fn name(&self) -> &str {
        "VirtIO-Block"
    }

    fn has_interrupt(&self) -> bool {
        self.irq_pending
    }

    fn acknowledge_interrupt(&mut self) {
        self.irq_pending = false;
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockGuestMemory {
        base: u32,
        bytes: Vec<u8>,
    }

    impl MockGuestMemory {
        fn new(base: u32, size: usize) -> Self {
            Self {
                base,
                bytes: vec![0; size],
            }
        }

        fn offset(&self, addr: Addr) -> Result<usize> {
            let target = addr.raw();
            if target < self.base {
                return Err(SimError::MemoryOutOfBounds { addr, size: 1 });
            }

            let offset = (target - self.base) as usize;
            if offset >= self.bytes.len() {
                return Err(SimError::MemoryOutOfBounds { addr, size: 1 });
            }

            Ok(offset)
        }

        fn write_u8_abs(&mut self, addr: u64, value: u8) {
            self.write_byte(Addr::new(addr as u32), value).unwrap();
        }

        fn read_u8_abs(&mut self, addr: u64) -> u8 {
            self.read_byte(Addr::new(addr as u32)).unwrap()
        }

        fn write_u16_abs(&mut self, addr: u64, value: u16) {
            self.write_u8_abs(addr, (value & 0xFF) as u8);
            self.write_u8_abs(addr + 1, (value >> 8) as u8);
        }

        fn read_u16_abs(&mut self, addr: u64) -> u16 {
            let b0 = self.read_u8_abs(addr) as u16;
            let b1 = self.read_u8_abs(addr + 1) as u16;
            b0 | (b1 << 8)
        }

        fn write_u32_abs(&mut self, addr: u64, value: u32) {
            for i in 0..4 {
                self.write_u8_abs(addr + i, ((value >> (i * 8)) & 0xFF) as u8);
            }
        }

        fn read_u32_abs(&mut self, addr: u64) -> u32 {
            let mut value = 0u32;
            for i in 0..4 {
                value |= (self.read_u8_abs(addr + i) as u32) << (i * 8);
            }
            value
        }

        fn write_u64_abs(&mut self, addr: u64, value: u64) {
            for i in 0..8 {
                self.write_u8_abs(addr + i, ((value >> (i * 8)) & 0xFF) as u8);
            }
        }
    }

    impl VirtioGuestMemory for MockGuestMemory {
        fn read_byte(&mut self, addr: Addr) -> Result<u8> {
            let offset = self.offset(addr)?;
            Ok(self.bytes[offset])
        }

        fn write_byte(&mut self, addr: Addr, value: u8) -> Result<()> {
            let offset = self.offset(addr)?;
            self.bytes[offset] = value;
            Ok(())
        }
    }

    fn read_u32(dev: &VirtioBlock, reg: u32) -> u32 {
        let mut value = 0u32;
        for i in 0..4 {
            let byte = dev.read(Addr::new(reg + i)).unwrap() as u32;
            value |= byte << (i * 8);
        }
        value
    }

    fn write_u32(dev: &mut VirtioBlock, reg: u32, value: u32) {
        for i in 0..4 {
            let byte = ((value >> (i * 8)) & 0xFF) as u8;
            dev.write(Addr::new(reg + i), byte).unwrap();
        }
    }

    #[test]
    fn test_virtio_block_identity_registers() {
        let dev = VirtioBlock::new();
        assert_eq!(read_u32(&dev, REG_MAGIC), 0x7472_6976);
        assert_eq!(read_u32(&dev, REG_VERSION), 2);
        assert_eq!(read_u32(&dev, REG_DEVICE_ID), 2);
    }

    #[test]
    fn test_virtio_block_read_sector_to_window() {
        let mut dev = VirtioBlock::with_disk_sectors(8);
        let payload = [0x11, 0x22, 0x33, 0x44];
        dev.preload_sector(3, &payload).unwrap();

        write_u32(&mut dev, REG_CONTROL, control_bits::IRQ_EN);
        write_u32(&mut dev, REG_SECTOR_LOW, 3);
        write_u32(&mut dev, REG_COMMAND, command::READ_SECTOR);

        assert_eq!(dev.read(Addr::new(DATA_WINDOW_START)).unwrap(), 0x11);
        assert_eq!(dev.read(Addr::new(DATA_WINDOW_START + 1)).unwrap(), 0x22);
        assert_eq!(read_u32(&dev, REG_RESULT), RESULT_OK);
        assert!(dev.has_interrupt());

        dev.acknowledge_interrupt();
        assert!(!dev.has_interrupt());
    }

    #[test]
    fn test_virtio_block_write_then_read_sector() {
        let mut dev = VirtioBlock::with_disk_sectors(8);

        dev.write(Addr::new(DATA_WINDOW_START), 0xAA).unwrap();
        dev.write(Addr::new(DATA_WINDOW_START + 1), 0xBB).unwrap();
        write_u32(&mut dev, REG_SECTOR_LOW, 2);
        write_u32(&mut dev, REG_COMMAND, command::WRITE_SECTOR);

        dev.write(Addr::new(DATA_WINDOW_START), 0x00).unwrap();
        dev.write(Addr::new(DATA_WINDOW_START + 1), 0x00).unwrap();
        write_u32(&mut dev, REG_SECTOR_LOW, 2);
        write_u32(&mut dev, REG_COMMAND, command::READ_SECTOR);

        assert_eq!(dev.read(Addr::new(DATA_WINDOW_START)).unwrap(), 0xAA);
        assert_eq!(dev.read(Addr::new(DATA_WINDOW_START + 1)).unwrap(), 0xBB);
        assert_eq!(read_u32(&dev, REG_RESULT), RESULT_OK);
    }

    #[test]
    fn test_virtio_block_queue_notify_read_flow() {
        let mut dev = VirtioBlock::with_disk_sectors(8);
        dev.preload_sector(1, &[0x5A, 0x6B]).unwrap();

        write_u32(&mut dev, REG_CONTROL, control_bits::IRQ_EN);
        write_u32(&mut dev, REG_QUEUE_NUM, 8);
        write_u32(&mut dev, REG_QUEUE_READY, 1);
        write_u32(&mut dev, REG_QUEUE_HEAD, 7);
        write_u32(&mut dev, REG_REQ_TYPE, request_type::IN);
        write_u32(&mut dev, REG_SECTOR_LOW, 1);
        write_u32(&mut dev, REG_QUEUE_AVAIL_IDX, 1);
        assert_eq!(read_u32(&dev, REG_QUEUE_HEAD), 7);

        write_u32(&mut dev, REG_QUEUE_NOTIFY, 0);

        assert_eq!(dev.read(Addr::new(DATA_WINDOW_START)).unwrap(), 0x5A);
        assert_eq!(dev.read(Addr::new(DATA_WINDOW_START + 1)).unwrap(), 0x6B);
        assert_eq!(read_u32(&dev, REG_REQ_STATUS), 0);
        assert_eq!(read_u32(&dev, REG_RESULT), RESULT_OK);
        assert_eq!(read_u32(&dev, REG_QUEUE_USED_IDX), 1);
        assert_eq!(read_u32(&dev, REG_LAST_USED_HEAD), 7);
        assert_eq!(read_u32(&dev, REG_QUEUE_USED_ELEM_BASE), 7);
        assert!(dev.has_interrupt());
    }

    #[test]
    fn test_virtio_block_queue_notify_write_then_read() {
        let mut dev = VirtioBlock::with_disk_sectors(8);

        write_u32(&mut dev, REG_QUEUE_NUM, 8);
        write_u32(&mut dev, REG_QUEUE_READY, 1);

        dev.write(Addr::new(DATA_WINDOW_START), 0xCC).unwrap();
        dev.write(Addr::new(DATA_WINDOW_START + 1), 0xDD).unwrap();

        write_u32(&mut dev, REG_QUEUE_HEAD, 1);
        write_u32(&mut dev, REG_REQ_TYPE, request_type::OUT);
        write_u32(&mut dev, REG_SECTOR_LOW, 2);
        write_u32(&mut dev, REG_QUEUE_AVAIL_IDX, 1);
        write_u32(&mut dev, REG_QUEUE_NOTIFY, 0);

        dev.write(Addr::new(DATA_WINDOW_START), 0x00).unwrap();
        dev.write(Addr::new(DATA_WINDOW_START + 1), 0x00).unwrap();

        write_u32(&mut dev, REG_QUEUE_HEAD, 2);
        write_u32(&mut dev, REG_REQ_TYPE, request_type::IN);
        write_u32(&mut dev, REG_SECTOR_LOW, 2);
        write_u32(&mut dev, REG_QUEUE_AVAIL_IDX, 2);
        write_u32(&mut dev, REG_QUEUE_NOTIFY, 0);

        assert_eq!(dev.read(Addr::new(DATA_WINDOW_START)).unwrap(), 0xCC);
        assert_eq!(dev.read(Addr::new(DATA_WINDOW_START + 1)).unwrap(), 0xDD);
        assert_eq!(read_u32(&dev, REG_REQ_STATUS), 0);
        assert_eq!(read_u32(&dev, REG_QUEUE_USED_IDX), 2);
        assert_eq!(read_u32(&dev, REG_LAST_USED_HEAD), 2);
    }

    #[test]
    fn test_virtio_block_queue_head_register_rw() {
        let mut dev = VirtioBlock::with_disk_sectors(8);
        write_u32(&mut dev, REG_QUEUE_HEAD, 7);
        assert_eq!(read_u32(&dev, REG_QUEUE_HEAD), 7);
    }

    #[test]
    fn test_virtio_block_descriptor_chain_read_flow() {
        let mut dev = VirtioBlock::with_disk_sectors(16);
        dev.preload_sector(3, &[0xDE, 0xAD, 0xBE, 0xEF]).unwrap();

        let guest_base = 0x8000_0000u32;
        let mut guest = MockGuestMemory::new(guest_base, 0x8000);

        let desc_addr = 0x8000_1000u64;
        let avail_addr = 0x8000_2000u64;
        let used_addr = 0x8000_3000u64;
        let req_addr = 0x8000_4000u64;
        let data_addr = 0x8000_5000u64;
        let status_addr = 0x8000_6000u64;

        let desc0 = desc_addr;
        guest.write_u64_abs(desc0, req_addr);
        guest.write_u32_abs(desc0 + 8, 16);
        guest.write_u16_abs(desc0 + 12, VIRTQ_DESC_F_NEXT);
        guest.write_u16_abs(desc0 + 14, 1);

        let desc1 = desc_addr + 16;
        guest.write_u64_abs(desc1, data_addr);
        guest.write_u32_abs(desc1 + 8, SECTOR_SIZE as u32);
        guest.write_u16_abs(desc1 + 12, VIRTQ_DESC_F_NEXT);
        guest.write_u16_abs(desc1 + 14, 2);

        let desc2 = desc_addr + 32;
        guest.write_u64_abs(desc2, status_addr);
        guest.write_u32_abs(desc2 + 8, 1);
        guest.write_u16_abs(desc2 + 12, 0);
        guest.write_u16_abs(desc2 + 14, 0);

        guest.write_u32_abs(req_addr, request_type::IN);
        guest.write_u64_abs(req_addr + 8, 3);

        guest.write_u16_abs(avail_addr + 2, 1);
        guest.write_u16_abs(avail_addr + 4, 0);

        write_u32(&mut dev, REG_CONTROL, control_bits::IRQ_EN);
        write_u32(&mut dev, REG_QUEUE_NUM, 8);
        write_u32(&mut dev, REG_QUEUE_READY, 1);
        write_u32(&mut dev, REG_QUEUE_DESC_LOW, desc_addr as u32);
        write_u32(&mut dev, REG_QUEUE_DESC_HIGH, (desc_addr >> 32) as u32);
        write_u32(&mut dev, REG_QUEUE_AVAIL_LOW, avail_addr as u32);
        write_u32(&mut dev, REG_QUEUE_AVAIL_HIGH, (avail_addr >> 32) as u32);
        write_u32(&mut dev, REG_QUEUE_USED_LOW, used_addr as u32);
        write_u32(&mut dev, REG_QUEUE_USED_HIGH, (used_addr >> 32) as u32);

        write_u32(&mut dev, REG_QUEUE_NOTIFY, 0);
        assert!(dev.has_pending_descriptor_notify());

        dev.process_pending_descriptor_notify(&mut guest).unwrap();

        assert_eq!(guest.read_u8_abs(data_addr), 0xDE);
        assert_eq!(guest.read_u8_abs(data_addr + 1), 0xAD);
        assert_eq!(guest.read_u8_abs(data_addr + 2), 0xBE);
        assert_eq!(guest.read_u8_abs(data_addr + 3), 0xEF);
        assert_eq!(guest.read_u8_abs(status_addr), VIRTIO_BLK_S_OK);

        assert_eq!(guest.read_u16_abs(used_addr + 2), 1);
        assert_eq!(guest.read_u32_abs(used_addr + 4), 0);
        assert_eq!(guest.read_u32_abs(used_addr + 8), SECTOR_SIZE as u32);

        assert_eq!(read_u32(&dev, REG_QUEUE_USED_IDX), 1);
        assert_eq!(read_u32(&dev, REG_LAST_USED_HEAD), 0);
        assert_eq!(read_u32(&dev, REG_REQ_STATUS), VIRTIO_BLK_S_OK as u32);
        assert!(dev.has_interrupt());
    }
}
