//! WebSocket server for CPU visualization.
//!
//! This module provides a WebSocket server that allows frontend clients
//! to connect and control the CPU execution, receiving real-time state updates.

use crate::cpu::ExecutionModel;
use crate::cpu::pipeline::PipelineCpu;
use crate::error::Result;
use crate::types::Addr;
use crate::visualize::snapshot::{
    disassemble, Breakpoint, CpuSnapshot, DisassemblyResponse, DisassembledInstruction,
    HistoryRecord, HistoryResponse, MemoryReadResponse,
};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, Mutex};
use tokio_tungstenite::{accept_async, tungstenite::Message};

/// Default base address for disassembly
const DEFAULT_BASE_ADDR: u32 = 0x80000000;

// ============================================================================
// Command Types
// ============================================================================

/// Parsed command from client.
#[derive(Debug, Clone, PartialEq)]
enum Command {
    /// Get current CPU state
    State,
    /// Execute single step
    Step,
    /// Start continuous execution
    Run,
    /// Pause execution
    Pause,
    /// Reset CPU
    Reset,
    /// Set execution speed
    Speed { value: u32 },
    /// Read memory region
    Memory { addr: u32, size: usize },
    /// Add breakpoint
    BreakpointAdd { addr: u32, label: Option<String> },
    /// Remove breakpoint
    BreakpointRemove { addr: u32 },
    /// List all breakpoints
    BreakpointList,
    /// Disassemble instructions
    Disassemble { addr: u32, count: usize },
    /// Get execution history
    History { start: usize, count: usize },
}

impl Command {
    /// Parse a command string into a Command enum.
    fn parse(input: &str) -> Option<Self> {
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }

        match parts[0] {
            "state" => Some(Command::State),
            "step" => Some(Command::Step),
            "run" => Some(Command::Run),
            "pause" => Some(Command::Pause),
            "reset" => Some(Command::Reset),
            "speed" => {
                parts.get(1)
                    .and_then(|s| s.parse::<u32>().ok())
                    .map(|value| Command::Speed { value })
            }
            "memory" => {
                if parts.len() >= 3 {
                    let addr = u32::from_str_radix(parts[1].trim_start_matches("0x"), 16).ok()?;
                    let size = parts[2].parse::<usize>().ok()?;
                    Some(Command::Memory { addr, size })
                } else {
                    None
                }
            }
            "breakpoint_add" | "bp_add" => {
                if parts.len() >= 2 {
                    let addr = u32::from_str_radix(parts[1].trim_start_matches("0x"), 16).ok()?;
                    let label = parts.get(2).map(|s| s.to_string());
                    Some(Command::BreakpointAdd { addr, label })
                } else {
                    None
                }
            }
            "breakpoint_remove" | "bp_remove" => {
                if parts.len() >= 2 {
                    let addr = u32::from_str_radix(parts[1].trim_start_matches("0x"), 16).ok()?;
                    Some(Command::BreakpointRemove { addr })
                } else {
                    None
                }
            }
            "breakpoint_list" | "bp_list" => Some(Command::BreakpointList),
            "disassemble" | "disasm" => {
                let addr = if parts.len() >= 2 {
                    u32::from_str_radix(parts[1].trim_start_matches("0x"), 16)
                        .unwrap_or(DEFAULT_BASE_ADDR)
                } else {
                    DEFAULT_BASE_ADDR
                };
                let count = if parts.len() >= 3 {
                    parts[2].parse::<usize>().unwrap_or(20)
                } else {
                    20
                };
                Some(Command::Disassemble { addr, count })
            }
            "history" => {
                let start = if parts.len() >= 2 {
                    parts[1].parse::<usize>().unwrap_or(0)
                } else {
                    0
                };
                let count = if parts.len() >= 3 {
                    parts[2].parse::<usize>().unwrap_or(100)
                } else {
                    100
                };
                Some(Command::History { start, count })
            }
            _ => None,
        }
    }
}

/// Type alias for the WebSocket sender.
type WsSender = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    Message,
>;

/// Context for command execution.
///
/// Groups all shared state needed for command handling into a single struct,
/// reducing function parameter count and improving maintainability.
pub struct CommandContext {
    /// CPU instance
    pub cpu: Arc<Mutex<PipelineCpu>>,
    /// Running state flag
    pub running: Arc<Mutex<bool>>,
    /// Execution speed (cycles per second)
    pub speed: Arc<Mutex<u32>>,
    /// WebSocket message sender
    pub tx: WsSender,
    /// Breakpoints map
    pub breakpoints: Arc<Mutex<HashMap<u32, Breakpoint>>>,
    /// Execution history
    pub history: Arc<Mutex<Vec<HistoryRecord>>>,
}

