//! Memory (MEM) Stage.
//!
//! This module implements the MEM stage of the pipeline.
//! In the synchronous RAM design, load data arrives via the `DataReadLatch`
//! (requested in the EX stage), while stores still write directly to the bus.

use crate::cpu::pipeline::control::{mem_width, WbControlSignals};
use crate::cpu::pipeline::registers::{DataReadLatch, ExMemRegister, MemWbRegister};
use crate::error::{MemoryAccessType, Result};
use crate::memory::Bus;
use crate::types::{Addr, Byte, Half, Word};

/// Memory stage.
#[derive(Debug, Clone, Default)]
pub struct MemoryStage {
    /// Data read from memory (for debugging).
    pub mem_data: Word,
}

impl MemoryStage {
    /// Create a new memory stage.
    pub fn new() -> Self {
        Self::default()
    }

    /// Execute MEM stage with synchronous RAM latch.
    ///
    /// In the synchronous RAM design, load data is provided via `data_latch`
    /// (the data was read from the bus during the EX stage). Store operations
    /// still write to the bus in this stage.
    ///
    /// # Arguments
    /// * `ex_mem` - EX/MEM pipeline register input
    /// * `data_latch` - Data read from bus in EX stage (for loads)
    /// * `bus` - Memory bus for store operations
    /// * `translate` - Address translation hook
    pub fn execute_with_latch<F>(
        &mut self,
        ex_mem: &ExMemRegister,
        data_latch: &DataReadLatch,
        bus: &mut Bus,
        mut translate: F,
    ) -> Result<MemWbRegister>
    where
        F: FnMut(&mut Bus, Addr, MemoryAccessType) -> Result<Addr>,
    {
        // Handle invalid instruction
        if !ex_mem.valid {
            return Ok(MemWbRegister::bubble_from_ex_mem(ex_mem));
        }

        let addr = Addr::new(ex_mem.alu_result.raw());
        let mut mem_data = Word::ZERO;

        // Handle memory read: use data from latch (requested in EX stage)
        if ex_mem.ctrl.mem_read && data_latch.valid {
            mem_data = Self::extract_latch_data(data_latch);
            self.mem_data = mem_data;
        }

        // Handle memory write: translate and write to bus (same as before)
        if ex_mem.ctrl.mem_write {
            self.write_memory(bus, addr, ex_mem.store_data, &ex_mem.ctrl, &mut translate)?;
        }

        // Determine write-back data
        let write_data = if ex_mem.ctrl.mem_read {
            mem_data
        } else {
            ex_mem.alu_result
        };

        // Create WB control signals
        let wb_ctrl = WbControlSignals {
            reg_write: ex_mem.ctrl.reg_write,
            mem_to_reg: ex_mem.ctrl.mem_read,
        };

        Ok(MemWbRegister {
            pc: ex_mem.pc,
            alu_result: ex_mem.alu_result,
            write_data,
            rd: ex_mem.rd,
            mem_read: ex_mem.ctrl.mem_read,
            mem_write: ex_mem.ctrl.mem_write,
            ctrl: wb_ctrl,
            valid: true,
        })
    }

    /// Extract and sign/zero-extend data from the latch based on access width.
    fn extract_latch_data(latch: &DataReadLatch) -> Word {
        match latch.width {
            mem_width::BYTE => {
                let byte_val = latch.raw_data.raw() as u8;
                if latch.sign_extend {
                    Word::from_byte(byte_val)
                } else {
                    Word::from_byte_zero(byte_val)
                }
            }
            mem_width::HALF => {
                let half_val = latch.raw_data.raw() as u16;
                if latch.sign_extend {
                    Word::from_half(half_val)
                } else {
                    Word::from_half_zero(half_val)
                }
            }
            mem_width::WORD => latch.raw_data,
            _ => Word::ZERO,
        }
    }

    // -----------------------------------------------------------------------
    // Legacy API (kept for backward compatibility)
    // -----------------------------------------------------------------------

    /// Execute the MEM stage (legacy, for single-cycle model).
    pub fn execute(&mut self, ex_mem: &ExMemRegister, bus: &mut Bus) -> Result<MemWbRegister> {
        self.execute_with_translate(ex_mem, bus, |_bus, addr, _access| Ok(addr))
    }

