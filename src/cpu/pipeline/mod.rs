//! 5-Stage Pipeline Implementation.
//!
//! This module provides a 5-stage pipelined CPU implementation (IF/ID/EX/MEM/WB)
//! with hazard detection, forwarding, and branch prediction.

mod control;
mod forward;
mod hazard;
mod registers;
pub mod stages;

pub use control::{AluOp, AluSrc, BranchType, ExControlSignals, MemControlSignals, WbControlSignals};
pub use forward::{ForwardSource, ForwardUnit};
pub use hazard::HazardUnit;
pub use registers::{ExMemRegister, IdExRegister, IfIdRegister, MemWbRegister};

use crate::cpu::execution_model::ExecutionModel;
use crate::cpu::pipeline::stages::{DecodeStage, ExecuteStage, FetchStage, MemoryStage, WritebackStage};
use crate::cpu::{CpuState, ProgramCounter, Registers};
use crate::error::{Result, SimError};
use crate::memory::Bus;
use crate::types::{Addr, PrivilegeLevel};
#[cfg(test)]
use crate::types::Word;

/// 5-stage pipelined CPU.
#[derive(Debug)]
pub struct PipelineCpu {
    // Shared resources
    regs: Registers,
    bus: Bus,
    privilege: PrivilegeLevel,

    // PC register
    pc: ProgramCounter,

    // Pipeline registers
    if_id: IfIdRegister,
    id_ex: IdExRegister,
    ex_mem: ExMemRegister,
    mem_wb: MemWbRegister,

    // Pipeline stages
    fetch_stage: FetchStage,
    decode_stage: DecodeStage,
    execute_stage: ExecuteStage,
    memory_stage: MemoryStage,
    writeback_stage: WritebackStage,

    // Hazard and forwarding units
    hazard_unit: HazardUnit,

    // CPU state
    instructions_executed: u64,
    cycles: u64,
    halted: bool,
}

impl PipelineCpu {
    /// Create a new pipelined CPU with the given system bus.
    pub fn new(bus: Bus) -> Self {
        Self {
            regs: Registers::new(),
            bus,
            privilege: PrivilegeLevel::Machine,
            pc: ProgramCounter::zero(),
            if_id: IfIdRegister::default(),
            id_ex: IdExRegister::default(),
            ex_mem: ExMemRegister::default(),
            mem_wb: MemWbRegister::default(),
            fetch_stage: FetchStage::new(),
            decode_stage: DecodeStage::new(),
            execute_stage: ExecuteStage::new(),
            memory_stage: MemoryStage::new(),
            writeback_stage: WritebackStage::new(),
            hazard_unit: HazardUnit::default(),
            instructions_executed: 0,
            cycles: 0,
            halted: false,
        }
    }

    /// Create a new pipelined CPU with the given bus and starting PC.
    pub fn with_pc(bus: Bus, start_pc: Addr) -> Self {
        let mut cpu = Self::new(bus);
        cpu.pc.set(start_pc);
        cpu.fetch_stage.set_pc(start_pc);
        cpu
    }

    /// Get a reference to the registers.
    pub fn registers(&self) -> &Registers {
        &self.regs
    }

    /// Get a mutable reference to the registers.
    pub fn registers_mut(&mut self) -> &mut Registers {
        &mut self.regs
    }

    /// Get a reference to the system bus.
    pub fn bus(&self) -> &Bus {
        &self.bus
    }

    /// Get a mutable reference to the system bus.
    pub fn bus_mut(&mut self) -> &mut Bus {
        &mut self.bus
    }

    /// Get the number of cycles executed.
    pub fn cycles(&self) -> u64 {
        self.cycles
    }

    /// Get the current IPC (Instructions Per Cycle).
    pub fn ipc(&self) -> f64 {
        if self.cycles == 0 {
            0.0
        } else {
            self.instructions_executed as f64 / self.cycles as f64
        }
    }

