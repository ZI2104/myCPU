//! Memory (MEM) Stage.
//!
//! This module implements the MEM stage of the pipeline.

use crate::cpu::pipeline::registers::{ExMemRegister, MemWbRegister};
use crate::cpu::pipeline::control::{WbControlSignals, mem_width};
use crate::error::Result;
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

    /// Execute the MEM stage.
    ///
    /// # Arguments
    /// * `ex_mem` - EX/MEM pipeline register input
    /// * `bus` - Memory bus for memory operations
    ///
    /// # Returns
    /// The MEM/WB pipeline register output.
    pub fn execute(
        &mut self,
        ex_mem: &ExMemRegister,
        bus: &mut Bus,
    ) -> Result<MemWbRegister> {
        // Handle invalid instruction
        if !ex_mem.valid {
            return Ok(MemWbRegister {
                pc: ex_mem.pc,
                write_data: Word::ZERO,
                rd: ex_mem.rd,
                ctrl: WbControlSignals::none(),
                valid: false,
            });
        }

        let addr = Addr::new(ex_mem.alu_result.raw());
        let mut mem_data = Word::ZERO;

        // Handle memory read
        if ex_mem.ctrl.mem_read {
            mem_data = self.read_memory(bus, addr, &ex_mem.ctrl)?;
            self.mem_data = mem_data;
        }

        // Handle memory write
        if ex_mem.ctrl.mem_write {
            self.write_memory(bus, addr, ex_mem.store_data, &ex_mem.ctrl)?;
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
            write_data,
            rd: ex_mem.rd,
            ctrl: wb_ctrl,
            valid: true,
        })
    }

    /// Read from memory based on width.
    fn read_memory(
        &self,
        bus: &Bus,
        addr: Addr,
        ctrl: &crate::cpu::pipeline::control::MemControlSignals,
    ) -> Result<Word> {
        match ctrl.mem_width {
            mem_width::BYTE => {
                let byte = bus.read_byte(addr)?;
                if ctrl.mem_sign_extend {
                    Ok(Word::from_byte(byte.raw()))
                } else {
                    Ok(Word::from_byte_zero(byte.raw()))
                }
            }
            mem_width::HALF => {
                let half = bus.read_half(addr)?;
                if ctrl.mem_sign_extend {
                    Ok(Word::from_half(half.raw()))
                } else {
                    Ok(Word::from_half_zero(half.raw()))
                }
            }
            mem_width::WORD => bus.read_word(addr),
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
    ) -> Result<()> {
        match ctrl.mem_width {
            mem_width::BYTE => {
                bus.write_byte(addr, Byte::new(data.byte()))?;
            }
            mem_width::HALF => {
                bus.write_half(addr, Half::new(data.half()))?;
            }
            mem_width::WORD => {
                bus.write_word(addr, data)?;
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
            valid: true,
        }
    }

    #[test]
    fn test_load_word() {
        let mut bus = create_test_bus();
        bus.write_word(Addr::new(0x100), Word::new(0x12345678)).unwrap();

        let mut stage = MemoryStage::new();
        let ex_mem = create_ex_mem_for_load(0x100, 4, false);

        let mem_wb = stage.execute(&ex_mem, &mut bus).unwrap();

        assert!(mem_wb.valid);
        assert_eq!(mem_wb.write_data.raw(), 0x12345678);
        assert!(mem_wb.ctrl.reg_write);
        assert!(mem_wb.ctrl.mem_to_reg);
    }

    #[test]
    fn test_load_byte_unsigned() {
        let mut bus = create_test_bus();
        bus.write_byte(Addr::new(0x100), Byte::new(0x80)).unwrap();

        let mut stage = MemoryStage::new();
        let ex_mem = create_ex_mem_for_load(0x100, 1, false);

        let mem_wb = stage.execute(&ex_mem, &mut bus).unwrap();

        // LBU: zero-extend
        assert_eq!(mem_wb.write_data.raw(), 0x80);
    }

    #[test]
    fn test_load_byte_signed() {
        let mut bus = create_test_bus();
        bus.write_byte(Addr::new(0x100), Byte::new(0x80)).unwrap();

        let mut stage = MemoryStage::new();
        let ex_mem = create_ex_mem_for_load(0x100, 1, true);

        let mem_wb = stage.execute(&ex_mem, &mut bus).unwrap();

        // LB: sign-extend 0x80 -> 0xFFFFFF80
        assert_eq!(mem_wb.write_data.raw(), 0xFFFFFF80);
    }

    #[test]
    fn test_store_word() {
        let mut bus = create_test_bus();
        let mut stage = MemoryStage::new();

        let ex_mem = create_ex_mem_for_store(0x100, 0xDEADBEEF, 4);
        let mem_wb = stage.execute(&ex_mem, &mut bus).unwrap();

        assert!(mem_wb.valid);
        assert!(!mem_wb.ctrl.reg_write);

        // Verify memory was written
        let read_val = bus.read_word(Addr::new(0x100)).unwrap();
        assert_eq!(read_val.raw(), 0xDEADBEEF);
    }

    #[test]
    fn test_store_byte() {
        let mut bus = create_test_bus();
        let mut stage = MemoryStage::new();

        let ex_mem = create_ex_mem_for_store(0x100, 0x12345678, 1);
        let _ = stage.execute(&ex_mem, &mut bus).unwrap();

        // Verify only low byte was written
        let read_byte = bus.read_byte(Addr::new(0x100)).unwrap();
        assert_eq!(read_byte.raw(), 0x78);
    }

    #[test]
    fn test_alu_result_passthrough() {
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
            valid: true,
        };

        let mem_wb = stage.execute(&ex_mem, &mut bus).unwrap();

        assert_eq!(mem_wb.write_data.raw(), 0x12345);
        assert!(!mem_wb.ctrl.mem_to_reg);
    }
}
