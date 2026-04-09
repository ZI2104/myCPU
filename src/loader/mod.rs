//! ELF Loader module
//!
//! This module provides ELF file parsing and loading functionality
//! using the `goblin` crate. It supports loading RISC-V ELF executables
//! into the simulator's memory.
//!
//! # Features
//!
//! - Parse ELF headers and program headers
//! - Load loadable segments into memory
//! - Resolve entry point address
//!
//! # Example
//!
//! ```rust,no_run
//! use mycpu::loader::ElfLoader;
//! use mycpu::memory::Bus;
//! use mycpu::error::Result;
//!
//! fn main() -> Result<()> {
//! let mut bus = Bus::new();
//! let loader = ElfLoader::from_file("program.elf")?;
//! loader.load_into(&mut bus)?;
//! let entry = loader.entry_point();
//! let _ = entry;
//! Ok(())
//! }
//! ```

use crate::error::{Result, SimError};
use crate::memory::Bus;
use crate::types::Addr;
use goblin::elf::{program_header, Elf};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// ELF loader for RISC-V executables
pub struct ElfLoader {
    /// Raw ELF bytes
    bytes: Vec<u8>,
    /// Entry point address
    entry: u64,
    /// Cached ELF header information
    header: ElfHeaderInfo,
    /// Cached loadable segments for safe loading without self-referential borrows
    loadable_segments: Vec<LoadableSegment>,
}

/// Internal loadable segment representation.
#[derive(Debug, Clone)]
struct LoadableSegment {
    /// File offset for segment data
    offset: usize,
    /// Segment info exposed to callers
    info: SegmentInfo,
}

impl ElfLoader {
    /// Load an ELF file from disk
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = File::open(path.as_ref()).map_err(|e| SimError::IoError(e.to_string()))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|e| SimError::IoError(e.to_string()))?;
        Self::from_bytes(bytes)
    }

    /// Parse ELF from bytes
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        // Parse the ELF file
        let elf = Elf::parse(&bytes).map_err(|e| SimError::ElfParseError(e.to_string()))?;

        // Verify it's a RISC-V ELF
        if elf.header.e_machine != goblin::elf::header::EM_RISCV {
            return Err(SimError::ElfParseError(format!(
                "Expected RISC-V ELF (machine type {}), got {}",
                goblin::elf::header::EM_RISCV,
                elf.header.e_machine
            )));
        }

        // Verify it's 32-bit (RV32I)
        if elf.is_64 {
            return Err(SimError::ElfParseError(
                "Expected 32-bit ELF for RV32I, got 64-bit".to_string(),
            ));
        }

        let header = ElfHeaderInfo {
            entry: elf.header.e_entry,
            phoff: elf.header.e_phoff,
            phnum: elf.header.e_phnum as usize,
            shoff: elf.header.e_shoff,
            shnum: elf.header.e_shnum as usize,
        };

        let mut loadable_segments = Vec::new();
        for ph in &elf.program_headers {
            if ph.p_type != program_header::PT_LOAD {
                continue;
            }

            let offset = ph.p_offset as usize;
            let filesz = ph.p_filesz as usize;
            let end = offset.checked_add(filesz).ok_or_else(|| {
                SimError::ElfParseError("ELF segment offset overflow".to_string())
            })?;

            if end > bytes.len() {
                return Err(SimError::ElfParseError(format!(
                    "ELF segment out of bounds: offset=0x{:x}, filesz=0x{:x}, file_size=0x{:x}",
                    offset,
                    filesz,
                    bytes.len()
                )));
            }

            loadable_segments.push(LoadableSegment {
                offset,
                info: SegmentInfo {
                    vaddr: ph.p_vaddr,
                    memsz: ph.p_memsz,
                    filesz: ph.p_filesz,
                    flags: ph.p_flags,
                },
            });
        }

        let entry = header.entry;

        Ok(Self {
            bytes,
            entry,
            header,
            loadable_segments,
        })
    }

    /// Get the entry point address
    pub fn entry_point(&self) -> Addr {
        Addr::new(self.entry as u32)
    }

    /// Get the number of loadable segments
    pub fn segment_count(&self) -> usize {
        self.loadable_segments.len()
    }

    /// Load all loadable segments into memory
    pub fn load_into(&self, bus: &mut Bus) -> Result<()> {
        for segment in &self.loadable_segments {
            // Get segment properties
            let vaddr = segment.info.vaddr as u32;
            let filesz = segment.info.filesz as usize;
            let memsz = segment.info.memsz as usize;
            let offset = segment.offset;

            // Copy file contents using bulk write
            if filesz > 0 {
                let segment_data = &self.bytes[offset..offset + filesz];
                bus.write_bytes(Addr::new(vaddr), segment_data)?;
            }

            // Zero-fill BSS section (memsz > filesz)
            if memsz > filesz {
                let bss_size = memsz - filesz;
                let bss_addr = Addr::new(vaddr + filesz as u32);
                let zeros = vec![0u8; bss_size];
                bus.write_bytes(bss_addr, &zeros)?;
            }
        }

        Ok(())
    }

    /// Get the ELF header info
    pub fn header_info(&self) -> ElfHeaderInfo {
        self.header.clone()
    }

    /// Get loadable segments info
    pub fn segments(&self) -> Vec<SegmentInfo> {
        self.loadable_segments
            .iter()
            .map(|seg| seg.info.clone())
            .collect()
    }
}

/// ELF header information
#[derive(Debug, Clone)]
pub struct ElfHeaderInfo {
    /// Entry point address
    pub entry: u64,
    /// Program header table offset
    pub phoff: u64,
    /// Number of program headers
    pub phnum: usize,
    /// Section header table offset
    pub shoff: u64,
    /// Number of section headers
    pub shnum: usize,
}

/// Segment information
#[derive(Debug, Clone)]
pub struct SegmentInfo {
    /// Virtual address
    pub vaddr: u64,
    /// Size in memory
    pub memsz: u64,
    /// Size in file
    pub filesz: u64,
    /// Segment flags (R/W/X)
    pub flags: u32,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require actual ELF files to work
    // In a real project, you'd include test fixtures

    #[test]
    fn test_invalid_elf_bytes() {
        let bytes = vec![0x00, 0x00, 0x00, 0x00];
        let result = ElfLoader::from_bytes(bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_elf_magic() {
        // Valid ELF magic but not a complete ELF
        let bytes = vec![0x7F, b'E', b'L', b'F'];
        let result = ElfLoader::from_bytes(bytes);
        assert!(result.is_err());
    }
}