    /// Execute MEM stage with address translation hook (legacy).
    pub fn execute_with_translate<F>(
        &mut self,
        ex_mem: &ExMemRegister,
        bus: &mut Bus,
        mut translate: F,
    ) -> Result<MemWbRegister>
    where
        F: FnMut(&mut Bus, Addr, MemoryAccessType) -> Result<Addr>,
    {
        if !ex_mem.valid {
            return Ok(MemWbRegister::bubble_from_ex_mem(ex_mem));
        }

        let addr = Addr::new(ex_mem.alu_result.raw());
        let mut mem_data = Word::ZERO;

        if ex_mem.ctrl.mem_read {
            mem_data = self.read_memory(bus, addr, &ex_mem.ctrl, &mut translate)?;
            self.mem_data = mem_data;
        }

        if ex_mem.ctrl.mem_write {
            self.write_memory(bus, addr, ex_mem.store_data, &ex_mem.ctrl, &mut translate)?;
        }

        let write_data = if ex_mem.ctrl.mem_read {
            mem_data
        } else {
            ex_mem.alu_result
        };

        let wb_ctrl = WbControlSignals {
            reg_write: ex_mem.ctrl.reg_write,
            mem_to_reg: ex_mem.ctrl.mem_read,
        };

        Ok(MemWbRegister {
            pc: ex_mem.pc,
            alu_result: ex_mem.alu_result,
            write_data,
            rd: ex_mem.rd,
            mem_read: ex_mem.ctrl.mem_read,
            mem_write: ex_mem.ctrl.mem_write,
            ctrl: wb_ctrl,
            valid: true,
        })
    }

    /// Read from memory based on width (legacy helper).
    fn read_memory(
        &self,
        bus: &mut Bus,
        addr: Addr,
        ctrl: &crate::cpu::pipeline::control::MemControlSignals,
        translate: &mut impl FnMut(&mut Bus, Addr, MemoryAccessType) -> Result<Addr>,
    ) -> Result<Word> {
        let paddr = translate(bus, addr, MemoryAccessType::Load)?;
        match ctrl.mem_width {
            mem_width::BYTE => {
                let byte = bus.read_byte(paddr)?;
                if ctrl.mem_sign_extend {
                    Ok(Word::from_byte(byte.raw()))
                } else {
                    Ok(Word::from_byte_zero(byte.raw()))
                }
            }
            mem_width::HALF => {
                let half = bus.read_half(paddr)?;
                if ctrl.mem_sign_extend {
                    Ok(Word::from_half(half.raw()))
                } else {
                    Ok(Word::from_half_zero(half.raw()))
                }
            }
            mem_width::WORD => bus.read_word(paddr),
            _ => Ok(Word::ZERO),
        }
    }

