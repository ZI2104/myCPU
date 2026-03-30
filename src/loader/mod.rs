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
//!
//! let mut bus = Bus::new();
//! let loader = ElfLoader::from_file("program.elf")?;
//! loader.load_into(&mut bus)?;
//! let entry = loader.entry_point();
//! ```

use crate::error::{Result, SimError};
use crate::memory::Bus;
use crate::types::Addr;
use goblin::elf::{Elf, program_header};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// ELF loader for RISC-V executables
pub struct ElfLoader {
    /// Raw ELF bytes
    bytes: Vec<u8>,
    /// Parsed ELF structure
    elf: Elf<'static>,
    /// Entry point address
    entry: u64,
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

        let entry = elf.header.e_entry;

        // Safety: We're transmuting the Elf to 'static. This is safe because
        // the bytes Vec is owned by Self and will live as long as Self.
        // The Elf borrows from bytes, so they must have the same lifetime.
        let elf = unsafe {
            std::mem::transmute::<Elf<'_>, Elf<'static>>(elf)
        };

        Ok(Self { bytes, elf, entry })
    }

    /// Get the entry point address
    pub fn entry_point(&self) -> Addr {
        Addr::new(self.entry as u32)
    }

    /// Get the number of loadable segments
    pub fn segment_count(&self) -> usize {
        self.elf
            .program_headers
            .iter()
            .filter(|ph| ph.p_type == program_header::PT_LOAD)
            .count()
    }

    /// Load all loadable segments into memory
    pub fn load_into(&self, bus: &mut Bus) -> Result<()> {
        for ph in &self.elf.program_headers {
            if ph.p_type != program_header::PT_LOAD {
                continue;
            }

            // Get segment properties
            let vaddr = ph.p_vaddr as u32;
            let filesz = ph.p_filesz as usize;
            let memsz = ph.p_memsz as usize;
            let offset = ph.p_offset as usize;

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
        ElfHeaderInfo {
            entry: self.elf.header.e_entry,
            phoff: self.elf.header.e_phoff,
            phnum: self.elf.header.e_phnum as usize,
            shoff: self.elf.header.e_shoff,
            shnum: self.elf.header.e_shnum as usize,
        }
    }

    /// Get loadable segments info
    pub fn segments(&self) -> Vec<SegmentInfo> {
        self.elf
            .program_headers
            .iter()
            .filter(|ph| ph.p_type == program_header::PT_LOAD)
            .map(|ph| SegmentInfo {
                vaddr: ph.p_vaddr,
                memsz: ph.p_memsz,
                filesz: ph.p_filesz,
                flags: ph.p_flags,
            })
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
