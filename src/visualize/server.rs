//! WebSocket server for CPU visualization.
//!
//! This module provides a WebSocket server that allows frontend clients
//! to connect and control the CPU execution, receiving real-time state updates.

use crate::cpu::pipeline::PipelineCpu;
use crate::cpu::ExecutionModel;
use crate::error::Result;
use crate::memory::Ram;
use crate::peripheral::INPUT_BASE;
use crate::types::Addr;
use crate::visualize::snapshot::{
    disassemble, Breakpoint, CpuSnapshot, DisassembledInstruction, DisassemblyResponse,
    FramebufferResponse, HistoryRecord, HistoryResponse, MemoryReadResponse,
};
use futures_util::{SinkExt, StreamExt};
use log::debug;
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

#[derive(Debug, Clone)]
struct GameDemoState {
    initialized: bool,
    tick: u64,
    ball_x: i32,
    ball_y: i32,
    vel_x: i32,
    vel_y: i32,
    left_paddle_y: i32,
    right_paddle_y: i32,
    score_left: u32,
    score_right: u32,
}

impl Default for GameDemoState {
    fn default() -> Self {
        let mut state = Self {
            initialized: false,
            tick: 0,
            ball_x: 0,
            ball_y: 0,
            vel_x: 3,
            vel_y: 2,
            left_paddle_y: 0,
            right_paddle_y: 0,
            score_left: 0,
            score_right: 0,
        };
        state.reset();
        state
    }
}

impl GameDemoState {
    const PADDLE_H: i32 = 36;
    const PADDLE_W: i32 = 3;
    const PADDLE_MARGIN: i32 = 10;
    const BALL_R: i32 = 3;

    const KEY_UP: u32 = 1 << 0;
    const KEY_DOWN: u32 = 1 << 2;
    const KEY_LEFT: u32 = 1 << 1;
    const KEY_RIGHT: u32 = 1 << 3;
    const KEY_A: u32 = 1 << 4;
    const KEY_B: u32 = 1 << 5;

    fn reset(&mut self) {
        self.tick = 0;
        self.ball_x = (LINUX_FB_WIDTH / 2) as i32;
        self.ball_y = (LINUX_FB_HEIGHT / 2) as i32;
        self.vel_x = 3;
        self.vel_y = 2;
        self.left_paddle_y = (LINUX_FB_HEIGHT as i32 - Self::PADDLE_H) / 2;
        self.right_paddle_y = (LINUX_FB_HEIGHT as i32 - Self::PADDLE_H) / 2;
        self.initialized = true;
    }

    fn clamp_paddles(&mut self) {
        let max_y = LINUX_FB_HEIGHT as i32 - Self::PADDLE_H;
        self.left_paddle_y = self.left_paddle_y.clamp(0, max_y);
        self.right_paddle_y = self.right_paddle_y.clamp(0, max_y);
    }

    fn step(&mut self, key_state: u32) {
        if !self.initialized {
            self.reset();
        }

        if (key_state & Self::KEY_B) != 0 {
            self.score_left = 0;
            self.score_right = 0;
        }

        let paddle_speed = if (key_state & Self::KEY_A) != 0 { 6 } else { 4 };
        if (key_state & Self::KEY_UP) != 0 {
            self.left_paddle_y -= paddle_speed;
        }
        if (key_state & Self::KEY_DOWN) != 0 {
            self.left_paddle_y += paddle_speed;
        }

        if (key_state & Self::KEY_LEFT) != 0 {
            self.vel_x = (self.vel_x - 1).max(-5);
        }
        if (key_state & Self::KEY_RIGHT) != 0 {
            self.vel_x = (self.vel_x + 1).min(5);
        }
        if self.vel_x == 0 {
            self.vel_x = 1;
        }

        // Right paddle AI follows ball.
        let right_center = self.right_paddle_y + Self::PADDLE_H / 2;
        if self.ball_y > right_center + 2 {
            self.right_paddle_y += 3;
        } else if self.ball_y < right_center - 2 {
            self.right_paddle_y -= 3;
        }

        self.clamp_paddles();

        self.ball_x += self.vel_x;
        self.ball_y += self.vel_y;

        let top = Self::BALL_R;
        let bottom = LINUX_FB_HEIGHT as i32 - 1 - Self::BALL_R;
        if self.ball_y <= top || self.ball_y >= bottom {
            self.vel_y = -self.vel_y;
            self.ball_y = self.ball_y.clamp(top, bottom);
        }

        let left_x = Self::PADDLE_MARGIN + Self::PADDLE_W;
        let right_x = LINUX_FB_WIDTH as i32 - 1 - Self::PADDLE_MARGIN - Self::PADDLE_W;

        let left_hit = self.ball_x - Self::BALL_R <= left_x
            && self.ball_y >= self.left_paddle_y
            && self.ball_y <= self.left_paddle_y + Self::PADDLE_H;
        if left_hit {
            self.vel_x = self.vel_x.abs().max(2);
            self.ball_x = left_x + Self::BALL_R;
        }

        let right_hit = self.ball_x + Self::BALL_R >= right_x
            && self.ball_y >= self.right_paddle_y
            && self.ball_y <= self.right_paddle_y + Self::PADDLE_H;
        if right_hit {
            self.vel_x = -self.vel_x.abs().max(2);
            self.ball_x = right_x - Self::BALL_R;
        }

        if self.ball_x < 0 {
            self.score_right = self.score_right.wrapping_add(1);
            self.ball_x = (LINUX_FB_WIDTH / 2) as i32;
            self.ball_y = (LINUX_FB_HEIGHT / 2) as i32;
            self.vel_x = 3;
            self.vel_y = 2;
        } else if self.ball_x >= LINUX_FB_WIDTH as i32 {
            self.score_left = self.score_left.wrapping_add(1);
            self.ball_x = (LINUX_FB_WIDTH / 2) as i32;
            self.ball_y = (LINUX_FB_HEIGHT / 2) as i32;
            self.vel_x = -3;
            self.vel_y = 2;
        }

        self.tick = self.tick.wrapping_add(1);
    }

