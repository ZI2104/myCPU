//! WebSocket server for CPU visualization.
//!
//! This module provides a WebSocket server that allows frontend clients
//! to connect and control the CPU execution, receiving real-time state updates.

use crate::cpu::pipeline::PipelineCpu;
use crate::cpu::ExecutionModel;
use crate::error::Result;
use crate::peripheral::INPUT_BASE;
use crate::types::Addr;
use crate::visualize::snapshot::{
    disassemble, Breakpoint, CpuSnapshot, DisassembledInstruction, DisassemblyResponse,
    FramebufferResponse, HistoryRecord, HistoryResponse, MemoryReadResponse,
};
use futures_util::{SinkExt, StreamExt};
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, Mutex};
use tokio_tungstenite::{accept_async, tungstenite::Message};

/// Default base address for disassembly
const DEFAULT_BASE_ADDR: u32 = 0x80000000;
/// Max pixels allowed in one framebuffer response to avoid oversized websocket payloads.
const MAX_FRAMEBUFFER_PIXELS: u32 = 1024 * 1024;
/// Default Linux demo framebuffer base address (must fit in default 16MB RAM window).
const LINUX_FB_ADDR: u32 = 0x80E0_0000;
/// Default Linux demo framebuffer width.
const LINUX_FB_WIDTH: u32 = 320;
/// Default Linux demo framebuffer height.
const LINUX_FB_HEIGHT: u32 = 240;
/// Default Linux demo framebuffer pixel format.
const LINUX_FB_FORMAT: PixelFormat = PixelFormat::Rgb565;

