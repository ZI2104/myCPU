//! 5-Stage Pipeline Implementation.
//!
//! This module provides a 5-stage pipelined CPU implementation (IF/ID/EX/MEM/WB)
//! with hazard detection, forwarding, and branch prediction.

mod control;
mod forward;
mod hazard;
mod registers;
pub mod stages;

pub use control::{
    AluOp, AluSrc, BranchType, ExControlSignals, MemControlSignals, WbControlSignals,
};
pub use forward::{ForwardSource, ForwardUnit};
pub use hazard::HazardUnit;
pub use registers::{ExMemRegister, IdExRegister, IfIdRegister, MemWbRegister};

// Re-export for visualization
use crate::visualize::snapshot::{
    disassemble, CpuSnapshot, ExStageInfo, IdStageInfo, IfStageInfo, MemStageInfo, PerfSnapshot,
    PipelineSnapshot, WbStageInfo,
};

use crate::cpu::csr::{CsrFile, PerfEvent, HPM_COUNTER_BASE, HPM_COUNTER_COUNT};
use crate::cpu::csr::{ExceptionCause, InterruptCause, Trap};
use crate::cpu::execution_model::ExecutionModel;
use crate::cpu::mmu;
use crate::cpu::perf_collector::PerfCollector;
use crate::cpu::pipeline::stages::{
    DecodeStage, ExecuteStage, FetchStage, MemoryStage, WritebackStage,
};
use crate::cpu::{CpuState, ProgramCounter, Registers};
use crate::error::{MemoryAccessType, Result, SimError};
use crate::memory::Bus;
use crate::types::{Addr, PrivilegeLevel, Word};

/// 5-stage pipelined CPU.
#[derive(Debug)]
pub struct PipelineCpu {
    // Shared resources
    regs: Registers,
    bus: Bus,
    csr: CsrFile,
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

    // Performance tracking
    perf: PerfCollector,

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
            csr: CsrFile::new(),
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
            perf: PerfCollector::new(),
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

    /// Get a reference to the CSR file.
    pub fn csr(&self) -> &CsrFile {
        &self.csr
    }

    /// Get a mutable reference to the CSR file.
    pub fn csr_mut(&mut self) -> &mut CsrFile {
        &mut self.csr
    }

    /// Get the current privilege level.
    pub fn privilege(&self) -> PrivilegeLevel {
        self.privilege
    }

    /// Set the privilege level.
    pub fn set_privilege(&mut self, level: PrivilegeLevel) {
        self.privilege = level;
    }

    /// Get a reference to the performance collector.
    pub fn perf_collector(&self) -> &PerfCollector {
        &self.perf
    }

    /// Get a mutable reference to the performance collector.
    pub fn perf_collector_mut(&mut self) -> &mut PerfCollector {
        &mut self.perf
    }

    /// Synchronize external interrupt status from CLINT and PLIC to MIP CSR.
    ///
    /// This updates the MTIP, MSIP, and MEIP bits in mip based on the current
    /// state of the CLINT and PLIC peripherals.
    fn sync_interrupts(&mut self) {
        // Sync CLINT interrupts (Timer and Software)
        let (mtip, msip) = self.bus.get_clint_interrupt_status();
        self.csr.mip.set_mtip(mtip);
        self.csr.mip.set_msip(msip);

        // Reflect peripheral IRQ lines into PLIC pending sources.
        self.bus.sync_plic_pending_from_peripherals();

        // Sync PLIC interrupts (External)
        let (meip, _seip) = self.bus.get_plic_interrupt_status();
        self.csr.mip.set_meip(meip);
    }

    /// Check for pending interrupts and handle them if enabled.
    ///
    /// # Returns
    /// `true` if an interrupt was taken, `false` otherwise.
    fn check_and_handle_interrupt(&mut self) -> bool {
        // Check if interrupts are globally enabled (MIE bit in mstatus)
        let mie_enabled = self.csr.mstatus.mie();

        // Check if there's a pending interrupt that's also enabled
        let pending = self.csr.mip.has_pending_interrupt(&self.csr.mie);

        if mie_enabled && pending {
            // Get the highest priority pending interrupt
            if let Some((is_interrupt, cause)) =
                self.csr.mip.highest_priority_interrupt(&self.csr.mie)
            {
                // Take the trap - flush pipeline and jump to handler
                let trap = if is_interrupt {
                    Trap::interrupt(InterruptCause::from_code(cause), self.pc.get())
                } else {
                    Trap::exception(ExceptionCause::from_code(cause), self.pc.get(), 0)
                };
                self.take_trap(trap);
                return true;
            }
        }

        false
    }