    /// Write to memory based on width.
    fn write_memory(
        &self,
        bus: &mut Bus,
        addr: Addr,
        data: Word,
        ctrl: &crate::cpu::pipeline::control::MemControlSignals,
        translate: &mut impl FnMut(&mut Bus, Addr, MemoryAccessType) -> Result<Addr>,
    ) -> Result<()> {
        let paddr = translate(bus, addr, MemoryAccessType::Store)?;
        match ctrl.mem_width {
            mem_width::BYTE => {
                bus.write_byte(paddr, Byte::new(data.byte()))?;
            }
            mem_width::HALF => {
                bus.write_half(paddr, Half::new(data.half()))?;
            }
            mem_width::WORD => {
                bus.write_word(paddr, data)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Reset the memory stage.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::pipeline::control::MemControlSignals;
    use crate::memory::{Bus, Ram};
    use crate::types::RegIdx;

    fn create_test_bus() -> Bus {
        let mut bus = Bus::new();
        let ram = Ram::new(4096);
        bus.attach_memory(Addr::new(0), ram, "RAM");
        bus
    }

    fn create_ex_mem_for_load(addr: u32, width: u32, sign_extend: bool) -> ExMemRegister {
        ExMemRegister {
            pc: Addr::new(0),
            pc_plus_4: Addr::new(4),
            alu_result: Word::new(addr),
            store_data: Word::ZERO,
            rd: RegIdx::new(1),
            ctrl: MemControlSignals {
                mem_read: true,
                mem_write: false,
                mem_width: width,
                mem_sign_extend: sign_extend,
                reg_write: true,
            },
            branch_taken: false,
            branch_target: Addr::new(0),
            forward_rs1: crate::cpu::pipeline::forward::ForwardSource::None,
            forward_rs2: crate::cpu::pipeline::forward::ForwardSource::None,
            valid: true,
        }
    }

    fn create_ex_mem_for_store(addr: u32, data: u32, width: u32) -> ExMemRegister {
        ExMemRegister {
            pc: Addr::new(0),
            pc_plus_4: Addr::new(4),
            alu_result: Word::new(addr),
            store_data: Word::new(data),
            rd: RegIdx::new(0),
            ctrl: MemControlSignals {
                mem_read: false,
                mem_write: true,
                mem_width: width,
                mem_sign_extend: false,
                reg_write: false,
            },
            branch_taken: false,
            branch_target: Addr::new(0),
            forward_rs1: crate::cpu::pipeline::forward::ForwardSource::None,
            forward_rs2: crate::cpu::pipeline::forward::ForwardSource::None,
            valid: true,
        }
    }

    // === Synchronous RAM latch tests ===

    #[test]
    fn test_load_word_from_latch() {
        let mut bus = create_test_bus();

        let data_latch = DataReadLatch {
            paddr: Addr::new(0x100),
            raw_data: Word::new(0x12345678),
            width: 4,
            sign_extend: false,
            valid: true,
        };

        let mut stage = MemoryStage::new();
        let ex_mem = create_ex_mem_for_load(0x100, 4, false);

        let mem_wb = stage
            .execute_with_latch(&ex_mem, &data_latch, &mut bus, |_bus, addr, _access| {
                Ok(addr)
            })
            .unwrap();

        assert!(mem_wb.valid);
        assert_eq!(mem_wb.write_data.raw(), 0x12345678);
        assert!(mem_wb.ctrl.reg_write);
        assert!(mem_wb.ctrl.mem_to_reg);
    }

    #[test]
    fn test_load_byte_unsigned_from_latch() {
        let mut bus = create_test_bus();

        let data_latch = DataReadLatch {
            paddr: Addr::new(0x100),
            raw_data: Word::new(0x80), // byte value 0x80
            width: 1,
            sign_extend: false,
            valid: true,
        };

        let mut stage = MemoryStage::new();
        let ex_mem = create_ex_mem_for_load(0x100, 1, false);

        let mem_wb = stage
            .execute_with_latch(&ex_mem, &data_latch, &mut bus, |_bus, addr, _access| {
                Ok(addr)
            })
            .unwrap();

        // LBU: zero-extend
        assert_eq!(mem_wb.write_data.raw(), 0x80);
    }

    #[test]
    fn test_load_byte_signed_from_latch() {
        let mut bus = create_test_bus();

        let data_latch = DataReadLatch {
            paddr: Addr::new(0x100),
            raw_data: Word::new(0x80), // byte value 0x80 (negative in signed)
            width: 1,
            sign_extend: true,
            valid: true,
        };

        let mut stage = MemoryStage::new();
        let ex_mem = create_ex_mem_for_load(0x100, 1, true);

        let mem_wb = stage
            .execute_with_latch(&ex_mem, &data_latch, &mut bus, |_bus, addr, _access| {
                Ok(addr)
            })
            .unwrap();

        // LB: sign-extend 0x80 -> 0xFFFFFF80
        assert_eq!(mem_wb.write_data.raw(), 0xFFFFFF80);
    }

    #[test]
    fn test_store_word_with_latch() {
        let mut bus = create_test_bus();
        let mut stage = MemoryStage::new();

        let data_latch = DataReadLatch::default(); // No load data needed for store

        let ex_mem = create_ex_mem_for_store(0x100, 0xDEADBEEF, 4);
        let mem_wb = stage
            .execute_with_latch(&ex_mem, &data_latch, &mut bus, |_bus, addr, _access| {
                Ok(addr)
            })
            .unwrap();

        assert!(mem_wb.valid);
        assert!(!mem_wb.ctrl.reg_write);

        // Verify memory was written
        let read_val = bus.read_word(Addr::new(0x100)).unwrap();
        assert_eq!(read_val.raw(), 0xDEADBEEF);
    }

    #[test]
    fn test_invalid_latch_gives_zero() {
        let mut bus = create_test_bus();

        let data_latch = DataReadLatch {
            paddr: Addr::new(0),
            raw_data: Word::new(0x12345678),
            width: 4,
            sign_extend: false,
            valid: false, // Invalid!
        };

        let mut stage = MemoryStage::new();
        let ex_mem = create_ex_mem_for_load(0x100, 4, false);

        let mem_wb = stage
            .execute_with_latch(&ex_mem, &data_latch, &mut bus, |_bus, addr, _access| {
                Ok(addr)
            })
            .unwrap();

        // With invalid latch, mem_data stays ZERO but write_data is ALU result
        // Actually no: mem_read is true, so write_data = mem_data = ZERO
        assert_eq!(mem_wb.write_data.raw(), 0);
    }

    // === Legacy API tests (unchanged) ===

    #[test]
    fn test_load_word_legacy() {
        let mut bus = create_test_bus();
        bus.write_word(Addr::new(0x100), Word::new(0x12345678))
            .unwrap();

        let mut stage = MemoryStage::new();
        let ex_mem = create_ex_mem_for_load(0x100, 4, false);

        let mem_wb = stage.execute(&ex_mem, &mut bus).unwrap();

        assert!(mem_wb.valid);
        assert_eq!(mem_wb.write_data.raw(), 0x12345678);
        assert!(mem_wb.ctrl.reg_write);
        assert!(mem_wb.ctrl.mem_to_reg);
    }

    #[test]
    fn test_load_byte_unsigned_legacy() {
        let mut bus = create_test_bus();
        bus.write_byte(Addr::new(0x100), Byte::new(0x80)).unwrap();

        let mut stage = MemoryStage::new();
        let ex_mem = create_ex_mem_for_load(0x100, 1, false);

        let mem_wb = stage.execute(&ex_mem, &mut bus).unwrap();

        assert_eq!(mem_wb.write_data.raw(), 0x80);
    }

    #[test]
    fn test_load_byte_signed_legacy() {
        let mut bus = create_test_bus();
        bus.write_byte(Addr::new(0x100), Byte::new(0x80)).unwrap();

        let mut stage = MemoryStage::new();
        let ex_mem = create_ex_mem_for_load(0x100, 1, true);

        let mem_wb = stage.execute(&ex_mem, &mut bus).unwrap();

        assert_eq!(mem_wb.write_data.raw(), 0xFFFFFF80);
    }

    #[test]
    fn test_store_word_legacy() {
        let mut bus = create_test_bus();
        let mut stage = MemoryStage::new();

        let ex_mem = create_ex_mem_for_store(0x100, 0xDEADBEEF, 4);
        let mem_wb = stage.execute(&ex_mem, &mut bus).unwrap();

        assert!(mem_wb.valid);
        assert!(!mem_wb.ctrl.reg_write);

        let read_val = bus.read_word(Addr::new(0x100)).unwrap();
        assert_eq!(read_val.raw(), 0xDEADBEEF);
    }

    #[test]
    fn test_store_byte_legacy() {
        let mut bus = create_test_bus();
        let mut stage = MemoryStage::new();

        let ex_mem = create_ex_mem_for_store(0x100, 0x12345678, 1);
        let _ = stage.execute(&ex_mem, &mut bus).unwrap();

        let read_byte = bus.read_byte(Addr::new(0x100)).unwrap();
        assert_eq!(read_byte.raw(), 0x78);
    }

    #[test]
    fn test_alu_result_passthrough_legacy() {
        let mut bus = create_test_bus();
        let mut stage = MemoryStage::new();

        let ex_mem = ExMemRegister {
            pc: Addr::new(0),
            pc_plus_4: Addr::new(4),
            alu_result: Word::new(0x12345),
            store_data: Word::ZERO,
            rd: RegIdx::new(1),
            ctrl: MemControlSignals::none(),
            branch_taken: false,
            branch_target: Addr::new(0),
            forward_rs1: crate::cpu::pipeline::forward::ForwardSource::None,
            forward_rs2: crate::cpu::pipeline::forward::ForwardSource::None,
            valid: true,
        };

        let mem_wb = stage.execute(&ex_mem, &mut bus).unwrap();

        assert_eq!(mem_wb.write_data.raw(), 0x12345);
        assert!(!mem_wb.ctrl.mem_to_reg);
    }
}
