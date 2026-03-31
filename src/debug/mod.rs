//! GDB Remote Protocol Debug Interface
//!
//! This module implements a GDB Remote Serial Protocol server for
//! debugging the RISC-V simulator. It allows connecting GDB or VSCode
//! to debug programs running in the simulator.
//!
//! # Supported Commands
//!
//! - `?` - Get reason for halt
//! - `g` - Read all registers
//! - `G` - Write all registers
//! - `m` - Read memory
//! - `M` - Write memory
//! - `c` - Continue execution
//! - `s` - Single step
//! - `Z0` - Insert software breakpoint
//! - `z0` - Remove software breakpoint
//! - `qSupported` - Query supported features
//! - `qAttached` - Query attach state
//!
//! # Example
//!
//! ```rust,no_run
//! use mycpu::debug::GdbServer;
//! use mycpu::cpu::Cpu;
//!
//! async fn run_debugger(cpu: &mut Cpu) {
//!     let mut server = GdbServer::new(cpu, 1234);
//!     server.run().await.unwrap();
//! }
//! ```

use crate::cpu::{Cpu, CpuState};
use crate::error::{Result, SimError};
use crate::memory::Bus;
use crate::types::{Addr, Byte, RegIdx, Word};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

/// GDB Remote Protocol server
pub struct GdbServer {
    /// Port to listen on
    port: u16,
    /// Target CPU instance
    cpu: Arc<Mutex<Cpu>>,
    /// Breakpoint addresses
    breakpoints: Arc<Mutex<HashSet<u32>>>,
    /// Whether the server should stop (reserved for future use)
    #[allow(dead_code)]
    running: Arc<Mutex<bool>>,
}

impl GdbServer {
    /// Create a new GDB server
    pub fn new(port: u16) -> Self {
        let cpu = Cpu::new(Bus::new());
        Self::with_cpu(port, cpu)
    }

    /// Create a new GDB server with a specific CPU instance
    pub fn with_cpu(port: u16, cpu: Cpu) -> Self {
        Self {
            port,
            cpu: Arc::new(Mutex::new(cpu)),
            breakpoints: Arc::new(Mutex::new(HashSet::new())),
            running: Arc::new(Mutex::new(true)),
        }
    }

    /// Start the GDB server
    pub async fn run(&self) -> Result<()> {
        let addr: SocketAddr = ([127, 0, 0, 1], self.port).into();
        let listener = TcpListener::bind(&addr)
            .await
            .map_err(|e| SimError::IoError(e.to_string()))?;

        log::info!("GDB server listening on {}", addr);

        loop {
            let (stream, peer) = listener
                .accept()
                .await
                .map_err(|e| SimError::IoError(e.to_string()))?;

            log::info!("GDB client connected from {}", peer);

            // Handle the connection
            self.handle_connection(stream).await?;
        }
    }

    /// Handle a single GDB client connection
    async fn handle_connection(&self, mut stream: TcpStream) -> Result<()> {
        let mut buffer = [0u8; 4096];

        loop {
            let n = stream
                .read(&mut buffer)
                .await
                .map_err(|e| SimError::IoError(e.to_string()))?;

            if n == 0 {
                log::info!("GDB client disconnected");
                break;
            }

            // Process the received data
            let response = self.process_data(&buffer[..n]).await;
            stream
                .write_all(&response)
                .await
                .map_err(|e| SimError::IoError(e.to_string()))?;
            stream
                .flush()
                .await
                .map_err(|e| SimError::IoError(e.to_string()))?;
        }

        Ok(())
    }

    /// Process incoming data and generate response
    async fn process_data(&self, data: &[u8]) -> Vec<u8> {
        // Handle GDB protocol packets
        // Packets are formatted as: $packet-data#checksum

        let mut response = Vec::new();

        for &byte in data {
            match byte {
                b'$' => {
                    // Start of packet - will be handled by packet parsing
                }
                b'+' => {
                    // Acknowledgment - no response needed
                }
                b'-' => {
                    // Negative acknowledgment - retransmit last packet
                    // For simplicity, we just send an empty ack
                    response.push(b'+');
                }
                b'\x03' => {
                    // Ctrl+C - interrupt
                    response.push(b'+');
                    response.extend_from_slice(b"$S05#b8");
                }
                _ => {}
            }
        }

        // Try to parse a complete packet
        if let Some((packet, _checksum)) = self.parse_packet(data) {
            let resp = self.handle_packet(&packet).await;
            response.push(b'+'); // Ack
            response.extend_from_slice(&self.format_packet(&resp));
        }

        response
    }

