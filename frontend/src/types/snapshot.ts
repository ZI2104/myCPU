// CPU state types for visualization

export interface CpuSnapshot {
  registers: number[];
  pc: number;
  privilege: string;
  pipeline: PipelineSnapshot;
  perf: PerfSnapshot;
  halted: boolean;
}

export interface PipelineSnapshot {
  if_stage: IfStageInfo | null;
  id_stage: IdStageInfo | null;
  ex_stage: ExStageInfo | null;
  mem_stage: MemStageInfo | null;
  wb_stage: WbStageInfo | null;
  stall: boolean;
  flush: boolean;
}

export interface IfStageInfo {
  pc: number;
  instruction: number;
  instruction_str: string;
}

export interface IdStageInfo {
  pc: number;
  rs1: number;
  rs2: number;
  rd: number;
  rs1_val: number;
  rs2_val: number;
  imm: number;
}

export interface ExStageInfo {
  pc: number;
  alu_result: number;
  rd: number;
  branch_taken: boolean;
  branch_target: number;
  is_branch: boolean;
}

export interface MemStageInfo {
  pc: number;
  alu_result: number;
  mem_read: boolean;
  mem_write: boolean;
  rd: number;
}

export interface WbStageInfo {
  pc: number;
  write_data: number;
  rd: number;
  reg_write: boolean;
}

export interface PerfSnapshot {
  cycles: number;
  instructions: number;
  ipc: number;
  stalls: number;
  load_use_stalls: number;
  control_hazards: number;
  load_use_stall_rate: number;
  control_hazard_rate: number;
  load_use_stall_share: number;
  control_hazard_share: number;
  branch_accuracy: number | null;
  memory_reads: number;
  memory_writes: number;
}

// Memory
export interface MemoryReadResponse {
  addr: number;
  data: number[];
  success: boolean;
  error: string | null;
}

export interface FramebufferResponse {
  type: 'framebuffer';
  addr: number;
  width: number;
  height: number;
  format: string;
  pixels: number[];
  success: boolean;
  error: string | null;
}

export interface InputStateResponse {
  type: 'input_state';
  success: boolean;
  base_addr?: number;
  key_state?: number;
  last_event?: number;
  event_count?: number;
  irq_pending?: boolean;
  error?: string;
}

export interface FramebufferGameResponse {
  type: 'framebuffer_game';
  success: boolean;
  action: string;
  tick?: number;
  key_state?: number;
  ball_x?: number;
  ball_y?: number;
  vel_x?: number;
  vel_y?: number;
  left_paddle_y?: number;
  right_paddle_y?: number;
  score_left?: number;
  score_right?: number;
  error?: string;
}

// Breakpoints
export interface Breakpoint {
  addr: number;
  enabled: boolean;
  label: string | null;
  hit_count: number;
}

export interface BreakpointListResponse {
  type: 'breakpoint_list';
  breakpoints: Breakpoint[];
}

// Disassembly
export interface DisassembledInstruction {
  addr: number;
  bytes: number[];
  instruction: string;
  has_breakpoint: boolean;
}

export interface DisassemblyResponse {
  base_addr: number;
  instructions: DisassembledInstruction[];
  success: boolean;
  error: string | null;
}

// History
export interface HistoryRecord {
  cycle: number;
  pc: number;
  instruction: number | null;
  instruction_str: string | null;
  reg_changes: [number, number][];
  mem_changes: [number, number][];
}

export interface HistoryResponse {
  records: HistoryRecord[];
  total: number;
  position: number;
}

// Register names
export const REGISTER_NAMES = [
  'zero', 'ra', 'sp', 'gp', 'tp', 't0', 't1', 't2',
  's0', 's1', 'a0', 'a1', 'a2', 'a3', 'a4', 'a5',
  'a6', 'a7', 's2', 's3', 's4', 's5', 's6', 's7',
  's8', 's9', 's10', 's11', 't3', 't4', 't5', 't6'
];
