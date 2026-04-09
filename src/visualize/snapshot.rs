//! CPU state snapshot for frontend visualization.
//!
//! This module provides serializable structures that capture the complete
//! state of the CPU at a point in time, suitable for WebSocket transmission
//! to the frontend visualizer.

use serde::{Deserialize, Serialize};

/// CPU state snapshot for visualization.
#[derive(Debug, Clone, Serialize)]
pub struct CpuSnapshot {
    /// General-purpose registers x0-x31
    pub registers: [u32; 32],
    /// Program counter
    pub pc: u32,
    /// Current privilege level
    pub privilege: String,
    /// Pipeline state
    pub pipeline: PipelineSnapshot,
    /// Performance counters
    pub perf: PerfSnapshot,
    /// Whether the CPU is halted
    pub halted: bool,
    /// Reset sequence counter - increments on each Reset to help frontend
    /// detect and prioritize reset-aligned snapshots.
    pub reset_sequence: u64,
}

/// Pipeline state snapshot.
#[derive(Debug, Clone, Serialize)]
pub struct PipelineSnapshot {
    /// pre-IF stage information (synchronous RAM read address)
    pub pre_if_stage: Option<PreIfStageInfo>,
    /// IF stage information
    pub if_stage: Option<IfStageInfo>,
    /// ID stage information
    pub id_stage: Option<IdStageInfo>,
    /// EX stage information
    pub ex_stage: Option<ExStageInfo>,
    /// MEM stage information
    pub mem_stage: Option<MemStageInfo>,
    /// WB stage information
    pub wb_stage: Option<WbStageInfo>,
    /// Whether the pipeline is stalled (Load-Use hazard)
    pub stall: bool,
    /// Whether the pipeline is being flushed
    pub flush: bool,
}

/// pre-IF (Instruction Fetch Request) stage information.
///
/// Shows the synchronous RAM read address being presented in the current cycle.
#[derive(Debug, Clone, Serialize)]
pub struct PreIfStageInfo {
    /// The nextPC value being used as the RAM read address
    pub next_pc: u32,
    /// The instruction being fetched (if already in latch)
    pub fetch_addr: u32,
}

/// IF (Instruction Fetch) stage information.
#[derive(Debug, Clone, Serialize)]
pub struct IfStageInfo {
    /// PC of the instruction
    pub pc: u32,
    /// Raw instruction word
    pub instruction: u32,
    /// Disassembled instruction string
    pub instruction_str: String,
}

/// ID (Instruction Decode) stage information.
#[derive(Debug, Clone, Serialize)]
pub struct IdStageInfo {
    /// PC of the instruction
    pub pc: u32,
    /// Source register 1 index
    pub rs1: u8,
    /// Source register 2 index
    pub rs2: u8,
    /// Destination register index
    pub rd: u8,
    /// Value of rs1
    pub rs1_val: u32,
    /// Value of rs2
    pub rs2_val: u32,
    /// Immediate value
    pub imm: i32,
}

/// EX (Execute) stage information.
#[derive(Debug, Clone, Serialize)]
pub struct ExStageInfo {
    /// PC of the instruction
    pub pc: u32,
    /// ALU result
    pub alu_result: u32,
    /// Destination register index
    pub rd: u8,
    /// Whether a branch was taken
    pub branch_taken: bool,
    /// Branch/jump target address
    pub branch_target: u32,
    /// Whether this is a branch instruction
    pub is_branch: bool,
}

/// MEM (Memory) stage information.
#[derive(Debug, Clone, Serialize)]
pub struct MemStageInfo {
    /// PC of the instruction
    pub pc: u32,
    /// ALU result (memory address for loads/stores)
    pub alu_result: u32,
    /// Whether this is a memory read
    pub mem_read: bool,
    /// Whether this is a memory write
    pub mem_write: bool,
    /// Destination register index
    pub rd: u8,
}

/// WB (Write Back) stage information.
#[derive(Debug, Clone, Serialize)]
pub struct WbStageInfo {
    /// PC of the instruction
    pub pc: u32,
    /// Data to write back
    pub write_data: u32,
    /// Destination register index
    pub rd: u8,
    /// Whether to write to register file
    pub reg_write: bool,
}

/// Memory region snapshot for visualization.
#[derive(Debug, Clone, Serialize)]
pub struct MemorySnapshot {
    /// Base address of this region
    pub base_addr: u32,
    /// Memory data (as hex string for efficient transfer)
    pub data: Vec<u8>,
    /// Number of bytes in this snapshot
    pub size: usize,
}