fn linux_fb_preset() -> Command {
    Command::Framebuffer {
        addr: LINUX_FB_ADDR,
        width: LINUX_FB_WIDTH,
        height: LINUX_FB_HEIGHT,
        format: LINUX_FB_FORMAT,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DemoPattern {
    Pong,
    Checker,
    Gradient,
}

impl DemoPattern {
    fn parse(input: &str) -> Option<Self> {
        match input.to_ascii_lowercase().as_str() {
            "pong" => Some(Self::Pong),
            "checker" | "check" => Some(Self::Checker),
            "gradient" | "grad" => Some(Self::Gradient),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Pong => "pong",
            Self::Checker => "checker",
            Self::Gradient => "gradient",
        }
    }
}

fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    let r5 = (r as u16 >> 3) & 0x1F;
    let g6 = (g as u16 >> 2) & 0x3F;
    let b5 = (b as u16 >> 3) & 0x1F;
    (r5 << 11) | (g6 << 5) | b5
}

fn write_rgb565_pixel(buf: &mut [u8], width: u32, x: u32, y: u32, color: u16) {
    let idx = ((y * width + x) * 2) as usize;
    if idx + 1 < buf.len() {
        let [lo, hi] = color.to_le_bytes();
        buf[idx] = lo;
        buf[idx + 1] = hi;
    }
}

fn generate_demo_frame_rgb565(pattern: DemoPattern, width: u32, height: u32) -> Vec<u8> {
    let mut buf = vec![0u8; (width * height * 2) as usize];

    match pattern {
        DemoPattern::Checker => {
            let tile = 16u32;
            for y in 0..height {
                for x in 0..width {
                    let dark = ((x / tile) + (y / tile)) % 2 == 0;
                    let color = if dark {
                        rgb565(20, 30, 50)
                    } else {
                        rgb565(230, 220, 180)
                    };
                    write_rgb565_pixel(&mut buf, width, x, y, color);
                }
            }
        }
        DemoPattern::Gradient => {
            for y in 0..height {
                for x in 0..width {
                    let r = ((x * 255) / width.max(1)) as u8;
                    let g = ((y * 255) / height.max(1)) as u8;
                    let b = 180u8;
                    write_rgb565_pixel(&mut buf, width, x, y, rgb565(r, g, b));
                }
            }
        }
        DemoPattern::Pong => {
            let bg = rgb565(8, 8, 16);
            let fg = rgb565(240, 240, 240);
            let ball = rgb565(255, 120, 40);

            for y in 0..height {
                for x in 0..width {
                    write_rgb565_pixel(&mut buf, width, x, y, bg);
                }
            }

            // Center dashed line
            let cx = width / 2;
            for y in (0..height).step_by(8) {
                for dy in 0..4 {
                    if y + dy < height {
                        write_rgb565_pixel(&mut buf, width, cx, y + dy, fg);
                    }
                }
            }

            // Paddles
            let paddle_h = (height / 4).max(24);
            let y0 = (height.saturating_sub(paddle_h)) / 2;
            for y in y0..(y0 + paddle_h).min(height) {
                write_rgb565_pixel(&mut buf, width, 10, y, fg);
                write_rgb565_pixel(&mut buf, width, 11, y, fg);
                if width > 12 {
                    write_rgb565_pixel(&mut buf, width, width - 12, y, fg);
                    write_rgb565_pixel(&mut buf, width, width - 11, y, fg);
                }
            }

            // Ball
            let bx = width / 2;
            let by = height / 2;
            for y in by.saturating_sub(3)..=(by + 3).min(height.saturating_sub(1)) {
                for x in bx.saturating_sub(3)..=(bx + 3).min(width.saturating_sub(1)) {
                    write_rgb565_pixel(&mut buf, width, x, y, ball);
                }
            }
        }
    }

    buf
}

fn parse_u32_auto(input: &str) -> Option<u32> {
    if let Some(hex) = input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
    {
        u32::from_str_radix(hex, 16).ok()
    } else {
        input.parse::<u32>().ok()
    }
}

fn parse_input_key_code(input: &str) -> Option<u8> {
    match input.to_ascii_lowercase().as_str() {
        "up" => Some(0),
        "left" => Some(1),
        "down" => Some(2),
        "right" => Some(3),
        "a" | "action" => Some(4),
        "b" | "back" => Some(5),
        "start" => Some(6),
        "select" => Some(7),
        other => parse_u32_auto(other)
            .and_then(|v| (v <= u8::MAX as u32).then_some(v as u8)),
    }
}

fn parse_input_pressed(input: &str) -> Option<bool> {
    match input.to_ascii_lowercase().as_str() {
        "down" | "press" | "pressed" | "1" => Some(true),
        "up" | "release" | "released" | "0" => Some(false),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PixelFormat {
    Gray8,
    Rgb565,
    Rgb888,
}

impl PixelFormat {
    fn parse(input: &str) -> Option<Self> {
        match input.to_ascii_lowercase().as_str() {
            "gray8" | "g8" => Some(Self::Gray8),
            "rgb565" | "565" => Some(Self::Rgb565),
            "rgb888" | "888" => Some(Self::Rgb888),
            _ => None,
        }
    }

    fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Gray8 => 1,
            Self::Rgb565 => 2,
            Self::Rgb888 => 3,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Gray8 => "gray8",
            Self::Rgb565 => "rgb565",
            Self::Rgb888 => "rgb888",
        }
    }
}

fn expand_rgb565_to_rgba(input: &[u8], pixel_count: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(pixel_count * 4);
    for chunk in input.chunks_exact(2) {
        let value = u16::from_le_bytes([chunk[0], chunk[1]]);
        let r = ((value >> 11) & 0x1F) as u8;
        let g = ((value >> 5) & 0x3F) as u8;
        let b = (value & 0x1F) as u8;
        output.push((r << 3) | (r >> 2));
        output.push((g << 2) | (g >> 4));
        output.push((b << 3) | (b >> 2));
        output.push(255);
    }
    output
}

fn expand_rgb888_to_rgba(input: &[u8], pixel_count: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(pixel_count * 4);
    for chunk in input.chunks_exact(3) {
        output.push(chunk[0]);
        output.push(chunk[1]);
        output.push(chunk[2]);
        output.push(255);
    }
    output
}

fn expand_gray8_to_rgba(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len() * 4);
    for gray in input {
        output.push(*gray);
        output.push(*gray);
        output.push(*gray);
        output.push(255);
    }
    output
}

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
    /// Read framebuffer and return RGBA pixels.
    Framebuffer {
        addr: u32,
        width: u32,
        height: u32,
        format: PixelFormat,
    },
    /// Generate and write a demo framebuffer scene into memory.
    FramebufferDemo { pattern: DemoPattern },
    /// Inject one input key event.
    InputKey { key_code: u8, pressed: bool },
    /// Clear all input key states.
    InputClear,
    /// Read input state snapshot.
    InputState,
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
            "speed" => parts
                .get(1)
                .and_then(|s| s.parse::<u32>().ok())
                .map(|value| Command::Speed { value }),
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
            "framebuffer" | "fb" => {
                if parts.len() == 1 {
                    return Some(linux_fb_preset());
                }
                if parts.len() == 2
                    && (parts[1].eq_ignore_ascii_case("linux")
                        || parts[1].eq_ignore_ascii_case("default"))
                {
                    return Some(linux_fb_preset());
                }
                if parts.len() < 4 {
                    return None;
                }
                let addr = parse_u32_auto(parts[1])?;
                let width = parse_u32_auto(parts[2])?;
                let height = parse_u32_auto(parts[3])?;
                let format = parts
                    .get(4)
                    .and_then(|value| PixelFormat::parse(value))
                    .unwrap_or(PixelFormat::Rgb565);
                Some(Command::Framebuffer {
                    addr,
                    width,
                    height,
                    format,
                })
            }
            "fb_demo" | "framebuffer_demo" => {
                let pattern = parts
                    .get(1)
                    .and_then(|value| DemoPattern::parse(value))
                    .unwrap_or(DemoPattern::Pong);
                Some(Command::FramebufferDemo { pattern })
            }
            "input" | "in" => {
                if parts.len() < 2 {
                    return None;
                }

                if parts[1].eq_ignore_ascii_case("clear") {
                    return Some(Command::InputClear);
                }

                if parts[1].eq_ignore_ascii_case("state") {
                    return Some(Command::InputState);
                }

                if parts.len() < 3 {
                    return None;
                }

                let key_code = parse_input_key_code(parts[1])?;
                let pressed = parse_input_pressed(parts[2])?;
                Some(Command::InputKey { key_code, pressed })
            }
            "input_clear" => Some(Command::InputClear),
            "input_state" => Some(Command::InputState),
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
    pub history: Arc<Mutex<VecDeque<HistoryRecord>>>,
}

