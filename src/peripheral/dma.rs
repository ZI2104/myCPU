//! Shared DMA helper functions for peripheral-to-RAM data transfer.
//!
//! All MMIO accelerators (NPU, LPU, GPU, TPU) need to read/write guest RAM
//! through the Bus's `ram_regions`. This module provides a single, shared
//! implementation to eliminate duplication across peripheral files.

use crate::error::{Result, SimError};
use crate::traits::Memory;
use crate::types::{Addr, Byte};

/// Type alias for the RAM region list used by DMA helpers.
pub type RamRegions = Vec<(Addr, usize, Box<dyn Memory>)>;

/// Validate that a 64-bit address fits in the RV32 address space.
pub fn guest_addr(raw: u64) -> Result<Addr> {
    if raw > u32::MAX as u64 {
        return Err(SimError::Peripheral(format!(
            "guest address {raw:#x} out of rv32 range"
        )));
    }
    Ok(Addr::new(raw as u32))
}

/// Read a single byte from guest RAM.
pub fn read_u8(ram_regions: &mut RamRegions, addr: u64) -> Result<u8> {
    let addr = guest_addr(addr)?;
    let target = addr.raw() as usize;
    for (base, size, memory) in ram_regions.iter_mut() {
        let base_addr = base.raw() as usize;
        if target >= base_addr && target < base_addr + *size {
            let relative = Addr::new((target - base_addr) as u32);
            return memory.read_byte(relative).map(|b| b.raw());
        }
    }
    Err(SimError::MemoryOutOfBounds { addr, size: 1 })
}

/// Write a single byte to guest RAM.
pub fn write_u8(ram_regions: &mut RamRegions, addr: u64, value: u8) -> Result<()> {
    let addr = guest_addr(addr)?;
    let target = addr.raw() as usize;
    for (base, size, memory) in ram_regions.iter_mut() {
        let base_addr = base.raw() as usize;
        if target >= base_addr && target < base_addr + *size {
            let relative = Addr::new((target - base_addr) as u32);
            return memory.write_byte(relative, Byte::new(value));
        }
    }
    Err(SimError::MemoryOutOfBounds { addr, size: 1 })
}

/// Read a u32 (little-endian) from guest RAM.
pub fn read_u32(ram_regions: &mut RamRegions, addr: u64) -> Result<u32> {
    let mut v = 0u32;
    for i in 0..4 {
        v |= (read_u8(ram_regions, addr + i)? as u32) << (i * 8);
    }
    Ok(v)
}

/// Write a u32 (little-endian) to guest RAM.
pub fn write_u32(ram_regions: &mut RamRegions, addr: u64, value: u32) -> Result<()> {
    for i in 0..4 {
        write_u8(ram_regions, addr + i, ((value >> (i * 8)) & 0xFF) as u8)?;
    }
    Ok(())
}

/// Read a contiguous block of f32 from guest RAM.
pub fn read_f32_slice(ram_regions: &mut RamRegions, addr: u64, count: usize) -> Result<Vec<f32>> {
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let bits = read_u32(ram_regions, addr + (i as u64) * 4)?;
        out.push(f32::from_bits(bits));
    }
    Ok(out)
}

/// Write a contiguous block of f32 into guest RAM.
pub fn write_f32_slice(ram_regions: &mut RamRegions, addr: u64, data: &[f32]) -> Result<()> {
    for (i, &v) in data.iter().enumerate() {
        write_u32(ram_regions, addr + (i as u64) * 4, v.to_bits())?;
    }
    Ok(())
}

/// Read a contiguous block of i8 from guest RAM.
pub fn read_i8_slice(ram_regions: &mut RamRegions, addr: u64, count: usize) -> Result<Vec<i8>> {
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        out.push(read_u8(ram_regions, addr + i as u64)? as i8);
    }
    Ok(out)
}

/// Write a contiguous block of i8 into guest RAM.
pub fn write_i8_slice(ram_regions: &mut RamRegions, addr: u64, data: &[i8]) -> Result<()> {
    for (i, &v) in data.iter().enumerate() {
        write_u8(ram_regions, addr + i as u64, v as u8)?;
    }
    Ok(())
}

/// Write a 20-byte tensor descriptor into guest RAM.
///
/// Layout: `[data_addr(4) | element_count(4) | shape[0](4) | shape[1](4) | shape[2](4)]`
pub fn write_tensor_desc(
    ram_regions: &mut RamRegions,
    desc_addr: u64,
    data_addr: u32,
    element_count: u32,
    shape: [u32; 3],
) -> Result<()> {
    write_u32(ram_regions, desc_addr, data_addr)?;
    write_u32(ram_regions, desc_addr + 4, element_count)?;
    write_u32(ram_regions, desc_addr + 8, shape[0])?;
    write_u32(ram_regions, desc_addr + 12, shape[1])?;
    write_u32(ram_regions, desc_addr + 16, shape[2])
}

/// Read a 20-byte tensor descriptor from guest RAM.
///
/// Returns `(data_addr, element_count, [shape0, shape1, shape2])`.
pub fn read_tensor_desc(
    ram_regions: &mut RamRegions,
    desc_addr: u64,
) -> Result<(u64, usize, [u32; 3])> {
    let data_addr = read_u32(ram_regions, desc_addr)? as u64;
    let element_count = read_u32(ram_regions, desc_addr + 4)?;
    let s0 = read_u32(ram_regions, desc_addr + 8)?;
    let s1 = read_u32(ram_regions, desc_addr + 12)?;
    let s2 = read_u32(ram_regions, desc_addr + 16)?;
    Ok((data_addr, element_count as usize, [s0, s1, s2]))
}
