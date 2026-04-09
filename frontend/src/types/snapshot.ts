// CPU state types for visualization

export interface CpuSnapshot {
  registers: number[];
  pc: number;
  privilege: string;
  pipeline: PipelineSnapshot;
  perf: PerfSnapshot;
  predictor?: PredictorSnapshot | null;
  halted: boolean;
  // Incremented by server on Reset so frontend can detect and prioritize
  // reset-aligned snapshots. Optional for backward compatibility.
  reset_sequence?: number;
}

export interface PipelineSnapshot {
  pre_if_stage: PreIfStageInfo | null;
  if_stage: IfStageInfo | null;
  id_stage: IdStageInfo | null;
  ex_stage: ExStageInfo | null;
  mem_stage: MemStageInfo | null;
  wb_stage: WbStageInfo | null;
  stall: boolean;
  flush: boolean;
}

export interface PreIfStageInfo {
  next_pc: number;
  fetch_addr: number;
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

// Branch Predictor
export interface PredictorSnapshot {
  predictor_type: string;
  predictor_display_name: string;
  predictions: number;
  correct: number;
  mispredictions: number;
  accuracy: number | null;
  btb_lookups: number;
  btb_hits: number;
  btb_misses: number;
  btb_hit_rate: number | null;
  btb_entries: BtbEntrySnapshot[];
}

export interface BtbEntrySnapshot {
  tag: number;
  target: number;
  is_branch: boolean;
}

// Predictor types for the switch command
export const PREDICTOR_TYPES = [
  { value: 'none', label: 'Always Not Taken' },
  { value: 'one_bit', label: '1-Bit' },
  { value: 'two_bit', label: '2-Bit Saturating' },
  { value: 'local', label: 'Local (2-Level)' },
  { value: 'global', label: 'Global (gshare)' },
] as const;

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

export interface NpuStateResponse {
  type: 'npu_state';
  success: boolean;
  control?: number;
  status?: number;
  opcode?: number;
  cycles?: number;
  desc_addr?: number;
  desc_len?: number;
  tasks_done?: number;
  tasks_error?: number;
  desc_notify_count?: number;
  pending_desc_notify?: boolean;
  error?: string;
}

export interface LpuStateResponse {
  type: 'lpu_state';
  success: boolean;
  control?: number;
  status?: number;
  opcode?: number;
  cycles?: number;
  desc_addr?: number;
  desc_len?: number;
  tasks_done?: number;
  tasks_error?: number;
  desc_notify_count?: number;
  pending_desc_notify?: boolean;
  error?: string;
}

export interface GpuStateResponse {
  type: 'gpu_state';
  success: boolean;
  control?: number;
  status?: number;
  kernel_type?: number;
  precision?: number;
  kernels_executed?: number;
  cycles?: number;
  ops_count?: number;
  bytes_transferred?: number;
  tasks_done?: number;
  tasks_error?: number;
  error_code?: number;
  work_queue_len?: number;
  conv_kernel_size?: number;
  conv_stride?: number;
  conv_padding?: number;
  conv_input_dims?: number;
  conv_channels?: number;
  error?: string;
}

export interface TpuStateResponse {
  type: 'tpu_state';
  success: boolean;
  control?: number;
  status?: number;
  kernel_type?: number;
  m?: number;
  n?: number;
  k?: number;
  matrices_computed?: number;
  cycles?: number;
  ops_count?: number;
  tasks_done?: number;
  tasks_error?: number;
  error_code?: number;
  input_scale?: number;
  output_scale?: number;
  input_zero_point?: number;
  output_zero_point?: number;
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