impl CommandContext {
    /// Create a new command context.
    pub fn new(
        cpu: Arc<Mutex<PipelineCpu>>,
        running: Arc<Mutex<bool>>,
        speed: Arc<Mutex<u32>>,
        tx: WsSender,
        breakpoints: Arc<Mutex<HashMap<u32, Breakpoint>>>,
        history: Arc<Mutex<Vec<HistoryRecord>>>,
    ) -> Self {
        Self { cpu, running, speed, tx, breakpoints, history }
    }

    /// Send a JSON response to the client.
    async fn send_json(&mut self, json: String) {
        let _ = self.tx.send(Message::Text(json)).await;
    }

    /// Send a status response.
    async fn send_status(&mut self, status: &str) {
        let json = format!(r#"{{"status":"{}"}}"#, status);
        self.send_json(json).await;
    }
}

/// Visualization server state.
pub struct VisualizeServer {
    /// CPU instance
    cpu: Arc<Mutex<PipelineCpu>>,
    /// Running state
    running: Arc<Mutex<bool>>,
    /// Speed (cycles per second, 0 = unlimited)
    speed: Arc<Mutex<u32>>,
    /// State broadcast sender
    state_tx: broadcast::Sender<CpuSnapshot>,
    /// Breakpoints (addr -> Breakpoint)
    breakpoints: Arc<Mutex<HashMap<u32, Breakpoint>>>,
    /// Execution history
    history: Arc<Mutex<Vec<HistoryRecord>>>,
    /// Maximum history size
    max_history: usize,
}

impl VisualizeServer {
    /// Create a new visualization server.
    pub fn new(cpu: PipelineCpu) -> Self {
        let (state_tx, _) = broadcast::channel(16);
        Self {
            cpu: Arc::new(Mutex::new(cpu)),
            running: Arc::new(Mutex::new(false)),
            speed: Arc::new(Mutex::new(10)),
            state_tx,
            breakpoints: Arc::new(Mutex::new(HashMap::new())),
            history: Arc::new(Mutex::new(Vec::new())),
            max_history: 10000,
        }
    }