    /// Take a trap and route it to M-mode or S-mode based on delegation state.
    fn take_trap(&mut self, trap: Trap) {
        // Per RISC-V spec, traps from M-mode are not delegated.
        let can_delegate = self.privilege != PrivilegeLevel::Machine
            && trap.should_delegate(&self.csr.mideleg, &self.csr.medeleg);

        let (handler_addr, new_privilege) = if can_delegate {
            trap.take_s_trap(
                &mut self.csr.sstatus,
                &mut self.csr.sepc,
                &mut self.csr.scause,
                &mut self.csr.stval,
                &self.csr.stvec,
                self.privilege,
            )
        } else {
            trap.take_m_trap(
                &mut self.csr.mstatus,
                &mut self.csr.mepc,
                &mut self.csr.mcause,
                &mut self.csr.mtval,
                &self.csr.mtvec,
                self.privilege,
            )
        };

        self.privilege = new_privilege;

        // Flush the pipeline
        self.flush_pipeline();

        // Jump to trap handler
        self.pc.set(handler_addr);
        self.fetch_stage.set_pc(handler_addr);
    }

    /// Flush the entire pipeline.
    fn flush_pipeline(&mut self) {
        // Insert bubbles into all pipeline registers
        self.if_id.valid = false;
        self.id_ex.valid = false;
        self.ex_mem.valid = false;
        self.mem_wb.valid = false;
    }