/// Memory read request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryReadRequest {
    /// Base address to read from
    pub addr: u32,
    /// Number of bytes to read
    pub size: usize,
}

/// Memory read response.
#[derive(Debug, Clone, Serialize)]
pub struct MemoryReadResponse {
    /// Base address
    pub addr: u32,
    /// Memory data
    pub data: Vec<u8>,
    /// Whether the read was successful
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
}

/// Framebuffer read response.
#[derive(Debug, Clone, Serialize)]
pub struct FramebufferResponse {
    /// Response type discriminator.
    #[serde(rename = "type")]
    pub response_type: String,
    /// Framebuffer base address.
    pub addr: u32,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Source pixel format.
    pub format: String,
    /// RGBA8888 pixel bytes.
    pub pixels: Vec<u8>,
    /// Whether the read was successful.
    pub success: bool,
    /// Error message if failed.
    pub error: Option<String>,
}

/// Breakpoint information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breakpoint {
    /// Breakpoint address
    pub addr: u32,
    /// Whether the breakpoint is enabled
    pub enabled: bool,
    /// Optional label for the breakpoint
    pub label: Option<String>,
    /// Hit count
    pub hit_count: u32,
}

/// Disassembled instruction.
#[derive(Debug, Clone, Serialize)]
pub struct DisassembledInstruction {
    /// Instruction address
    pub addr: u32,
    /// Raw instruction bytes
    pub bytes: Vec<u8>,
    /// Disassembled instruction string
    pub instruction: String,
    /// Whether there's a breakpoint at this address
    pub has_breakpoint: bool,
}

/// Disassembly view response.
#[derive(Debug, Clone, Serialize)]
pub struct DisassemblyResponse {
    /// Base address of the disassembly
    pub base_addr: u32,
    /// List of disassembled instructions
    pub instructions: Vec<DisassembledInstruction>,
    /// Whether the response is successful
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
}

/// History record for a single cycle.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryRecord {
    /// Cycle number
    pub cycle: u64,
    /// PC at this cycle
    pub pc: u32,
    /// Instruction executed (if any)
    pub instruction: Option<u32>,
    /// Disassembled instruction string
    pub instruction_str: Option<String>,
    /// Register changes (reg_idx -> new_value)
    pub reg_changes: Vec<(u8, u32)>,
    /// Memory changes (addr -> new_value)
    pub mem_changes: Vec<(u32, u8)>,
}

/// History response.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryResponse {
    /// List of history records
    pub records: Vec<HistoryRecord>,
    /// Total records available
    pub total: usize,
    /// Current position in history
    pub position: usize,
}

/// Performance counters snapshot.
#[derive(Debug, Clone, Serialize)]
pub struct PerfSnapshot {
    /// Total cycles executed
    pub cycles: u64,
    /// Total instructions retired
    pub instructions: u64,
    /// Instructions Per Cycle
    pub ipc: f64,
    /// Total stall cycles
    pub stalls: u64,
    /// Load-Use stall count
    pub load_use_stalls: u64,
    /// Control hazard count
    pub control_hazards: u64,
    /// Load-Use stall rate among cycles (percentage)
    pub load_use_stall_rate: f64,
    /// Control hazard rate among cycles (percentage)
    pub control_hazard_rate: f64,
    /// Load-Use share among total stalls (percentage)
    pub load_use_stall_share: f64,
    /// Control hazard share among total stalls (percentage)
    pub control_hazard_share: f64,
    /// Branch prediction accuracy (percentage)
    pub branch_accuracy: Option<f64>,
    /// Memory read count
    pub memory_reads: u64,
    /// Memory write count
    pub memory_writes: u64,
}

impl CpuSnapshot {
    /// Create a new CPU snapshot with default values.
    pub fn new() -> Self {
        Self {
            registers: [0; 32],
            pc: 0,
            privilege: "Machine".to_string(),
            pipeline: PipelineSnapshot::default(),
            perf: PerfSnapshot::default(),
            halted: false,
            reset_sequence: 0,
        }
    }
}

impl Default for CpuSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for PipelineSnapshot {
    fn default() -> Self {
        Self {
            pre_if_stage: None,
            if_stage: None,
            id_stage: None,
            ex_stage: None,
            mem_stage: None,
            wb_stage: None,
            stall: false,
            flush: false,
        }
    }
}