    /// Parse a GDB packet from raw data
    fn parse_packet<'a>(&self, data: &'a [u8]) -> Option<(&'a [u8], u8)> {
        // Find packet start
        let start = data.iter().position(|&b| b == b'$')?;
        let rest = &data[start + 1..];

        // Find packet end (# followed by 2 hex digits)
        let end = rest.iter().position(|&b| b == b'#')?;
        let packet = &rest[..end];

        // Parse checksum
        if rest.len() < end + 3 {
            return None;
        }
        let checksum_hex = &rest[end + 1..end + 3];
        let checksum = self.parse_hex_byte(checksum_hex)?;

        // Verify checksum
        let computed: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        if computed != checksum {
            log::warn!("Checksum mismatch: expected {}, got {}", computed, checksum);
        }

        Some((packet, checksum))
    }

    /// Format a response packet
    fn format_packet(&self, data: &str) -> Vec<u8> {
        let checksum: u8 = data.bytes().fold(0, |a, b| a.wrapping_add(b));
        format!("${}#{:02x}", data, checksum).into_bytes()
    }

    /// Parse a two-digit hex byte
    fn parse_hex_byte(&self, hex: &[u8]) -> Option<u8> {
        if hex.len() != 2 {
            return None;
        }
        let high = char::from(hex[0]).to_digit(16)?;
        let low = char::from(hex[1]).to_digit(16)?;
        Some(((high << 4) | low) as u8)
    }

    /// Handle a GDB packet command
    async fn handle_packet(&self, packet: &[u8]) -> String {
        if packet.is_empty() {
            return "".to_string();
        }

        let cmd = packet[0] as char;
        let args = std::str::from_utf8(&packet[1..]).unwrap_or("");

        match cmd {
            '?' => self.cmd_halt_reason(),
            'g' => self.cmd_read_registers().await,
            'G' => self.cmd_write_registers(args).await,
            'm' => self.cmd_read_memory(args).await,
            'M' => self.cmd_write_memory(args).await,
            'c' => self.cmd_continue(args).await,
            's' => self.cmd_step(args).await,
            'p' => self.cmd_read_register(args).await,
            'P' => self.cmd_write_register(args).await,
            'Z' => self.cmd_insert_breakpoint(args).await,
            'z' => self.cmd_remove_breakpoint(args).await,
            'q' => self.cmd_query(args),
            'H' => self.cmd_set_thread(args),
            _ => {
                log::debug!("Unsupported GDB command: {}", cmd);
                "".to_string()
            }
        }
    }

    /// Command: Get halt reason
    fn cmd_halt_reason(&self) -> String {
        // S05 = SIGTRAP (breakpoint hit)
        "S05".to_string()
    }

    /// Command: Read all registers
    async fn cmd_read_registers(&self) -> String {
        let cpu = self.cpu.lock().await;
        let mut out = String::with_capacity(33 * 8);

        for i in 0..32 {
            let value = cpu.registers().read(RegIdx::new(i as u8)).raw();
            out.push_str(&Self::encode_u32_le(value));
        }

        out.push_str(&Self::encode_u32_le(cpu.pc().raw()));
        out
    }

    /// Command: Write all registers
    async fn cmd_write_registers(&self, args: &str) -> String {
        let expected_len = 33 * 8;
        if args.len() != expected_len {
            return "E01".to_string();
        }

        let mut cpu = self.cpu.lock().await;

        for i in 0..32 {
            let start = i * 8;
            let end = start + 8;
            let Some(value) = Self::decode_u32_le(&args[start..end]) else {
                return "E01".to_string();
            };
            cpu.registers_mut()
                .write(RegIdx::new(i as u8), Word::new(value));
        }

        let Some(pc) = Self::decode_u32_le(&args[32 * 8..33 * 8]) else {
            return "E01".to_string();
        };
        cpu.set_pc(Addr::new(pc));

        "OK".to_string()
    }

    /// Command: Read memory
    async fn cmd_read_memory(&self, args: &str) -> String {
        // Format: addr,length
        let parts: Vec<&str> = args.split(',').collect();
        if parts.len() != 2 {
            return "".to_string();
        }

        let addr = match u32::from_str_radix(parts[0], 16) {
            Ok(v) => v,
            Err(_) => return "E01".to_string(),
        };
        let len = match usize::from_str_radix(parts[1], 16) {
            Ok(v) => v,
            Err(_) => return "E01".to_string(),
        };

        let cpu = self.cpu.lock().await;
        let mut result = String::with_capacity(len * 2);
        for i in 0..len {
            match cpu.read_byte(Addr::new(addr.wrapping_add(i as u32))) {
                Ok(byte) => result.push_str(&format!("{:02x}", byte.raw())),
                Err(_) => return "E01".to_string(),
            }
        }

        result
    }

    /// Command: Write memory
    async fn cmd_write_memory(&self, args: &str) -> String {
        let Some((head, payload)) = args.split_once(':') else {
            return "E01".to_string();
        };

        let parts: Vec<&str> = head.split(',').collect();
        if parts.len() != 2 {
            return "E01".to_string();
        }

        let addr = match u32::from_str_radix(parts[0], 16) {
            Ok(v) => v,
            Err(_) => return "E01".to_string(),
        };
        let len = match usize::from_str_radix(parts[1], 16) {
            Ok(v) => v,
            Err(_) => return "E01".to_string(),
        };

        if payload.len() != len * 2 {
            return "E01".to_string();
        }

        let mut cpu = self.cpu.lock().await;
        for i in 0..len {
            let b = &payload[i * 2..i * 2 + 2];
            let byte = match u8::from_str_radix(b, 16) {
                Ok(v) => v,
                Err(_) => return "E01".to_string(),
            };

            if cpu
                .write_byte(Addr::new(addr.wrapping_add(i as u32)), Byte::new(byte))
                .is_err()
            {
                return "E01".to_string();
            }
        }

        "OK".to_string()
    }

    /// Command: Continue execution
    async fn cmd_continue(&self, args: &str) -> String {
        if !args.is_empty() {
            if let Ok(addr) = u32::from_str_radix(args, 16) {
                let mut cpu = self.cpu.lock().await;
                cpu.set_pc(Addr::new(addr));
            }
        }

        for _ in 0..1_000_000 {
            {
                let cpu = self.cpu.lock().await;
                let pc = cpu.pc().raw();
                if self.breakpoints.lock().await.contains(&pc) {
                    return "S05".to_string();
                }
            }

            let mut cpu = self.cpu.lock().await;
            match cpu.step() {
                Ok(_) => {
                    let pc = cpu.pc().raw();
                    if self.breakpoints.lock().await.contains(&pc) {
                        return "S05".to_string();
                    }
                }
                Err(SimError::Halted) => return "W00".to_string(),
                Err(SimError::Breakpoint(_) | SimError::Ebreak(_)) => return "S05".to_string(),
                Err(_) => return "E01".to_string(),
            }
        }

        "S05".to_string()
    }

    /// Command: Single step
    async fn cmd_step(&self, args: &str) -> String {
        if !args.is_empty() {
            if let Ok(addr) = u32::from_str_radix(args, 16) {
                let mut cpu = self.cpu.lock().await;
                cpu.set_pc(Addr::new(addr));
            }
        }

        let mut cpu = self.cpu.lock().await;
        match cpu.step() {
            Ok(_) => "S05".to_string(),
            Err(SimError::Halted) => "W00".to_string(),
            Err(SimError::Breakpoint(_) | SimError::Ebreak(_)) => "S05".to_string(),
            Err(_) => "E01".to_string(),
        }
    }

    /// Command: Read single register (pXX)
    async fn cmd_read_register(&self, args: &str) -> String {
        let Ok(reg_idx) = u32::from_str_radix(args, 16) else {
            return "E01".to_string();
        };

        let cpu = self.cpu.lock().await;
        if reg_idx < 32 {
            let value = cpu.registers().read(RegIdx::new(reg_idx as u8)).raw();
            return Self::encode_u32_le(value);
        }
        if reg_idx == 32 {
            return Self::encode_u32_le(cpu.pc().raw());
        }

        "E01".to_string()
    }

    /// Command: Write single register (PXX=vvvvvvvv)
    async fn cmd_write_register(&self, args: &str) -> String {
        let Some((reg_hex, value_hex)) = args.split_once('=') else {
            return "E01".to_string();
        };

        let Ok(reg_idx) = u32::from_str_radix(reg_hex, 16) else {
            return "E01".to_string();
        };

        let Some(value) = Self::decode_u32_le(value_hex) else {
            return "E01".to_string();
        };

        let mut cpu = self.cpu.lock().await;
        if reg_idx < 32 {
            cpu.registers_mut()
                .write(RegIdx::new(reg_idx as u8), Word::new(value));
            return "OK".to_string();
        }
        if reg_idx == 32 {
            cpu.set_pc(Addr::new(value));
            return "OK".to_string();
        }

        "E01".to_string()
    }

    /// Command: Insert breakpoint
    async fn cmd_insert_breakpoint(&self, args: &str) -> String {
        // Format: type,addr,kind
        let parts: Vec<&str> = args.split(',').collect();
        if parts.len() != 3 {
            return "E01".to_string();
        }

        // Only support software breakpoints (type 0)
        if parts[0] != "0" {
            return "".to_string();
        }

        if let Ok(addr) = u32::from_str_radix(parts[1], 16) {
            self.breakpoints.lock().await.insert(addr);
            return "OK".to_string();
        }

        "E01".to_string()
    }

    /// Command: Remove breakpoint
    async fn cmd_remove_breakpoint(&self, args: &str) -> String {
        // Format: type,addr,kind
        let parts: Vec<&str> = args.split(',').collect();
        if parts.len() != 3 {
            return "E01".to_string();
        }

        if let Ok(addr) = u32::from_str_radix(parts[1], 16) {
            self.breakpoints.lock().await.remove(&addr);
            return "OK".to_string();
        }

        "E01".to_string()
    }

    /// Command: Query commands
    fn cmd_query(&self, args: &str) -> String {
        if args.starts_with("Supported") {
            // Report supported features
            "PacketSize=4096;BreakpointCommands+;swbreak+;hwbreak-".to_string()
        } else if args.starts_with("Attached") {
            // We're always "attached" to the process
            "1".to_string()
        } else if args.starts_with("C") {
            // Current thread ID
            "QC1".to_string()
        } else if args.starts_with("fThreadInfo") {
            // First thread info
            "m1".to_string()
        } else if args.starts_with("sThreadInfo") {
            // Subsequent thread info (end of list)
            "l".to_string()
        } else {
            "".to_string()
        }
    }

    /// Command: Set thread for subsequent operations
    fn cmd_set_thread(&self, _args: &str) -> String {
        "OK".to_string()
    }

    /// Check if address is a breakpoint
    pub async fn is_breakpoint(&self, addr: u32) -> bool {
        self.breakpoints.lock().await.contains(&addr)
    }

    fn encode_u32_le(value: u32) -> String {
        let bytes = value.to_le_bytes();
        format!(
            "{:02x}{:02x}{:02x}{:02x}",
            bytes[0], bytes[1], bytes[2], bytes[3]
        )
    }

    fn decode_u32_le(hex: &str) -> Option<u32> {
        if hex.len() != 8 {
            return None;
        }

        let b0 = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let b1 = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b2 = u8::from_str_radix(&hex[4..6], 16).ok()?;
        let b3 = u8::from_str_radix(&hex[6..8], 16).ok()?;
        Some(u32::from_le_bytes([b0, b1, b2, b3]))
    }
}