    /// Start the WebSocket server.
    pub async fn start(self, port: u16) -> Result<()> {
        let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
        let listener = TcpListener::bind(addr).await?;

        println!("Visualization server started on ws://{}", addr);
        println!("Open frontend/index.html in browser to connect");

        // Pass references to the run loop task
        let cpu = self.cpu.clone();
        let running = self.running.clone();
        let speed = self.speed.clone();
        let state_tx = self.state_tx.clone();
        let breakpoints = self.breakpoints.clone();
        let history = self.history.clone();
        let max_history = self.max_history;

        // Spawn the continuous execution task
        tokio::spawn(Self::run_loop(
            cpu.clone(),
            running.clone(),
            speed.clone(),
            state_tx.clone(),
            breakpoints.clone(),
            history.clone(),
            max_history,
        ));

        // Accept client connections
        loop {
            let (stream, client_addr) = listener.accept().await?;
            let cpu = self.cpu.clone();
            let running = self.running.clone();
            let speed = self.speed.clone();
            let mut state_rx = self.state_tx.subscribe();
            let breakpoints = self.breakpoints.clone();
            let history = self.history.clone();

            tokio::spawn(async move {
                println!("Client connected from {}", client_addr);

                let ws = match accept_async(stream).await {
                    Ok(ws) => ws,
                    Err(e) => {
                        eprintln!("WebSocket accept error: {}", e);
                        return;
                    }
                };

                let (mut tx, mut rx) = ws.split();

                // Send initial state
                {
                    let cpu_guard = cpu.lock().await;
                    let snapshot = cpu_guard.snapshot();
                    let json = serde_json::to_string(&snapshot).unwrap();
                    if tx.send(Message::Text(json)).await.is_err() {
                        return;
                    }
                }

                loop {
                    tokio::select! {
                        // Handle incoming commands
                        msg = rx.next() => {
                            match msg {
                                Some(Ok(Message::Text(cmd))) => {
                                    // Parse and execute command
                                    match Command::parse(&cmd) {
                                        Some(command) => {
                                            let mut ctx = CommandContext::new(
                                                cpu.clone(),
                                                running.clone(),
                                                speed.clone(),
                                                tx,
                                                breakpoints.clone(),
                                                history.clone(),
                                            );
                                            if Self::execute_command(&mut ctx, command).await.is_err() {
                                                break;
                                            }
                                            tx = ctx.tx; // Take back ownership
                                        }
                                        None => {
                                            let response = format!(r#"{{"error":"unknown command: {}"}}"#, cmd);
                                            let _ = tx.send(Message::Text(response)).await;
                                        }
                                    }
                                }
                                Some(Ok(Message::Close(_))) => {
                                    println!("Client {} disconnected", client_addr);
                                    break;
                                }
                                Some(Ok(Message::Ping(data))) => {
                                    if tx.send(Message::Pong(data)).await.is_err() {
                                        break;
                                    }
                                }
                                Some(Err(e)) => {
                                    eprintln!("WebSocket error: {}", e);
                                    break;
                                }
                                None => break,
                                _ => {}
                            }
                        }
                        // Broadcast state updates
                        Ok(snapshot) = state_rx.recv() => {
                            let json = serde_json::to_string(&snapshot).unwrap();
                            if tx.send(Message::Text(json)).await.is_err() {
                                break;
                            }
                        }
                    }
                }

                println!("Client {} connection closed", client_addr);
            });
        }
    }

    /// Execute a parsed command using CommandContext.
    async fn execute_command(ctx: &mut CommandContext, cmd: Command) -> Result<()> {
        match cmd {
            Command::State => {
                let json = {
                    let cpu_guard = ctx.cpu.lock().await;
                    let snapshot = cpu_guard.snapshot();
                    serde_json::to_string(&snapshot).unwrap()
                };
                ctx.send_json(json).await;
            }
            Command::Step => {
                let json = {
                    let mut cpu_guard = ctx.cpu.lock().await;
                    let _ = cpu_guard.clock();
                    let snapshot = cpu_guard.snapshot();
                    serde_json::to_string(&snapshot).unwrap()
                };
                ctx.send_json(json).await;
            }
            Command::Run => {
                {
                    let mut running_guard = ctx.running.lock().await;
                    *running_guard = true;
                }
                ctx.send_status("running").await;
            }
            Command::Pause => {
                {
                    let mut running_guard = ctx.running.lock().await;
                    *running_guard = false;
                }
                ctx.send_status("paused").await;
            }
            Command::Reset => {
                let json = {
                    let mut cpu_guard = ctx.cpu.lock().await;
                    cpu_guard.reset();
                    let snapshot = cpu_guard.snapshot();
                    serde_json::to_string(&snapshot).unwrap()
                };
                ctx.send_json(json).await;
            }
            Command::Speed { value } => {
                {
                    let mut speed_guard = ctx.speed.lock().await;
                    *speed_guard = value;
                }
                let response = format!(r#"{{"status":"speed_set","speed":{}}}"#, value);
                ctx.send_json(response).await;
            }
            Command::Memory { addr, size } => {
                let response = {
                    let cpu_guard = ctx.cpu.lock().await;
                    let bus = cpu_guard.bus();

                    let mut data = Vec::with_capacity(size);
                    for i in 0..size {
                        match bus.read_byte(Addr::new(addr + i as u32)) {
                            Ok(byte) => data.push(byte.raw()),
                            Err(_) => break,
                        }
                    }

                    MemoryReadResponse {
                        addr,
                        data,
                        success: true,
                        error: None,
                    }
                };
                let json = serde_json::to_string(&response).unwrap();
                ctx.send_json(json).await;
            }
            Command::BreakpointAdd { addr, label } => {
                {
                    let mut breakpoints_guard = ctx.breakpoints.lock().await;
                    breakpoints_guard.insert(addr, Breakpoint {
                        addr,
                        enabled: true,
                        label,
                        hit_count: 0,
                    });
                }
                let response = format!(r#"{{"type":"breakpoint_added","addr":"0x{:08x}"}}"#, addr);
                ctx.send_json(response).await;
            }
            Command::BreakpointRemove { addr } => {
                {
                    let mut breakpoints_guard = ctx.breakpoints.lock().await;
                    breakpoints_guard.remove(&addr);
                }
                let response = format!(r#"{{"type":"breakpoint_removed","addr":"0x{:08x}"}}"#, addr);
                ctx.send_json(response).await;
            }
            Command::BreakpointList => {
                let response = {
                    let breakpoints_guard = ctx.breakpoints.lock().await;
                    let bp_list: Vec<&Breakpoint> = breakpoints_guard.values().collect();
                    serde_json::json!({
                        "type": "breakpoint_list",
                        "breakpoints": bp_list
                    })
                };
                ctx.send_json(response.to_string()).await;
            }
            Command::Disassemble { addr, count } => {
                // Read instructions from memory
                let instructions_data = {
                    let cpu_guard = ctx.cpu.lock().await;
                    let bus = cpu_guard.bus();

                    let mut data = Vec::with_capacity(count);
                    let mut current_addr = addr;

                    for _ in 0..count {
                        let mut bytes = [0u8; 4];
                        let mut success = true;
                        for i in 0..4 {
                            match bus.read_byte(Addr::new(current_addr + i)) {
                                Ok(byte) => bytes[i as usize] = byte.raw(),
                                Err(_) => {
                                    success = false;
                                    break;
                                }
                            }
                        }

                        if !success {
                            break;
                        }

                        let instr = u32::from_le_bytes(bytes);
                        data.push((current_addr, bytes.to_vec(), disassemble(instr)));
                        current_addr += 4;
                    }
                    data
                };

                // Check breakpoints separately
                let instructions = {
                    let breakpoints_guard = ctx.breakpoints.lock().await;
                    instructions_data
                        .into_iter()
                        .map(|(addr, bytes, instr)| DisassembledInstruction {
                            addr,
                            bytes,
                            instruction: instr,
                            has_breakpoint: breakpoints_guard.contains_key(&addr),
                        })
                        .collect()
                };

                let response = DisassemblyResponse {
                    base_addr: addr,
                    instructions,
                    success: true,
                    error: None,
                };
                let json = serde_json::to_string(&response).unwrap();
                ctx.send_json(json).await;
            }
            Command::History { start, count } => {
                let response = {
                    let history_guard = ctx.history.lock().await;
                    let total = history_guard.len();
                    let end = (start + count).min(total);
                    let records: Vec<_> = history_guard[start..end].to_vec();

                    HistoryResponse {
                        records,
                        total,
                        position: start,
                    }
                };
                let json = serde_json::to_string(&response).unwrap();
                ctx.send_json(json).await;
            }
        }

        Ok(())
    }

    /// Continuous execution loop.
    async fn run_loop(
        cpu: Arc<Mutex<PipelineCpu>>,
        running: Arc<Mutex<bool>>,
        speed: Arc<Mutex<u32>>,
        state_tx: broadcast::Sender<CpuSnapshot>,
        breakpoints: Arc<Mutex<HashMap<u32, Breakpoint>>>,
        history: Arc<Mutex<Vec<HistoryRecord>>>,
        max_history: usize,
    ) {
        loop {
            let is_running = *running.lock().await;
            if !is_running {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                continue;
            }

            let current_speed = *speed.lock().await;

            // Execute cycle and collect data
            let (snapshot, new_pc, _should_pause) = {
                let mut cpu_guard = cpu.lock().await;
                if cpu_guard.is_halted() {
                    let mut running_guard = running.lock().await;
                    *running_guard = false;
                    continue;
                }

                // Record history before execution
                let pc = cpu_guard.pc().raw();
                let cycles = cpu_guard.cycles();

                // Read instruction at PC
                let instr = {
                    let bus = cpu_guard.bus();
                    let mut bytes = [0u8; 4];
                    let mut success = true;
                    for i in 0..4 {
                        match bus.read_byte(Addr::new(pc + i)) {
                            Ok(byte) => bytes[i as usize] = byte.raw(),
                            Err(_) => {
                                success = false;
                                break;
                            }
                        }
                    }
                    if success {
                        Some(u32::from_le_bytes(bytes))
                    } else {
                        None
                    }
                };

                // Execute
                let _ = cpu_guard.clock();

                // Record to history (drop bus lock first)
                {
                    let mut history_guard = history.lock().await;
                    if history_guard.len() >= max_history {
                        history_guard.remove(0);
                    }
                    history_guard.push(HistoryRecord {
                        cycle: cycles,
                        pc,
                        instruction: instr,
                        instruction_str: instr.map(disassemble),
                        reg_changes: vec![],
                        mem_changes: vec![],
                    });
                }

                // Get new PC and snapshot
                let new_pc = cpu_guard.pc().raw();
                let snapshot = cpu_guard.snapshot();
                (snapshot, new_pc, false)
            };

            // Check breakpoints separately
            {
                let mut breakpoints_guard = breakpoints.lock().await;
                if let Some(bp) = breakpoints_guard.get_mut(&new_pc) {
                    bp.hit_count += 1;
                    let mut running_guard = running.lock().await;
                    *running_guard = false;
                }
            }

            // Broadcast state update
            let _ = state_tx.send(snapshot);

            if current_speed > 0 {
                let delay_ms = 1000 / current_speed;
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms as u64)).await;
            } else {
                tokio::task::yield_now().await;
            }
        }
    }
}

/// Start the visualization server with a CPU.
pub async fn start_visualize_server(cpu: PipelineCpu, port: u16) -> Result<()> {
    let server = VisualizeServer::new(cpu);
    server.start(port).await
}