    /// Execute one clock cycle (advance all pipeline stages).
    ///
    /// The pipeline executes stages in reverse order (WB -> MEM -> EX -> ID -> IF)
    /// to avoid overwriting pipeline register data before it's read.
    pub fn clock(&mut self) -> Result<()> {
        if self.halted {
            return Err(SimError::Halted);
        }

        // ========== Step 1: Update hazard detection ==========
        self.hazard_unit.update(&self.id_ex, &self.if_id, &self.ex_mem);

        // ========== Step 2: Execute stages in reverse order ==========
        // This prevents overwriting pipeline registers before they're read

        // 2a. Write Back stage - write result to register file
        let wb_completed = self.writeback_stage.execute(&self.mem_wb, &mut self.regs)?;
        if wb_completed {
            self.instructions_executed += 1;
        }

        // 2b. Memory stage - perform memory access
        // Use old ex_mem value, produce new mem_wb
        let new_mem_wb = self.memory_stage.execute(&self.ex_mem, &mut self.bus)?;

        // 2c. Execute stage - perform ALU operations and branch evaluation
        // Use old id_ex value and old pipeline registers for forwarding
        let new_ex_mem = self.execute_stage.execute(
            &self.id_ex,
            &self.ex_mem,
            &self.mem_wb,
            self.hazard_unit.flush_id_ex,
        )?;

        // 2d. Decode stage - decode instruction and read registers
        let new_id_ex = self.decode_stage.execute(
            &self.if_id,
            &self.regs,
            self.hazard_unit.flush_id_ex,
        )?;

        // 2e. Fetch stage - fetch instruction from memory
        // Handle stall and branch prediction
        let new_if_id = self.fetch_stage.execute(
            &self.bus,
            self.hazard_unit.stall,
            self.ex_mem.branch_target,
            self.ex_mem.branch_taken,
        )?;

        // ========== Step 3: Update pipeline registers ==========
        // Update in forward order to maintain correct state
        self.mem_wb = new_mem_wb;
        self.ex_mem = new_ex_mem;

        // ID/EX is always updated (flush_id_ex controls if it becomes a bubble)
        self.id_ex = new_id_ex;

        // IF/ID is only updated if not stalled (stall keeps old instruction in IF/ID)
        if !self.hazard_unit.stall {
            self.if_id = new_if_id;
        }

        // ========== Step 4: Update PC ==========
        // PC is updated by fetch stage internally, sync here
        if self.ex_mem.branch_taken {
            // Branch was taken - PC already updated in fetch stage
            self.pc.set(self.fetch_stage.pc());
        } else if !self.hazard_unit.stall_pc {
            // Normal case - PC advances with fetch stage
            self.pc.set(self.fetch_stage.pc());
        }

        self.cycles += 1;

        Ok(())
    }
}

impl ExecutionModel for PipelineCpu {
    fn step(&mut self) -> Result<CpuState> {
        self.clock()?;
        Ok(self.state())
    }

    fn reset(&mut self) {
        self.regs.reset();
        self.pc = ProgramCounter::zero();
        self.privilege = PrivilegeLevel::Machine;
        self.if_id = IfIdRegister::default();
        self.id_ex = IdExRegister::default();
        self.ex_mem = ExMemRegister::default();
        self.mem_wb = MemWbRegister::default();
        self.fetch_stage.reset();
        self.decode_stage.reset();
        self.execute_stage.reset();
        self.memory_stage.reset();
        self.writeback_stage.reset();
        self.hazard_unit.reset();
        self.instructions_executed = 0;
        self.cycles = 0;
        self.halted = false;
    }

    fn state(&self) -> CpuState {
        CpuState::from_cpu(
            &self.pc,
            &self.regs,
            self.privilege,
            self.instructions_executed,
            self.halted,
        )
    }

    fn pc(&self) -> Addr {
        self.pc.get()
    }

    fn set_pc(&mut self, addr: Addr) {
        self.pc.set(addr);
        self.fetch_stage.set_pc(addr);
    }

    fn is_halted(&self) -> bool {
        self.halted
    }

    fn halt(&mut self) {
        self.halted = true;
    }