/// GDB debug session helper for CPU
pub struct DebugSession<'a> {
    /// Reference to CPU
    cpu: &'a mut Cpu,
    /// Breakpoints
    breakpoints: HashSet<u32>,
}

impl<'a> DebugSession<'a> {
    /// Create a new debug session
    pub fn new(cpu: &'a mut Cpu) -> Self {
        Self {
            cpu,
            breakpoints: HashSet::new(),
        }
    }

    /// Add a breakpoint
    pub fn add_breakpoint(&mut self, addr: u32) {
        self.breakpoints.insert(addr);
    }

    /// Remove a breakpoint
    pub fn remove_breakpoint(&mut self, addr: u32) {
        self.breakpoints.remove(&addr);
    }

    /// Check if address is a breakpoint
    pub fn is_breakpoint(&self, addr: u32) -> bool {
        self.breakpoints.contains(&addr)
    }

    /// Single step the CPU
    pub fn step(&mut self) -> Result<CpuState> {
        self.cpu.step()
    }

    /// Run until breakpoint or error
    pub fn run(&mut self) -> Result<CpuState> {
        loop {
            let pc = self.cpu.pc().raw();
            if self.is_breakpoint(pc) {
                return Ok(self.cpu.state());
            }
            self.cpu.step()?;
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Ram;

    fn create_server_with_ram() -> GdbServer {
        let mut bus = Bus::new();
        bus.attach_memory(Addr::new(0), Ram::new(4096), "RAM");
        let cpu = Cpu::new(bus);
        GdbServer::with_cpu(1234, cpu)
    }

    #[test]
    fn test_gdb_server_new() {
        let server = GdbServer::new(1234);
        assert_eq!(server.port, 1234);
    }

    #[test]
    fn test_format_packet() {
        let server = GdbServer::new(1234);
        let packet = server.format_packet("OK");
        assert_eq!(packet, b"$OK#9a");
    }

    #[test]
    fn test_parse_hex_byte() {
        let server = GdbServer::new(1234);
        assert_eq!(server.parse_hex_byte(b"00"), Some(0x00));
        assert_eq!(server.parse_hex_byte(b"FF"), Some(0xFF));
        assert_eq!(server.parse_hex_byte(b"5a"), Some(0x5A));
        assert_eq!(server.parse_hex_byte(b"GG"), None);
    }

    #[test]
    fn test_cmd_halt_reason() {
        let server = GdbServer::new(1234);
        assert_eq!(server.cmd_halt_reason(), "S05");
    }

    #[test]
    fn test_cmd_query_supported() {
        let server = GdbServer::new(1234);
        let result = server.cmd_query("Supported");
        assert!(result.contains("PacketSize"));
    }

    #[test]
    fn test_breakpoint() {
        let mut set = HashSet::new();
        set.insert(0x8000_0000u32);
        assert!(set.contains(&0x8000_0000u32));
        assert!(!set.contains(&0x8000_0004u32));
    }

    #[tokio::test]
    async fn test_gdb_read_write_registers() {
        let server = create_server_with_ram();

        {
            let mut cpu = server.cpu.lock().await;
            cpu.registers_mut()
                .write(RegIdx::new(1), Word::new(0x1234_5678));
            cpu.set_pc(Addr::new(0x1000));
        }

        let regs = server.cmd_read_registers().await;
        assert_eq!(&regs[8..16], "78563412");
        assert_eq!(&regs[32 * 8..33 * 8], "00100000");

        let mut payload = "00".repeat(33 * 4);
        payload.replace_range(8..16, "EFBEADDE");
        payload.replace_range(32 * 8..33 * 8, "00020000");
        assert_eq!(server.cmd_write_registers(&payload).await, "OK");

        let cpu = server.cpu.lock().await;
        assert_eq!(cpu.registers().read(RegIdx::new(1)).raw(), 0xDEAD_BEEF);
        assert_eq!(cpu.pc().raw(), 0x0000_0200);
    }

    #[tokio::test]
    async fn test_gdb_read_write_memory() {
        let server = create_server_with_ram();

        assert_eq!(server.cmd_write_memory("10,4:01020304").await, "OK");
        let data = server.cmd_read_memory("10,4").await;
        assert_eq!(data, "01020304");
    }
}