impl Default for PerfSnapshot {
    fn default() -> Self {
        Self {
            cycles: 0,
            instructions: 0,
            ipc: 0.0,
            stalls: 0,
            load_use_stalls: 0,
            control_hazards: 0,
            load_use_stall_rate: 0.0,
            control_hazard_rate: 0.0,
            load_use_stall_share: 0.0,
            control_hazard_share: 0.0,
            branch_accuracy: None,
            memory_reads: 0,
            memory_writes: 0,
        }
    }
}

/// Disassemble a RISC-V instruction into a human-readable string.
pub fn disassemble(instruction: u32) -> String {
    let opcode = instruction & 0x7F;
    let rd = ((instruction >> 7) & 0x1F) as u8;
    let funct3 = (instruction >> 12) & 0x7;
    let rs1 = ((instruction >> 15) & 0x1F) as u8;
    let rs2 = ((instruction >> 20) & 0x1F) as u8;
    let funct7 = (instruction >> 25) & 0x7F;

    // Decode immediate values
    let imm_i = ((instruction >> 20) as i32) << 20 >> 20;
    let imm_s =
        (((instruction >> 25) as i32) << 7 | ((instruction >> 7) & 0x1F) as i32) << 20 >> 18 >> 2;
    let imm_b = {
        let imm_12 = ((instruction >> 31) & 1) as i32;
        let imm_10_5 = ((instruction >> 25) & 0x3F) as i32;
        let imm_4_1 = ((instruction >> 8) & 0xF) as i32;
        let imm_11 = ((instruction >> 7) & 1) as i32;
        let imm = (imm_12 << 12) | (imm_11 << 11) | (imm_10_5 << 5) | (imm_4_1 << 1);
        (imm << 19) >> 19
    };
    let imm_u = instruction & 0xFFFFF000;
    let imm_j = {
        let imm_20 = ((instruction >> 31) & 1) as i32;
        let imm_10_1 = ((instruction >> 21) & 0x3FF) as i32;
        let imm_11 = ((instruction >> 20) & 1) as i32;
        let imm_19_12 = ((instruction >> 12) & 0xFF) as i32;
        let imm = (imm_20 << 20) | (imm_19_12 << 12) | (imm_11 << 11) | (imm_10_1 << 1);
        (imm << 11) >> 11
    };

    match opcode {
        0x37 => format!("lui x{}, 0x{:05X}", rd, imm_u >> 12),
        0x17 => format!("auipc x{}, 0x{:05X}", rd, imm_u >> 12),
        0x6F => format!("jal x{}, {}", rd, imm_j),
        0x67 => {
            if funct3 == 0 {
                format!("jalr x{}, x{}, {}", rd, rs1, imm_i)
            } else {
                format!("unknown")
            }
        }
        0x63 => {
            let branch_name = match funct3 {
                0 => "beq",
                1 => "bne",
                4 => "blt",
                5 => "bge",
                6 => "bltu",
                7 => "bgeu",
                _ => "unknown",
            };
            format!("{} x{}, x{}, {}", branch_name, rs1, rs2, imm_b)
        }
        0x03 => {
            let load_name = match funct3 {
                0 => "lb",
                1 => "lh",
                2 => "lw",
                4 => "lbu",
                5 => "lhu",
                _ => "unknown",
            };
            format!("{} x{}, {}(x{})", load_name, rd, imm_i, rs1)
        }
        0x23 => {
            let store_name = match funct3 {
                0 => "sb",
                1 => "sh",
                2 => "sw",
                _ => "unknown",
            };
            format!("{} x{}, {}(x{})", store_name, rs2, imm_s, rs1)
        }
        0x13 => match funct3 {
            0 => format!("addi x{}, x{}, {}", rd, rs1, imm_i),
            2 => format!("slti x{}, x{}, {}", rd, rs1, imm_i),
            3 => format!("sltiu x{}, x{}, {:#x}", rd, rs1, imm_i as u32),
            4 => format!("xori x{}, x{}, {:#x}", rd, rs1, imm_i as u32),
            6 => format!("ori x{}, x{}, {:#x}", rd, rs1, imm_i as u32),
            7 => format!("andi x{}, x{}, {:#x}", rd, rs1, imm_i as u32),
            1 => format!("slli x{}, x{}, {}", rd, rs1, rs2),
            5 => {
                if funct7 == 0 {
                    format!("srli x{}, x{}, {}", rd, rs1, rs2)
                } else {
                    format!("srai x{}, x{}, {}", rd, rs1, rs2)
                }
            }
            _ => format!("unknown"),
        },
        0x33 => match funct3 {
            0 => match funct7 {
                0x00 => format!("add x{}, x{}, x{}", rd, rs1, rs2),
                0x20 => format!("sub x{}, x{}, x{}", rd, rs1, rs2),
                0x01 => format!("mul x{}, x{}, x{}", rd, rs1, rs2),
                _ => format!("unknown"),
            },
            1 => match funct7 {
                0x00 => format!("sll x{}, x{}, x{}", rd, rs1, rs2),
                0x01 => format!("mulh x{}, x{}, x{}", rd, rs1, rs2),
                _ => format!("unknown"),
            },
            2 => match funct7 {
                0x00 => format!("slt x{}, x{}, x{}", rd, rs1, rs2),
                0x01 => format!("mulhsu x{}, x{}, x{}", rd, rs1, rs2),
                _ => format!("unknown"),
            },
            3 => match funct7 {
                0x00 => format!("sltu x{}, x{}, x{}", rd, rs1, rs2),
                0x01 => format!("mulhu x{}, x{}, x{}", rd, rs1, rs2),
                _ => format!("unknown"),
            },
            4 => match funct7 {
                0x00 => format!("xor x{}, x{}, x{}", rd, rs1, rs2),
                0x01 => format!("div x{}, x{}, x{}", rd, rs1, rs2),
                _ => format!("unknown"),
            },
            5 => match funct7 {
                0x00 => format!("srl x{}, x{}, x{}", rd, rs1, rs2),
                0x20 => format!("sra x{}, x{}, x{}", rd, rs1, rs2),
                0x01 => format!("divu x{}, x{}, x{}", rd, rs1, rs2),
                _ => format!("unknown"),
            },
            6 => match funct7 {
                0x00 => format!("or x{}, x{}, x{}", rd, rs1, rs2),
                0x01 => format!("rem x{}, x{}, x{}", rd, rs1, rs2),
                _ => format!("unknown"),
            },
            7 => match funct7 {
                0x00 => format!("and x{}, x{}, x{}", rd, rs1, rs2),
                0x01 => format!("remu x{}, x{}, x{}", rd, rs1, rs2),
                _ => format!("unknown"),
            },
            _ => format!("unknown"),
        },
        0x0F => {
            if instruction == 0x0000000F {
                "fence".to_string()
            } else {
                format!("fence")
            }
        }
        0x73 => match funct3 {
            0 => {
                if instruction == 0x00000073 {
                    "ecall".to_string()
                } else if instruction == 0x00100073 {
                    "ebreak".to_string()
                } else if funct7 == 0x30 {
                    "mret".to_string()
                } else if funct7 == 0x25 {
                    "sret".to_string()
                } else {
                    format!("system")
                }
            }
            1 => format!("csrrw x{}, csr:{:#x}, x{}", rd, imm_i as u32, rs1),
            2 => format!("csrrs x{}, csr:{:#x}, x{}", rd, imm_i as u32, rs1),
            3 => format!("csrrc x{}, csr:{:#x}, x{}", rd, imm_i as u32, rs1),
            5 => format!("csrrwi x{}, csr:{:#x}, {}", rd, imm_i as u32, rs1),
            6 => format!("csrrsi x{}, csr:{:#x}, {}", rd, imm_i as u32, rs1),
            7 => format!("csrrci x{}, csr:{:#x}, {}", rd, imm_i as u32, rs1),
            _ => format!("unknown"),
        },
        0x00 => "nop".to_string(),
        _ => format!("unknown ({:#04x})", opcode),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disassemble_addi() {
        let instr = 0x02A00093;
        assert!(disassemble(instr).contains("addi"));
    }

    #[test]
    fn test_disassemble_lui() {
        let instr = 0x123450B7;
        assert!(disassemble(instr).contains("lui"));
    }

    #[test]
    fn test_snapshot_default() {
        let snapshot = CpuSnapshot::default();
        assert_eq!(snapshot.pc, 0);
        assert_eq!(snapshot.registers.len(), 32);
        assert!(!snapshot.halted);
    }
}