    /// Execute a trap return instruction (mret/sret/uret).
    ///
    /// This restores the PC from the appropriate EPC register and
    /// restores the previous privilege level.
    fn execute_trap_return(&mut self) -> Result<()> {
        // Determine which mode we're returning from based on the instruction
        // For simplicity, we assume MRET for now (based on privilege level)
        // The decode stage has already identified this as a trap return

        // Get return PC from mepc
        let return_pc = self.csr.mepc.get();

        // Restore privilege level from mstatus.MPP
        let new_priv = self.csr.mstatus.mpp();

        // Restore interrupt enable from mstatus.MPIE to mstatus.MIE
        let mpie = self.csr.mstatus.mpie();
        self.csr.mstatus.set_mie(mpie);

        // Set MPIE to 1 (per spec)
        self.csr.mstatus.set_mpie(true);

        // Set MPP to U-mode (0)
        self.csr.mstatus.set_mpp(PrivilegeLevel::User);

        // Update privilege level
        self.privilege = new_priv;

        // Flush the pipeline
        self.flush_pipeline();

        // Jump to return PC
        self.pc.set(return_pc);
        self.fetch_stage.set_pc(return_pc);

        Ok(())
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

    /// Create a snapshot of the CPU state for visualization.
    ///
    /// This returns a serializable snapshot containing all registers,
    /// pipeline stages, and performance counters.
    pub fn snapshot(&self) -> CpuSnapshot {
        let perf = &self.perf;
        let total_stalls = perf.total_stalls();
        let load_use_stall_rate = if perf.cycles > 0 {
            perf.load_use_stalls as f64 / perf.cycles as f64 * 100.0
        } else {
            0.0
        };
        let control_hazard_rate = if perf.cycles > 0 {
            perf.control_hazards as f64 / perf.cycles as f64 * 100.0
        } else {
            0.0
        };
        let load_use_stall_share = if total_stalls > 0 {
            perf.load_use_stalls as f64 / total_stalls as f64 * 100.0
        } else {
            0.0
        };
        let control_hazard_share = if total_stalls > 0 {
            perf.control_hazards as f64 / total_stalls as f64 * 100.0
        } else {
            0.0
        };

        CpuSnapshot {
            registers: self.regs.as_slice().try_into().unwrap_or([0; 32]),
            pc: self.pc.get().raw(),
            privilege: format!("{:?}", self.privilege),
            pipeline: PipelineSnapshot {
                if_stage: if self.if_id.valid {
                    Some(IfStageInfo {
                        pc: self.if_id.pc.raw(),
                        instruction: self.if_id.instruction,
                        instruction_str: disassemble(self.if_id.instruction),
                    })
                } else {
                    None
                },
                id_stage: if self.id_ex.valid {
                    Some(IdStageInfo {
                        pc: self.id_ex.pc.raw(),
                        rs1: self.id_ex.rs1.raw(),
                        rs2: self.id_ex.rs2.raw(),
                        rd: self.id_ex.rd.raw(),
                        rs1_val: self.id_ex.rs1_val.raw(),
                        rs2_val: self.id_ex.rs2_val.raw(),
                        imm: self.id_ex.imm,
                    })
                } else {
                    None
                },
                ex_stage: if self.ex_mem.valid {
                    Some(ExStageInfo {
                        pc: self.ex_mem.pc.raw(),
                        alu_result: self.ex_mem.alu_result.raw(),
                        rd: self.ex_mem.rd.raw(),
                        branch_taken: self.ex_mem.branch_taken,
                        branch_target: self.ex_mem.branch_target.raw(),
                        is_branch: self.ex_mem.branch_taken,
                    })
                } else {
                    None
                },
                mem_stage: if self.ex_mem.valid {
                    Some(MemStageInfo {
                        pc: self.ex_mem.pc.raw(),
                        alu_result: self.ex_mem.alu_result.raw(),
                        mem_read: self.ex_mem.ctrl.mem_read,
                        mem_write: self.ex_mem.ctrl.mem_write,
                        rd: self.ex_mem.rd.raw(),
                    })
                } else {
                    None
                },
                wb_stage: if self.mem_wb.valid {
                    Some(WbStageInfo {
                        pc: self.mem_wb.pc.raw(),
                        write_data: self.mem_wb.write_data.raw(),
                        rd: self.mem_wb.rd.raw(),
                        reg_write: self.mem_wb.ctrl.reg_write,
                    })
                } else {
                    None
                },
                stall: self.hazard_unit.stall,
                flush: self.hazard_unit.flush_id_ex,
            },
            perf: PerfSnapshot {
                cycles: perf.cycles,
                instructions: perf.instructions_retired,
                ipc: perf.ipc(),
                stalls: total_stalls,
                load_use_stalls: perf.load_use_stalls,
                control_hazards: perf.control_hazards,
                load_use_stall_rate,
                control_hazard_rate,
                load_use_stall_share,
                control_hazard_share,
                branch_accuracy: perf.branch_accuracy(),
                memory_reads: perf.memory_reads,
                memory_writes: perf.memory_writes,
            },
            halted: self.halted,
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

        // Advance CLINT timer by one cycle before sampling interrupt state.
        self.bus.tick_clint(1);

        // ========== Step 0: Synchronize interrupts from CLINT ==========
        self.sync_interrupts();

        // ========== Step 0.5: Check for pending interrupts ==========
        if self.check_and_handle_interrupt() {
            // Interrupt was taken - pipeline is flushed, skip rest of cycle
            self.cycles += 1;
            self.perf.record(PerfEvent::Cycles);
            self.perf.record(PerfEvent::InterruptsTaken);
            self.perf.record(PerfEvent::PipelineFlushes);
            // Update CSR counters
            self.csr.perf.tick();
            return Ok(());
        }

        // ========== Step 1: Update hazard detection ==========
        self.hazard_unit
            .update(&self.id_ex, &self.if_id, &self.ex_mem);

        // Record stall events
        if self.hazard_unit.stall {
            self.perf.record(PerfEvent::LoadUseStalls);
        }

        // ========== Step 2: Execute stages in reverse order ==========
        // This prevents overwriting pipeline registers before they're read

        // 2a. Write Back stage - write result to register file
        let wb_completed = self.writeback_stage.execute(&self.mem_wb, &mut self.regs)?;
        if wb_completed {
            self.instructions_executed += 1;
            self.perf.record(PerfEvent::InstructionsRetired);
            self.csr.perf.instruction_retired();
        }

        // 2b. Memory stage - perform memory access
        // Use old ex_mem value, produce new mem_wb
        let satp_for_mem = self.csr.satp;
        let privilege_for_mem = self.privilege;
        let new_mem_wb = match self.memory_stage.execute_with_translate(
            &self.ex_mem,
            &mut self.bus,
            |bus, vaddr, access| {
                mmu::translate_addr(bus, &satp_for_mem, privilege_for_mem, vaddr, access)
            },
        ) {
            Ok(mem_wb) => mem_wb,
            Err(SimError::PageFault { addr, access }) => {
                self.take_trap(Trap::exception(
                    Self::page_fault_cause(access),
                    self.ex_mem.pc,
                    addr.raw(),
                ));
                self.cycles += 1;
                self.perf.record(PerfEvent::Cycles);
                self.perf.record(PerfEvent::PipelineFlushes);
                self.csr.perf.tick();
                return Ok(());
            }
            Err(err) => return Err(err),
        };

        // Track memory accesses
        if self.ex_mem.ctrl.mem_read {
            self.perf.record(PerfEvent::MemoryReads);
        }
        if self.ex_mem.ctrl.mem_write {
            self.perf.record(PerfEvent::MemoryWrites);
        }

        // 2c. Execute stage - perform ALU operations and branch evaluation
        // Use old id_ex value and old pipeline registers for forwarding
        let mut new_ex_mem = self.execute_stage.execute(
            &self.id_ex,
            &self.ex_mem,
            &self.mem_wb,
            self.hazard_unit.flush_id_ex,
        )?;

        // Track ALU operations and branches
        if self.id_ex.valid && self.id_ex.ctrl.alu_op != AluOp::Nop {
            self.perf.record(PerfEvent::AluOperations);
        }
        if self.id_ex.ctrl.branch {
            self.perf.record(PerfEvent::BranchExecuted);
            if new_ex_mem.branch_taken {
                self.perf.record(PerfEvent::BranchTaken);
                self.perf.record(PerfEvent::ControlHazards);
            } else {
                self.perf.record(PerfEvent::BranchNotTaken);
            }
        }

        // 2c-2. Handle CSR instructions and trap returns
        if self.id_ex.ctrl.csr_op {
            // Execute CSR operation
            let rs1_val = self.id_ex.rs1_val.raw();
            let csr_result = self.csr.execute(
                self.id_ex.ctrl.csr_op_type,
                self.id_ex.ctrl.csr_addr,
                rs1_val,
                self.privilege,
            )?;
            // Override ALU result with CSR read value
            new_ex_mem.alu_result = Word::new(csr_result);
            self.perf.record(PerfEvent::CsrAccesses);
        } else if self.id_ex.ctrl.trap_return {
            // Handle trap return (mret/sret/uret)
            self.execute_trap_return()?;
            // Flush the pipeline after trap return
            new_ex_mem.valid = false;
            self.perf.record(PerfEvent::PipelineFlushes);
        }

        // 2d. Decode stage - decode instruction and read registers
        let new_id_ex =
            self.decode_stage
                .execute(&self.if_id, &self.regs, self.hazard_unit.flush_id_ex)?;

        // 2e. Fetch stage - fetch instruction from memory
        // Handle stall and branch prediction
        let satp_for_if = self.csr.satp;
        let privilege_for_if = self.privilege;
        let new_if_id = match self.fetch_stage.execute_with_translate(
            &self.bus,
            self.hazard_unit.stall,
            self.ex_mem.branch_target,
            self.ex_mem.branch_taken,
            |bus, vaddr| {
                mmu::translate_addr(
                    bus,
                    &satp_for_if,
                    privilege_for_if,
                    vaddr,
                    MemoryAccessType::Instruction,
                )
            },
        ) {
            Ok(if_id) => if_id,
            Err(SimError::PageFault { addr, access }) => {
                self.take_trap(Trap::exception(
                    Self::page_fault_cause(access),
                    self.fetch_stage.pc(),
                    addr.raw(),
                ));
                self.cycles += 1;
                self.perf.record(PerfEvent::Cycles);
                self.perf.record(PerfEvent::PipelineFlushes);
                self.csr.perf.tick();
                return Ok(());
            }
            Err(err) => return Err(err),
        };

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

        // ========== Step 5: Update performance counters ==========
        self.cycles += 1;
        self.perf.record(PerfEvent::Cycles);
        self.csr.perf.tick();

        // Record events to HPM counters (skip if all inhibited)
        if !self.csr.perf.mcountinhibit.hpms() {
            for i in 0..HPM_COUNTER_COUNT {
                // Skip if this specific counter is inhibited
                if self.csr.perf.mcountinhibit.hpm(i + HPM_COUNTER_BASE) {
                    continue;
                }

                let event = self.csr.perf.mhpmevents[i].event();
                if event == PerfEvent::None {
                    continue;
                }

                // Check if this event occurred this cycle
                match event {
                    PerfEvent::Cycles => self.csr.perf.mhpmcounters[i].increment(),
                    PerfEvent::InstructionsRetired if wb_completed => {
                        self.csr.perf.mhpmcounters[i].increment()
                    }
                    PerfEvent::LoadUseStalls if self.hazard_unit.stall => {
                        self.csr.perf.mhpmcounters[i].increment()
                    }
                    PerfEvent::ControlHazards if self.ex_mem.branch_taken => {
                        self.csr.perf.mhpmcounters[i].increment()
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    fn page_fault_cause(access: MemoryAccessType) -> ExceptionCause {
        match access {
            MemoryAccessType::Instruction => ExceptionCause::InstructionPageFault,
            MemoryAccessType::Load => ExceptionCause::LoadPageFault,
            MemoryAccessType::Store => ExceptionCause::StorePageFault,
        }
    }
}

impl ExecutionModel for PipelineCpu {
    fn step(&mut self) -> Result<CpuState> {
        self.clock()?;
        Ok(self.state())
    }

    fn reset(&mut self) {
        self.regs.reset();
        self.csr.reset();
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
        self.perf.reset();
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
            ram.write_word(Addr::new(i * 4), Word::new(0x00000013))
                .unwrap();
        }
        // Write actual program
        for (i, &instr) in program.iter().enumerate() {
            ram.write_word(Addr::new((i * 4) as u32), Word::new(instr))
                .unwrap();
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
            println!(
                "  IF/ID: pc={:08x}, instr={:08x}, valid={}",
                cpu.if_id.pc.raw(),
                cpu.if_id.instruction,
                cpu.if_id.valid
            );
            println!(
                "  ID/EX: pc={:08x}, rd={}, rs1_val={}, imm={}, ctrl.reg_write={}, valid={}",
                cpu.id_ex.pc.raw(),
                cpu.id_ex.rd.raw(),
                cpu.id_ex.rs1_val.raw(),
                cpu.id_ex.imm,
                cpu.id_ex.ctrl.reg_write,
                cpu.id_ex.valid
            );
            println!(
                "  EX/MEM: pc={:08x}, alu_result={:08x}, rd={}, ctrl.reg_write={}, valid={}",
                cpu.ex_mem.pc.raw(),
                cpu.ex_mem.alu_result.raw(),
                cpu.ex_mem.rd.raw(),
                cpu.ex_mem.ctrl.reg_write,
                cpu.ex_mem.valid
            );
            println!(
                "  MEM/WB: pc={:08x}, write_data={:08x}, rd={}, ctrl.reg_write={}, valid={}",
                cpu.mem_wb.pc.raw(),
                cpu.mem_wb.write_data.raw(),
                cpu.mem_wb.rd.raw(),
                cpu.mem_wb.ctrl.reg_write,
                cpu.mem_wb.valid
            );
            println!(
                "  HazardUnit: stall={}, flush_id_ex={}",
                cpu.hazard_unit.stall, cpu.hazard_unit.flush_id_ex
            );
            println!(
                "  x1 = {}",
                cpu.regs.read(crate::types::RegIdx::new(1)).raw()
            );
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