impl CommandContext {
    /// Create a new command context.
    pub fn new(
        cpu: Arc<Mutex<PipelineCpu>>,
        running: Arc<Mutex<bool>>,
        speed: Arc<Mutex<u32>>,
        tx: WsSender,
        breakpoints: Arc<Mutex<HashMap<u32, Breakpoint>>>,
        history: Arc<Mutex<VecDeque<HistoryRecord>>>,
    ) -> Self {
        Self {
            cpu,
            running,
            speed,
            tx,
            breakpoints,
            history,
        }
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
    history: Arc<Mutex<VecDeque<HistoryRecord>>>,
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
            history: Arc::new(Mutex::new(VecDeque::new())),
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
                    breakpoints_guard.insert(
                        addr,
                        Breakpoint {
                            addr,
                            enabled: true,
                            label,
                            hit_count: 0,
                        },
                    );
                }
                let response = format!(r#"{{"type":"breakpoint_added","addr":"0x{:08x}"}}"#, addr);
                ctx.send_json(response).await;
            }
            Command::BreakpointRemove { addr } => {
                {
                    let mut breakpoints_guard = ctx.breakpoints.lock().await;
                    breakpoints_guard.remove(&addr);
                }
                let response =
                    format!(r#"{{"type":"breakpoint_removed","addr":"0x{:08x}"}}"#, addr);
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
                    let take = end.saturating_sub(start);
                    let records: Vec<_> = history_guard
                        .iter()
                        .skip(start)
                        .take(take)
                        .cloned()
                        .collect();

                    HistoryResponse {
                        records,
                        total,
                        position: start,
                    }
                };
                let json = serde_json::to_string(&response).unwrap();
                ctx.send_json(json).await;
            }
            Command::Framebuffer {
                addr,
                width,
                height,
                format,
            } => {
                let response = {
                    if width == 0 || height == 0 {
                        FramebufferResponse {
                            response_type: "framebuffer".to_string(),
                            addr,
                            width,
                            height,
                            format: format.as_str().to_string(),
                            pixels: Vec::new(),
                            success: false,
                            error: Some("framebuffer width/height must be > 0".to_string()),
                        }
                    } else {
                        let pixel_count = width.saturating_mul(height);
                        if pixel_count > MAX_FRAMEBUFFER_PIXELS {
                            FramebufferResponse {
                                response_type: "framebuffer".to_string(),
                                addr,
                                width,
                                height,
                                format: format.as_str().to_string(),
                                pixels: Vec::new(),
                                success: false,
                                error: Some(format!(
                                    "framebuffer too large: {} pixels (max {})",
                                    pixel_count, MAX_FRAMEBUFFER_PIXELS
                                )),
                            }
                        } else {
                            let raw_size = pixel_count as usize * format.bytes_per_pixel();
                            let mut raw = vec![0u8; raw_size];
                            let read_result = {
                                let cpu_guard = ctx.cpu.lock().await;
                                cpu_guard
                                    .bus()
                                    .read_bytes(Addr::new(addr), &mut raw)
                                    .map(|_| ())
                            };

                            match read_result {
                                Ok(()) => {
                                    let pixels = match format {
                                        PixelFormat::Gray8 => expand_gray8_to_rgba(&raw),
                                        PixelFormat::Rgb565 => {
                                            expand_rgb565_to_rgba(&raw, pixel_count as usize)
                                        }
                                        PixelFormat::Rgb888 => {
                                            expand_rgb888_to_rgba(&raw, pixel_count as usize)
                                        }
                                    };
                                    FramebufferResponse {
                                        response_type: "framebuffer".to_string(),
                                        addr,
                                        width,
                                        height,
                                        format: format.as_str().to_string(),
                                        pixels,
                                        success: true,
                                        error: None,
                                    }
                                }
                                Err(err) => FramebufferResponse {
                                    response_type: "framebuffer".to_string(),
                                    addr,
                                    width,
                                    height,
                                    format: format.as_str().to_string(),
                                    pixels: Vec::new(),
                                    success: false,
                                    error: Some(err.to_string()),
                                },
                            }
                        }
                    }
                };

                let json = serde_json::to_string(&response).unwrap();
                ctx.send_json(json).await;
            }
            Command::FramebufferDemo { pattern } => {
                let result = {
                    let mut cpu_guard = ctx.cpu.lock().await;
                    let data = generate_demo_frame_rgb565(pattern, LINUX_FB_WIDTH, LINUX_FB_HEIGHT);
                    cpu_guard
                        .bus_mut()
                        .write_bytes(Addr::new(LINUX_FB_ADDR), &data)
                        .map(|_| ())
                };

                let response = match result {
                    Ok(()) => serde_json::json!({
                        "type": "framebuffer_demo",
                        "success": true,
                        "pattern": pattern.as_str(),
                        "addr": LINUX_FB_ADDR,
                        "width": LINUX_FB_WIDTH,
                        "height": LINUX_FB_HEIGHT,
                        "format": LINUX_FB_FORMAT.as_str(),
                    }),
                    Err(err) => serde_json::json!({
                        "type": "framebuffer_demo",
                        "success": false,
                        "pattern": pattern.as_str(),
                        "error": err.to_string(),
                    }),
                };

                ctx.send_json(response.to_string()).await;
            }
            Command::InputKey { key_code, pressed } => {
                let success = {
                    let mut cpu_guard = ctx.cpu.lock().await;
                    cpu_guard.bus_mut().inject_input_key(key_code, pressed)
                };

                let response = serde_json::json!({
                    "type": "input_ack",
                    "success": success,
                    "key_code": key_code,
                    "pressed": pressed,
                });
                ctx.send_json(response.to_string()).await;
            }
            Command::InputClear => {
                let success = {
                    let mut cpu_guard = ctx.cpu.lock().await;
                    cpu_guard.bus_mut().clear_input_keys()
                };

                let response = serde_json::json!({
                    "type": "input_ack",
                    "success": success,
                    "cleared": true,
                });
                ctx.send_json(response.to_string()).await;
            }
            Command::InputState => {
                let response = {
                    let cpu_guard = ctx.cpu.lock().await;
                    if let Some((key_state, last_event, event_count, irq_pending)) =
                        cpu_guard.bus().get_input_snapshot()
                    {
                        serde_json::json!({
                            "type": "input_state",
                            "success": true,
                            "base_addr": INPUT_BASE,
                            "key_state": key_state,
                            "last_event": last_event,
                            "event_count": event_count,
                            "irq_pending": irq_pending,
                        })
                    } else {
                        serde_json::json!({
                            "type": "input_state",
                            "success": false,
                            "error": "Input peripheral not attached",
                        })
                    }
                };

                ctx.send_json(response.to_string()).await;
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
        history: Arc<Mutex<VecDeque<HistoryRecord>>>,
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
                        history_guard.pop_front();
                    }
                    history_guard.push_back(HistoryRecord {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_framebuffer_command_defaults_to_rgb565() {
        let cmd = Command::parse("framebuffer 0x81000000 320 240");
        assert_eq!(
            cmd,
            Some(Command::Framebuffer {
                addr: 0x8100_0000,
                width: 320,
                height: 240,
                format: PixelFormat::Rgb565,
            })
        );
    }

    #[test]
    fn test_parse_framebuffer_command_with_format() {
        let cmd = Command::parse("fb 0x81000000 64 64 gray8");
        assert_eq!(
            cmd,
            Some(Command::Framebuffer {
                addr: 0x8100_0000,
                width: 64,
                height: 64,
                format: PixelFormat::Gray8,
            })
        );
    }

    #[test]
    fn test_parse_framebuffer_linux_preset_alias() {
        let cmd = Command::parse("fb linux");
        assert_eq!(
            cmd,
            Some(Command::Framebuffer {
                addr: LINUX_FB_ADDR,
                width: LINUX_FB_WIDTH,
                height: LINUX_FB_HEIGHT,
                format: LINUX_FB_FORMAT,
            })
        );
    }

    #[test]
    fn test_parse_framebuffer_default_without_arguments() {
        let cmd = Command::parse("framebuffer");
        assert_eq!(
            cmd,
            Some(Command::Framebuffer {
                addr: LINUX_FB_ADDR,
                width: LINUX_FB_WIDTH,
                height: LINUX_FB_HEIGHT,
                format: LINUX_FB_FORMAT,
            })
        );
    }

    #[test]
    fn test_expand_rgb565_to_rgba() {
        // pure red pixel in RGB565: 0xF800
        let rgba = expand_rgb565_to_rgba(&[0x00, 0xF8], 1);
        assert_eq!(rgba, vec![255, 0, 0, 255]);
    }

    #[test]
    fn test_parse_fb_demo_pattern() {
        let cmd = Command::parse("fb_demo checker");
        assert_eq!(
            cmd,
            Some(Command::FramebufferDemo {
                pattern: DemoPattern::Checker,
            })
        );
    }

    #[test]
    fn test_generate_demo_frame_size() {
        let frame = generate_demo_frame_rgb565(DemoPattern::Pong, 320, 240);
        assert_eq!(frame.len(), 320 * 240 * 2);
    }

    #[test]
    fn test_parse_input_key_command_with_named_key() {
        let cmd = Command::parse("input left down");
        assert_eq!(
            cmd,
            Some(Command::InputKey {
                key_code: 1,
                pressed: true,
            })
        );
    }

    #[test]
    fn test_parse_input_key_command_with_numeric_key() {
        let cmd = Command::parse("input 7 up");
        assert_eq!(
            cmd,
            Some(Command::InputKey {
                key_code: 7,
                pressed: false,
            })
        );
    }

    #[test]
    fn test_parse_input_state_and_clear_commands() {
        assert_eq!(Command::parse("input state"), Some(Command::InputState));
        assert_eq!(Command::parse("input clear"), Some(Command::InputClear));
    }
}

/// Start the visualization server with a CPU.
pub async fn start_visualize_server(cpu: PipelineCpu, port: u16) -> Result<()> {
    let server = VisualizeServer::new(cpu);
    server.start(port).await
}
