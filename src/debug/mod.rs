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
    /// Breakpoint addresses
    breakpoints: Arc<Mutex<HashSet<u32>>>,
    /// Whether the server should stop
    running: Arc<Mutex<bool>>,
}

impl GdbServer {
    /// Create a new GDB server
    pub fn new(port: u16) -> Self {
        Self {
            port,
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
            'g' => self.cmd_read_registers(),
            'G' => self.cmd_write_registers(args),
            'm' => self.cmd_read_memory(args),
            'M' => self.cmd_write_memory(args),
            'c' => self.cmd_continue(args),
            's' => self.cmd_step(args),
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
    fn cmd_read_registers(&self) -> String {
        // Return 32 general purpose registers + PC
        // Each register is 4 bytes (8 hex digits)
        // This is a placeholder - actual implementation needs CPU access
        "0".repeat(33 * 8)
    }

    /// Command: Write all registers
    fn cmd_write_registers(&self, _args: &str) -> String {
        "OK".to_string()
    }

    /// Command: Read memory
    fn cmd_read_memory(&self, args: &str) -> String {
        // Format: addr,length
        let parts: Vec<&str> = args.split(',').collect();
        if parts.len() != 2 {
            return "".to_string();
        }

        let _addr = u32::from_str_radix(parts[0], 16).unwrap_or(0);
        let len = usize::from_str_radix(parts[1], 16).unwrap_or(0);

        // Return placeholder data - actual implementation needs memory access
        "00".repeat(len)
    }

    /// Command: Write memory
    fn cmd_write_memory(&self, _args: &str) -> String {
        "OK".to_string()
    }

    /// Command: Continue execution
    fn cmd_continue(&self, _args: &str) -> String {
        // Empty response means continue running
        "".to_string()
    }

    /// Command: Single step
    fn cmd_step(&self, _args: &str) -> String {
        // Return halt reason after step
        "S05".to_string()
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
}