    fn instructions_executed(&self) -> u64 {
        self.instructions_executed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Ram;
    use crate::Memory;

    fn create_test_bus_with_program(program: &[u32]) -> Bus {
        let mut bus = Bus::new();
        let mut ram = Ram::new(4096);
        // Fill with NOP instructions (addi x0, x0, 0 = 0x00000013)
        for i in 0..1024 {
            ram.write_word(Addr::new(i * 4), Word::new(0x00000013)).unwrap();
        }
        // Write actual program
        for (i, &instr) in program.iter().enumerate() {
            ram.write_word(Addr::new((i * 4) as u32), Word::new(instr)).unwrap();
        }
        bus.attach_memory(Addr::new(0), ram, "RAM");
        bus
    }

    #[test]
    fn test_pipeline_cpu_creation() {
        let bus = Bus::new();
        let cpu = PipelineCpu::new(bus);

        assert_eq!(cpu.pc(), Addr::new(0));
        assert_eq!(cpu.instructions_executed(), 0);
        assert!(!cpu.is_halted());
    }

    #[test]
    fn test_pipeline_with_pc() {
        let bus = Bus::new();
        let cpu = PipelineCpu::with_pc(bus, Addr::new(0x1000));

        assert_eq!(cpu.pc(), Addr::new(0x1000));
    }

    #[test]
    fn test_pipeline_reset() {
        let bus = Bus::new();
        let mut cpu = PipelineCpu::new(bus);
        cpu.set_pc(Addr::new(0x1000));
        cpu.halt();

        cpu.reset();

        assert_eq!(cpu.pc(), Addr::new(0));
        assert!(!cpu.is_halted());
        assert_eq!(cpu.instructions_executed(), 0);
    }

    #[test]
    fn test_pipeline_simple_addi_debug() {
        // ADDI x1, x0, 42 (x1 = 0 + 42 = 42)
        let program = [0x02A00093]; // addi x1, x0, 42
        let bus = create_test_bus_with_program(&program);
        let mut cpu = PipelineCpu::with_pc(bus, Addr::new(0));

        // Debug: print pipeline state BEFORE each cycle
        for cycle in 0..10 {
            println!("=== Before Cycle {} ===", cycle);
            println!("  IF/ID: pc={:08x}, instr={:08x}, valid={}",
                cpu.if_id.pc.raw(), cpu.if_id.instruction, cpu.if_id.valid);
            println!("  ID/EX: pc={:08x}, rd={}, rs1_val={}, imm={}, ctrl.reg_write={}, valid={}",
                cpu.id_ex.pc.raw(), cpu.id_ex.rd.raw(), cpu.id_ex.rs1_val.raw(), cpu.id_ex.imm,
                cpu.id_ex.ctrl.reg_write, cpu.id_ex.valid);
            println!("  EX/MEM: pc={:08x}, alu_result={:08x}, rd={}, ctrl.reg_write={}, valid={}",
                cpu.ex_mem.pc.raw(), cpu.ex_mem.alu_result.raw(), cpu.ex_mem.rd.raw(),
                cpu.ex_mem.ctrl.reg_write, cpu.ex_mem.valid);
            println!("  MEM/WB: pc={:08x}, write_data={:08x}, rd={}, ctrl.reg_write={}, valid={}",
                cpu.mem_wb.pc.raw(), cpu.mem_wb.write_data.raw(), cpu.mem_wb.rd.raw(),
                cpu.mem_wb.ctrl.reg_write, cpu.mem_wb.valid);
            println!("  HazardUnit: stall={}, flush_id_ex={}",
                cpu.hazard_unit.stall, cpu.hazard_unit.flush_id_ex);
            println!("  x1 = {}", cpu.regs.read(crate::types::RegIdx::new(1)).raw());
            println!("");

            cpu.clock().unwrap();
        }

        // x1 should now be 42
        let x1_val = cpu.registers().read(crate::types::RegIdx::new(1)).raw();
        println!("Final x1 = {}", x1_val);
        assert_eq!(x1_val, 42);
    }

    #[test]
    fn test_pipeline_simple_addi() {
        // ADDI x1, x0, 42
        let program = [0x02A00093]; // addi x1, x0, 42
        let bus = create_test_bus_with_program(&program);
        let mut cpu = PipelineCpu::with_pc(bus, Addr::new(0));

        // Run enough cycles to complete
        for _ in 0..5 {
            cpu.clock().unwrap();
        }

    }

    #[test]
    fn test_pipeline_ipc_calculation() {
        let bus = Bus::new();
        let mut cpu = PipelineCpu::new(bus);

        assert_eq!(cpu.ipc(), 0.0);

        cpu.instructions_executed = 10;
        cpu.cycles = 20;

        assert!((cpu.ipc() - 0.5).abs() < 0.001);
    }
}