    fn render_rgb565(&self) -> Vec<u8> {
        let width = LINUX_FB_WIDTH;
        let height = LINUX_FB_HEIGHT;
        let mut buf = vec![0u8; (width * height * 2) as usize];

        let bg = rgb565(8, 10, 20);
        let line = rgb565(120, 130, 160);
        let paddle = rgb565(235, 235, 235);
        let ball = rgb565(255, 130, 50);
        let score = rgb565(80, 200, 120);

        for y in 0..height {
            for x in 0..width {
                write_rgb565_pixel(&mut buf, width, x, y, bg);
            }
        }

        // Middle line
        let mid = width / 2;
        for y in (0..height).step_by(8) {
            for dy in 0..4 {
                if y + dy < height {
                    write_rgb565_pixel(&mut buf, width, mid, y + dy, line);
                }
            }
        }

        // Paddles
        let lx = Self::PADDLE_MARGIN as u32;
        let rx = width - 1 - Self::PADDLE_MARGIN as u32;
        for y in
            self.left_paddle_y.max(0) as u32..(self.left_paddle_y + Self::PADDLE_H).max(0) as u32
        {
            if y >= height {
                break;
            }
            for dx in 0..Self::PADDLE_W as u32 {
                write_rgb565_pixel(&mut buf, width, lx + dx, y, paddle);
                write_rgb565_pixel(&mut buf, width, rx.saturating_sub(dx), y, paddle);
            }
        }

        // Ball
        let bx = self.ball_x;
        let by = self.ball_y;
        for y in (by - Self::BALL_R)..=(by + Self::BALL_R) {
            if y < 0 || y >= height as i32 {
                continue;
            }
            for x in (bx - Self::BALL_R)..=(bx + Self::BALL_R) {
                if x < 0 || x >= width as i32 {
                    continue;
                }
                write_rgb565_pixel(&mut buf, width, x as u32, y as u32, ball);
            }
        }

        // Score bars
        let left_score_len = self.score_left.min(20);
        let right_score_len = self.score_right.min(20);
        for i in 0..left_score_len {
            let x = 12 + i * 6;
            for y in 8..14 {
                write_rgb565_pixel(&mut buf, width, x, y, score);
                write_rgb565_pixel(&mut buf, width, x + 1, y, score);
            }
        }
        for i in 0..right_score_len {
            let x = width.saturating_sub(14 + i * 6);
            for y in 8..14 {
                write_rgb565_pixel(&mut buf, width, x, y, score);
                write_rgb565_pixel(&mut buf, width, x.saturating_sub(1), y, score);
            }
        }

        buf
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GameCommandAction {
    Init,
    Step,
    Reset,
    State,
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
        other => parse_u32_auto(other).and_then(|v| (v <= u8::MAX as u32).then_some(v as u8)),
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
    /// Execute N steps in one request
    StepN { count: u32 },
    /// Start continuous execution
    Run,
    /// Pause execution
    Pause,
    /// Reset CPU
    Reset,
    /// Set initial PC used by Reset
    SetInitialPc { addr: u32 },
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
    /// Run framebuffer game demo command.
    FramebufferGame { action: GameCommandAction },
    /// Inject one input key event.
    InputKey { key_code: u8, pressed: bool },
    /// Clear all input key states.
    InputClear,
    /// Read input state snapshot.
    InputState,
    /// Read NPU state snapshot.
    NpuState,
    /// Read LPU state snapshot.
    LpuState,
    /// Read GPU state snapshot.
    GpuState,
    /// Read TPU state snapshot.
    TpuState,
    /// Switch branch predictor type.
    PredictorSwitch { predictor_type: String },
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
            "stepn" | "step_n" => parts
                .get(1)
                .and_then(|s| s.parse::<u32>().ok())
                .map(|count| Command::StepN { count }),
            "run" => Some(Command::Run),
            "pause" => Some(Command::Pause),
            "reset" => Some(Command::Reset),
            "set_initial_pc" => parts
                .get(1)
                .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                .map(|addr| Command::SetInitialPc { addr }),
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
            "fb_game" | "framebuffer_game" => {
                let action = match parts.get(1).map(|value| value.to_ascii_lowercase()) {
                    Some(value) if value == "init" => GameCommandAction::Init,
                    Some(value) if value == "reset" => GameCommandAction::Reset,
                    Some(value) if value == "state" => GameCommandAction::State,
                    Some(value) if value == "step" => GameCommandAction::Step,
                    Some(_) => return None,
                    None => GameCommandAction::Step,
                };
                Some(Command::FramebufferGame { action })
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
            "npu" => {
                let action = parts.get(1).map(|value| value.to_ascii_lowercase());
                if action.as_deref() == Some("state") || action.as_deref() == Some("status") {
                    Some(Command::NpuState)
                } else {
                    None
                }
            }
            "lpu" => {
                let action = parts.get(1).map(|value| value.to_ascii_lowercase());
                if action.as_deref() == Some("state") || action.as_deref() == Some("status") {
                    Some(Command::LpuState)
                } else {
                    None
                }
            }
            "gpu" => {
                let action = parts.get(1).map(|value| value.to_ascii_lowercase());
                if action.as_deref() == Some("state") || action.as_deref() == Some("status") {
                    Some(Command::GpuState)
                } else {
                    None
                }
            }
            "tpu" => {
                let action = parts.get(1).map(|value| value.to_ascii_lowercase());
                if action.as_deref() == Some("state") || action.as_deref() == Some("status") {
                    Some(Command::TpuState)
                } else {
                    None
                }
            }
            "predictor_switch" => parts
                .get(1)
                .map(|pt| Command::PredictorSwitch {
                    predictor_type: pt.to_string(),
                }),
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
    /// State broadcast sender (so command handlers can broadcast authoritative snapshots)
    pub state_tx: broadcast::Sender<CpuSnapshot>,
    /// Breakpoints map
    pub breakpoints: Arc<Mutex<HashMap<u32, Breakpoint>>>,
    /// Execution history
    pub history: Arc<Mutex<VecDeque<HistoryRecord>>>,
    /// Framebuffer game state
    game_state: Arc<Mutex<GameDemoState>>,
    /// Initial PC value for reset
    initial_pc: Arc<Mutex<u32>>,
    /// Clock lock to serialize clock() with reset operations
    pub clock_lock: Arc<Mutex<()>>,
    /// Reset sequence counter
    pub reset_sequence: Arc<Mutex<u64>>,
}

impl CommandContext {
    /// Create a new command context.
    fn new(
        cpu: Arc<Mutex<PipelineCpu>>,
        running: Arc<Mutex<bool>>,
        speed: Arc<Mutex<u32>>,
        tx: WsSender,
        breakpoints: Arc<Mutex<HashMap<u32, Breakpoint>>>,
        history: Arc<Mutex<VecDeque<HistoryRecord>>>,
        game_state: Arc<Mutex<GameDemoState>>,
        initial_pc: Arc<Mutex<u32>>,
        state_tx: broadcast::Sender<CpuSnapshot>,
        clock_lock: Arc<Mutex<()>>,
        reset_sequence: Arc<Mutex<u64>>,
    ) -> Self {
        Self {
            cpu,
            running,
            speed,
            tx,
            state_tx,
            breakpoints,
            history,
            game_state,
            initial_pc,
            clock_lock,
            reset_sequence,
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
    /// Framebuffer game state
    game_state: Arc<Mutex<GameDemoState>>,
    /// Maximum history size
    max_history: usize,
    /// Initial PC value for reset
    initial_pc: Arc<Mutex<u32>>,
    /// Lock used to serialize clock() calls with reset so reset is atomic
    clock_lock: Arc<Mutex<()>>,
    /// Reset sequence counter
    reset_sequence: Arc<Mutex<u64>>,
}

impl VisualizeServer {
    /// Create a new visualization server.
    pub fn new(cpu: PipelineCpu) -> Self {
        let initial_pc = cpu.pc().raw();
        Self::new_with_initial_pc(cpu, initial_pc)
    }

    /// Create a new visualization server with an explicit initial PC used by Reset.
    ///
    /// This is useful when CPU state may have advanced before server startup
    /// (e.g. warmup). Reset should still return to the original entry/start PC.
    pub fn new_with_initial_pc(cpu: PipelineCpu, initial_pc: u32) -> Self {
        let (state_tx, _) = broadcast::channel(16);
        Self {
            cpu: Arc::new(Mutex::new(cpu)),
            running: Arc::new(Mutex::new(false)),
            speed: Arc::new(Mutex::new(10)),
            state_tx,
            breakpoints: Arc::new(Mutex::new(HashMap::new())),
            history: Arc::new(Mutex::new(VecDeque::new())),
            game_state: Arc::new(Mutex::new(GameDemoState::default())),
            max_history: 10000,
            initial_pc: Arc::new(Mutex::new(initial_pc)),
            clock_lock: Arc::new(Mutex::new(())),
            reset_sequence: Arc::new(Mutex::new(0)),
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
        let game_state = self.game_state.clone();
        let max_history = self.max_history;
        let reset_sequence = self.reset_sequence.clone();

        // Spawn the continuous execution task
        let clock_lock_for_loop = self.clock_lock.clone();
        tokio::spawn(Self::run_loop(
            cpu.clone(),
            running.clone(),
            speed.clone(),
            state_tx.clone(),
            breakpoints.clone(),
            history.clone(),
            max_history,
            clock_lock_for_loop,
            reset_sequence,
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
            let game_state = game_state.clone();
            let initial_pc = self.initial_pc.clone();
            // clone a sender specifically for this connection so the outer
            // `state_tx` isn't moved into the connection closure
            let state_tx_for_conn = state_tx.clone();
            let clock_lock_for_conn = self.clock_lock.clone();
            let reset_sequence_for_conn = self.reset_sequence.clone();

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
                    let mut snapshot = cpu_guard.snapshot();
                    snapshot.reset_sequence = *reset_sequence_for_conn.lock().await;
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
                                            // include the broadcast sender so command handlers
                                            // can publish authoritative snapshots without
                                            // sending duplicate per-client JSON messages
                                            let mut ctx = CommandContext::new(
                                                cpu.clone(),
                                                running.clone(),
                                                speed.clone(),
                                                tx,
                                                breakpoints.clone(),
                                                history.clone(),
                                                game_state.clone(),
                                                initial_pc.clone(),
                                                state_tx_for_conn.clone(),
                                                clock_lock_for_conn.clone(),
                                                reset_sequence_for_conn.clone(),
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

                // Pause execution when the client disconnects so the CPU
                // does not continue running with no one watching.
                {
                    let mut running_guard = running.lock().await;
                    *running_guard = false;
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
                    let mut snapshot = cpu_guard.snapshot();
                    snapshot.reset_sequence = *ctx.reset_sequence.lock().await;
                    serde_json::to_string(&snapshot).unwrap()
                };
                ctx.send_json(json).await;
            }
            Command::Step => {
                // Execute one cycle, then broadcast the authoritative snapshot
                let snapshot = {
                    // serialize with clock_lock to avoid racing with reset
                    let _clk = ctx.clock_lock.lock().await;
                    let mut cpu_guard = ctx.cpu.lock().await;
                    let _ = cpu_guard.clock();
                    let mut snapshot = cpu_guard.snapshot();
                    snapshot.reset_sequence = *ctx.reset_sequence.lock().await;
                    snapshot
                };
                // Broadcast to all subscribers (including this client) to avoid
                // sending the same snapshot twice via both broadcast and direct send.
                let _ = ctx.state_tx.send(snapshot.clone());
            }
            Command::StepN { count } => {
                // Execute N cycles, then broadcast final snapshot and send a
                // small ack to the requesting client.
                let (executed, final_snapshot) = {
                    // serialize with clock_lock
                    let _clk = ctx.clock_lock.lock().await;
                    let mut cpu_guard = ctx.cpu.lock().await;
                    let mut executed = 0u32;
                    let target = count.max(1);

                    while executed < target {
                        if cpu_guard.is_halted() {
                            break;
                        }
                        let _ = cpu_guard.clock();
                        executed += 1;
                    }

                    let mut snapshot = cpu_guard.snapshot();
                    snapshot.reset_sequence = *ctx.reset_sequence.lock().await;
                    (executed, snapshot)
                };

                // Broadcast the final snapshot
                let _ = ctx.state_tx.send(final_snapshot.clone());

                // Send an acknowledgment with executed count
                let response = serde_json::json!({
                    "type": "stepn",
                    "success": true,
                    "requested": count,
                    "executed": executed,
                });
                ctx.send_json(response.to_string()).await;
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
                // Ensure execution is paused before and after reset to avoid a
                // race where the run loop picks up execution immediately after
                // resetting the PC. Make reset idempotent and pause execution.
                {
                    let mut running_guard = ctx.running.lock().await;
                    *running_guard = false;
                }

                let snapshot_val = {
                    // Ensure no clock() is in-flight: acquire clock_lock then cpu
                    let _clk = ctx.clock_lock.lock().await;
                    let mut cpu_guard = ctx.cpu.lock().await;
                    let initial_pc = *ctx.initial_pc.lock().await;

                    // Log PC before reset for diagnostics
                    let before_pc = cpu_guard.pc();
                    debug!(
                        "[visualize] reset requested: before_pc=0x{:08x}, initial_pc=0x{:08x}",
                        before_pc.raw(),
                        initial_pc
                    );

                    // Reset CPU and restore initial PC atomically
                    cpu_guard.reset_with_pc(crate::types::Addr::new(initial_pc));

                    // Log PC after reset
                    let after_pc = cpu_guard.pc();
                    debug!(
                        "[visualize] reset completed: after_pc=0x{:08x}",
                        after_pc.raw()
                    );
                    cpu_guard.snapshot()
                };

                let new_reset_sequence = {
                    let mut seq_guard = ctx.reset_sequence.lock().await;
                    *seq_guard = seq_guard.saturating_add(1);
                    *seq_guard
                };
                // Notify client that CPU is paused after reset
                ctx.send_status("paused").await;

                // Broadcast the authoritative snapshot via the central channel.
                // To avoid the frontend seeing a post-reset fetch that already
                // advanced the IF PC, publish a modified snapshot that forces
                // the top-level PC and IF-stage PC to the configured initial PC.
                let mut modified_snapshot = snapshot_val.clone();
                let initial_pc = *ctx.initial_pc.lock().await;
                // Force top-level pc to initial PC so UI shows the
                // expected fetch state immediately after reset.
                modified_snapshot.pc = initial_pc;
                modified_snapshot.reset_sequence = new_reset_sequence;

                // Read the actual instruction at initial_pc from memory so the
                // IF stage shows the correct first instruction instead of 0.
                let instruction = {
                    let cpu_guard = ctx.cpu.lock().await;
                    let bus = cpu_guard.bus();
                    let mut bytes = [0u8; 4];
                    for i in 0..4 {
                        match bus.read_byte(Addr::new(initial_pc + i)) {
                            Ok(byte) => bytes[i as usize] = byte.raw(),
                            Err(_) => {
                                bytes = [0u8; 4];
                                break;
                            }
                        }
                    }
                    u32::from_le_bytes(bytes)
                };
                let instruction_str = crate::visualize::snapshot::disassemble(instruction);

                modified_snapshot.pipeline.if_stage =
                    Some(crate::visualize::snapshot::IfStageInfo {
                        pc: initial_pc,
                        instruction,
                        instruction_str,
                    });
                modified_snapshot.pipeline.pre_if_stage =
                    Some(crate::visualize::snapshot::PreIfStageInfo {
                        next_pc: initial_pc + 4,
                        fetch_addr: initial_pc,
                    });
                // Reset performance counters on the snapshot so frontend cycle
                // columns restart from C0 after a reset.
                modified_snapshot.perf.cycles = 0;
                modified_snapshot.perf.instructions = 0;
                modified_snapshot.perf.ipc = 0.0;
                modified_snapshot.perf.stalls = 0;
                modified_snapshot.perf.load_use_stalls = 0;
                modified_snapshot.perf.control_hazards = 0;
                modified_snapshot.perf.load_use_stall_rate = 0.0;
                modified_snapshot.perf.control_hazard_rate = 0.0;
                modified_snapshot.perf.load_use_stall_share = 0.0;
                modified_snapshot.perf.control_hazard_share = 0.0;
                modified_snapshot.perf.branch_accuracy = None;
                modified_snapshot.perf.memory_reads = 0;
                modified_snapshot.perf.memory_writes = 0;

                let _ = ctx.state_tx.send(modified_snapshot.clone());
                // Also send the modified snapshot directly to the requesting client
                // to ensure the UI updates immediately for this client before any
                // other broadcast (defensive against ordering issues).
                let _ = ctx
                    .send_json(serde_json::to_string(&modified_snapshot).unwrap())
                    .await;
            }
            Command::Speed { value } => {
                {
                    let mut speed_guard = ctx.speed.lock().await;
                    *speed_guard = value;
                }
                let response = format!(r#"{{"status":"speed_set","speed":{}}}"#, value);
                ctx.send_json(response).await;
            }
            Command::SetInitialPc { addr } => {
                // Log and update initial PC used by Reset
                let response = {
                    let mut initial_guard = ctx.initial_pc.lock().await;
                    let old = *initial_guard;
                    *initial_guard = addr;
                    debug!(
                        "[visualize] set_initial_pc: old=0x{:08x} new=0x{:08x}",
                        old, addr
                    );
                    format!(r#"{{"status":"initial_pc_set","addr":"0x{:08x}"}}"#, addr)
                };
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
            Command::FramebufferGame { action } => {
                let response = {
                    let mut cpu_guard = ctx.cpu.lock().await;
                    let mut game_state = ctx.game_state.lock().await;

                    match action {
                        GameCommandAction::Init => {
                            game_state.reset();
                            let frame = game_state.render_rgb565();
                            match cpu_guard
                                .bus_mut()
                                .write_bytes(Addr::new(LINUX_FB_ADDR), &frame)
                            {
                                Ok(_) => serde_json::json!({
                                    "type": "framebuffer_game",
                                    "success": true,
                                    "action": "init",
                                    "tick": game_state.tick,
                                    "ball_x": game_state.ball_x,
                                    "ball_y": game_state.ball_y,
                                    "score_left": game_state.score_left,
                                    "score_right": game_state.score_right,
                                }),
                                Err(err) => serde_json::json!({
                                    "type": "framebuffer_game",
                                    "success": false,
                                    "action": "init",
                                    "error": err.to_string(),
                                }),
                            }
                        }
                        GameCommandAction::Reset => {
                            game_state.score_left = 0;
                            game_state.score_right = 0;
                            game_state.reset();
                            let frame = game_state.render_rgb565();
                            match cpu_guard
                                .bus_mut()
                                .write_bytes(Addr::new(LINUX_FB_ADDR), &frame)
                            {
                                Ok(_) => serde_json::json!({
                                    "type": "framebuffer_game",
                                    "success": true,
                                    "action": "reset",
                                    "tick": game_state.tick,
                                    "ball_x": game_state.ball_x,
                                    "ball_y": game_state.ball_y,
                                    "score_left": game_state.score_left,
                                    "score_right": game_state.score_right,
                                }),
                                Err(err) => serde_json::json!({
                                    "type": "framebuffer_game",
                                    "success": false,
                                    "action": "reset",
                                    "error": err.to_string(),
                                }),
                            }
                        }
                        GameCommandAction::State => serde_json::json!({
                            "type": "framebuffer_game",
                            "success": true,
                            "action": "state",
                            "tick": game_state.tick,
                            "ball_x": game_state.ball_x,
                            "ball_y": game_state.ball_y,
                            "vel_x": game_state.vel_x,
                            "vel_y": game_state.vel_y,
                            "left_paddle_y": game_state.left_paddle_y,
                            "right_paddle_y": game_state.right_paddle_y,
                            "score_left": game_state.score_left,
                            "score_right": game_state.score_right,
                        }),
                        GameCommandAction::Step => {
                            let key_state = cpu_guard
                                .bus()
                                .get_input_snapshot()
                                .map(|(state, _, _, _)| state)
                                .unwrap_or(0);

                            game_state.step(key_state);
                            let frame = game_state.render_rgb565();
                            match cpu_guard
                                .bus_mut()
                                .write_bytes(Addr::new(LINUX_FB_ADDR), &frame)
                            {
                                Ok(_) => serde_json::json!({
                                    "type": "framebuffer_game",
                                    "success": true,
                                    "action": "step",
                                    "tick": game_state.tick,
                                    "key_state": key_state,
                                    "ball_x": game_state.ball_x,
                                    "ball_y": game_state.ball_y,
                                    "vel_x": game_state.vel_x,
                                    "vel_y": game_state.vel_y,
                                    "left_paddle_y": game_state.left_paddle_y,
                                    "right_paddle_y": game_state.right_paddle_y,
                                    "score_left": game_state.score_left,
                                    "score_right": game_state.score_right,
                                }),
                                Err(err) => serde_json::json!({
                                    "type": "framebuffer_game",
                                    "success": false,
                                    "action": "step",
                                    "error": err.to_string(),
                                }),
                            }
                        }
                    }
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
            Command::NpuState => {
                let response = {
                    let cpu_guard = ctx.cpu.lock().await;
                    if let Some(state) = cpu_guard.bus().get_npu_snapshot() {
                        serde_json::json!({
                            "type": "npu_state",
                            "success": true,
                            "control": state.control,
                            "status": state.status,
                            "opcode": state.opcode,
                            "cycles": state.cycles,
                            "desc_addr": state.desc_addr,
                            "desc_len": state.desc_len,
                            "tasks_done": state.tasks_done,
                            "tasks_error": state.tasks_error,
                            "desc_notify_count": state.desc_notify_count,
                            "pending_desc_notify": state.pending_desc_notify,
                        })
                    } else {
                        serde_json::json!({
                            "type": "npu_state",
                            "success": false,
                            "error": "NPU peripheral not attached",
                        })
                    }
                };

                ctx.send_json(response.to_string()).await;
            }
            Command::LpuState => {
                let response = {
                    let cpu_guard = ctx.cpu.lock().await;
                    if let Some(state) = cpu_guard.bus().get_lpu_snapshot() {
                        serde_json::json!({
                            "type": "lpu_state",
                            "success": true,
                            "control": state.control,
                            "status": state.status,
                            "opcode": state.opcode,
                            "cycles": state.cycles,
                            "desc_addr": state.desc_addr,
                            "desc_len": state.desc_len,
                            "tasks_done": state.tasks_done,
                            "tasks_error": state.tasks_error,
                            "desc_notify_count": state.desc_notify_count,
                            "pending_desc_notify": state.pending_desc_notify,
                        })
                    } else {
                        serde_json::json!({
                            "type": "lpu_state",
                            "success": false,
                            "error": "LPU peripheral not attached",
                        })
                    }
                };

                ctx.send_json(response.to_string()).await;
            }
            Command::GpuState => {
                let response = {
                    let cpu_guard = ctx.cpu.lock().await;
                    if let Some(state) = cpu_guard.bus().get_gpu_snapshot() {
                        serde_json::json!({
                            "type": "gpu_state",
                            "success": true,
                            "control": state.control,
                            "status": state.status,
                            "kernel_type": state.kernel_type,
                            "precision": state.precision,
                            "kernels_executed": state.kernels_executed,
                            "cycles": state.cycles,
                            "ops_count": state.ops_count,
                            "bytes_transferred": state.bytes_transferred,
                            "tasks_done": state.tasks_done,
                            "tasks_error": state.tasks_error,
                            "error_code": state.error_code,
                            "work_queue_len": state.work_queue_len,
                            "conv_kernel_size": state.conv_kernel_size,
                            "conv_stride": state.conv_stride,
                            "conv_padding": state.conv_padding,
                            "conv_input_dims": state.conv_input_dims,
                            "conv_channels": state.conv_channels,
                        })
                    } else {
                        serde_json::json!({
                            "type": "gpu_state",
                            "success": false,
                            "error": "GPU peripheral not attached",
                        })
                    }
                };

                ctx.send_json(response.to_string()).await;
            }
            Command::TpuState => {
                let response = {
                    let cpu_guard = ctx.cpu.lock().await;
                    if let Some(state) = cpu_guard.bus().get_tpu_snapshot() {
                        serde_json::json!({
                            "type": "tpu_state",
                            "success": true,
                            "control": state.control,
                            "status": state.status,
                            "kernel_type": state.kernel_type,
                            "m": state.m,
                            "n": state.n,
                            "k": state.k,
                            "matrices_computed": state.matrices_computed,
                            "cycles": state.cycles,
                            "ops_count": state.ops_count,
                            "tasks_done": state.tasks_done,
                            "tasks_error": state.tasks_error,
                            "error_code": state.error_code,
                            "input_scale": state.input_scale,
                            "output_scale": state.output_scale,
                            "input_zero_point": state.input_zero_point,
                            "output_zero_point": state.output_zero_point,
                        })
                    } else {
                        serde_json::json!({
                            "type": "tpu_state",
                            "success": false,
                            "error": "TPU peripheral not attached",
                        })
                    }
                };

                ctx.send_json(response.to_string()).await;
            }

            Command::PredictorSwitch { predictor_type } => {
                use crate::cpu::pipeline::PredictorType;
                let response = match PredictorType::from_str_lossy(&predictor_type) {
                    Some(pt) => {
                        let mut cpu_guard = ctx.cpu.lock().await;
                        cpu_guard.switch_predictor(pt);
                        let new_type = cpu_guard.predictor_type();
                        serde_json::json!({
                            "type": "predictor_switch",
                            "success": true,
                            "predictor_type": new_type.as_str(),
                            "display_name": new_type.display_name(),
                        })
                    }
                    None => serde_json::json!({
                        "type": "predictor_switch",
                        "success": false,
                        "error": format!("Unknown predictor type: {}", predictor_type),
                    }),
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
        clock_lock: Arc<Mutex<()>>,
        reset_sequence: Arc<Mutex<u64>>,
    ) {
        loop {
            let is_running = *running.lock().await;
            if !is_running {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                continue;
            }

            let current_speed = *speed.lock().await;

            // Execute cycle and collect data
            let (snapshot, new_pc, cycles_after) = {
                let cpu_guard = cpu.lock().await;
                if cpu_guard.is_halted() {
                    let mut running_guard = running.lock().await;
                    *running_guard = false;
                    continue;
                }

                // Acquire clock_lock first, then CPU lock to maintain consistent
                // locking order with Reset/Step handlers. This prevents a race
                // where run_loop grabs the CPU lock before Reset grabs the
                // clock_lock and causes either an extra clock or a deadlock.
                drop(cpu_guard); // release earlier cpu lock obtained above
                let _clk = clock_lock.lock().await;
                let mut cpu_guard = cpu.lock().await;
                if cpu_guard.is_halted() {
                    let mut running_guard = running.lock().await;
                    *running_guard = false;
                    continue;
                }

                // Execute one clock and log result for debugging
                let before_cycles = cpu_guard.cycles();
                let clock_res = cpu_guard.clock();
                match clock_res {
                    Ok(()) => {
                        debug!(
                            "[visualize::run_loop] clock executed: before_cycles={} after_cycles={}",
                            before_cycles,
                            cpu_guard.cycles()
                        );
                    }
                    Err(err) => {
                        // Log the error and pause execution to avoid busy-looping on
                        // unserviceable memory accesses. Also mark CPU halted so
                        // subsequent cycles won't attempt to execute.
                        debug!(
                            "[visualize::run_loop] clock returned error: {:?} (cycles_before={})",
                            err, before_cycles
                        );

                        // If the error is a memory out-of-bounds on a low address
                        // (likely because no RAM was attached at 0x0), attach a
                        // small RAM region automatically and try again. This
                        // helps demo mode where a program runs at low addresses.
                        if let crate::error::SimError::MemoryOutOfBounds { addr, size: _ } = &err {
                            if addr.raw() < 0x0010_0000 {
                                debug!(
                                    "[visualize::run_loop] auto-attaching RAM at 0x00000000 to satisfy access 0x{:08x}",
                                    addr.raw()
                                );
                                // Attach 64 KiB RAM at address 0 filled with NOPs
                                let size = 64 * 1024;
                                let mut data = vec![0u8; size];
                                // Fill with RISC-V NOP (addi x0,x0,0 = 0x00000013) little-endian
                                for i in (0..size).step_by(4) {
                                    data[i] = 0x13;
                                }
                                cpu_guard.bus_mut().attach_memory(
                                    Addr::new(0),
                                    Ram::from_data(data),
                                    "Auto RAM",
                                );
                                // Continue the loop so next iteration will retry clock
                                drop(cpu_guard);
                                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                                continue;
                            }
                        }

                        // Otherwise, halt and pause execution.
                        cpu_guard.halt();
                        let mut running_guard = running.lock().await;
                        *running_guard = false;

                        // Broadcast a final snapshot so clients see halted state
                        let mut snapshot = cpu_guard.snapshot();
                        snapshot.reset_sequence = *reset_sequence.lock().await;
                        let _ = state_tx.send(snapshot);

                        // Sleep briefly to avoid tight error loop.
                        drop(cpu_guard);
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        continue;
                    }
                }

                // After executing, collect snapshot and metrics
                let new_pc = cpu_guard.pc().raw();
                let cycles_after = cpu_guard.cycles();
                let mut snapshot = cpu_guard.snapshot();
                snapshot.reset_sequence = *reset_sequence.lock().await;

                // Derive instruction info from snapshot IF stage if available
                let (instr_opt, instr_str_opt) = if let Some(if_stage) = &snapshot.pipeline.if_stage
                {
                    (
                        Some(if_stage.instruction),
                        if_stage.instruction_str.clone().into(),
                    )
                } else {
                    (None, None)
                };

                // Record to history (drop bus lock first)
                {
                    let mut history_guard = history.lock().await;
                    if history_guard.len() >= max_history {
                        history_guard.pop_front();
                    }
                    history_guard.push_back(HistoryRecord {
                        cycle: cycles_after,
                        pc: new_pc,
                        instruction: instr_opt,
                        instruction_str: instr_str_opt,
                        reg_changes: vec![],
                        mem_changes: vec![],
                    });
                }

                (snapshot, new_pc, cycles_after)
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
            // Debug log: print cycle and IF stage PC to help diagnose reset timing
            if let Some(if_stage) = &snapshot.pipeline.if_stage {
                debug!(
                    "[visualize::run_loop] broadcast cycles={} top_pc=0x{:08x} if_pc=0x{:08x}",
                    cycles_after, new_pc, if_stage.pc
                );
            } else {
                debug!(
                    "[visualize::run_loop] broadcast cycles={} top_pc=0x{:08x} if_pc=NONE",
                    cycles_after, new_pc
                );
            }

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
    fn test_parse_set_initial_pc_command() {
        let cmd = Command::parse("set_initial_pc 0x80000000");
        assert_eq!(cmd, Some(Command::SetInitialPc { addr: 0x8000_0000 }));
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

    #[test]
    fn test_parse_npu_lpu_state_commands() {
        assert_eq!(Command::parse("npu state"), Some(Command::NpuState));
        assert_eq!(Command::parse("npu status"), Some(Command::NpuState));
        assert_eq!(Command::parse("lpu state"), Some(Command::LpuState));
        assert_eq!(Command::parse("lpu status"), Some(Command::LpuState));
        assert_eq!(Command::parse("gpu state"), Some(Command::GpuState));
        assert_eq!(Command::parse("gpu status"), Some(Command::GpuState));
        assert_eq!(Command::parse("tpu state"), Some(Command::TpuState));
        assert_eq!(Command::parse("tpu status"), Some(Command::TpuState));
        assert_eq!(Command::parse("npu"), None);
        assert_eq!(Command::parse("lpu"), None);
        assert_eq!(Command::parse("gpu"), None);
        assert_eq!(Command::parse("tpu"), None);
    }

    #[test]
    fn test_parse_stepn_command() {
        assert_eq!(
            Command::parse("stepn 100"),
            Some(Command::StepN { count: 100 })
        );
        assert_eq!(
            Command::parse("step_n 42"),
            Some(Command::StepN { count: 42 })
        );
        assert_eq!(Command::parse("stepn"), None);
    }

    #[test]
    fn test_parse_framebuffer_game_commands() {
        assert_eq!(
            Command::parse("fb_game init"),
            Some(Command::FramebufferGame {
                action: GameCommandAction::Init,
            })
        );
        assert_eq!(
            Command::parse("fb_game"),
            Some(Command::FramebufferGame {
                action: GameCommandAction::Step,
            })
        );
        assert_eq!(
            Command::parse("fb_game reset"),
            Some(Command::FramebufferGame {
                action: GameCommandAction::Reset,
            })
        );
        assert_eq!(
            Command::parse("fb_game state"),
            Some(Command::FramebufferGame {
                action: GameCommandAction::State,
            })
        );
    }

    #[test]
    fn test_game_demo_state_step_progresses_tick() {
        let mut game = GameDemoState::default();
        let before_tick = game.tick;
        let before_ball_x = game.ball_x;
        game.step(0);
        assert!(game.tick > before_tick);
        assert_ne!(game.ball_x, before_ball_x);
    }

    #[tokio::test]
    async fn test_initial_pc_update_and_reset_behaviour() {
        use crate::memory::Bus;
        use crate::types::Addr;

        // Create CPU with initial PC 0x1000
        let bus = Bus::new();
        let cpu = PipelineCpu::with_pc(bus, Addr::new(0x1000));
        let server = VisualizeServer::new(cpu);

        // initial_pc should be 0x1000
        {
            let ip = *server.initial_pc.lock().await;
            assert_eq!(ip, 0x1000);
        }

        // Update initial_pc to 0x2000
        {
            let mut ip_lock = server.initial_pc.lock().await;
            *ip_lock = 0x2000;
        }

        // Mutate CPU pc to some other value, then reset using reset_with_pc
        {
            let mut cpu_guard = server.cpu.lock().await;
            cpu_guard.set_pc(Addr::new(0x3000));
            let initial = *server.initial_pc.lock().await;
            cpu_guard.reset_with_pc(Addr::new(initial));
            assert_eq!(cpu_guard.pc().raw(), 0x2000);
        }
    }
}

/// Start the visualization server with a CPU.
pub async fn start_visualize_server(cpu: PipelineCpu, port: u16) -> Result<()> {
    let server = VisualizeServer::new(cpu);
    server.start(port).await
}

/// Start visualization server with an explicit initial PC for Reset.
pub async fn start_visualize_server_with_initial_pc(
    cpu: PipelineCpu,
    port: u16,
    initial_pc: u32,
) -> Result<()> {
    let server = VisualizeServer::new_with_initial_pc(cpu, initial_pc);
    server.start(port).await
}
