//! GDB Remote Protocol implementation for DiffTest
//!
//! This module implements the GDB Remote Serial Protocol client
//! for communicating with QEMU's GDB stub.

use std::io::{BufReader, BufWriter, Read, Write};
use std::net::TcpStream;

/// GDB Protocol handler
pub struct GdbProtocol {
    /// TCP stream for communication
    stream: TcpStream,
    /// Reader for the stream
    reader: BufReader<TcpStream>,
    /// Writer for the stream
    writer: BufWriter<TcpStream>,
}

impl GdbProtocol {
    /// Create a new GDB protocol handler
    pub fn new(stream: TcpStream) -> Self {
        let reader = BufReader::new(stream.try_clone().expect("Failed to clone stream"));
        let writer = BufWriter::new(stream.try_clone().expect("Failed to clone stream"));

        Self {
            stream,
            reader,
            writer,
        }
    }

    /// Perform initial handshake with GDB server
    pub fn handshake(&mut self) -> Result<(), String> {
        // Read any initial data
        let mut buf = [0u8; 256];
        let timeout = std::time::Duration::from_secs(5);

        self.stream
            .set_read_timeout(Some(timeout))
            .map_err(|e| format!("Failed to set timeout: {}", e))?;

        // Try to read acknowledgment or signal
        match self.reader.read(&mut buf) {
            Ok(n) if n > 0 => {
                log::debug!("Received {} bytes during handshake", n);
            }
            Ok(_) => {
                log::debug!("No initial data from GDB server");
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                log::debug!("No immediate data from GDB server (expected)");
            }
            Err(e) => {
                return Err(format!("Handshake read error: {}", e));
            }
        }

        // Send a simple query to verify connection
        self.send_packet("qSupported")?;

        let response = self.recv_packet()?;
        log::info!(
            "GDB server capabilities: {}",
            String::from_utf8_lossy(&response)
        );

        Ok(())
    }

    /// Send a packet to GDB server
    pub fn send_packet(&mut self, data: &str) -> Result<(), String> {
        // Format: $data#checksum
        let checksum: u8 = data.bytes().fold(0u8, |a, b| a.wrapping_add(b));
        let packet = format!("${}#{:02x}", data, checksum);

        log::trace!("Sending: {}", packet);

        self.writer
            .write_all(packet.as_bytes())
            .map_err(|e| format!("Write error: {}", e))?;
        self.writer
            .flush()
            .map_err(|e| format!("Flush error: {}", e))?;

        // Wait for acknowledgment (+)
        let mut ack = [0u8; 1];
        match self.reader.read(&mut ack) {
            Ok(1) if ack[0] == b'+' => {
                log::trace!("Received ACK");
                Ok(())
            }
            Ok(1) if ack[0] == b'-' => Err("Negative acknowledgment received".to_string()),
            Ok(_) => {
                log::warn!("Unexpected acknowledgment: {:?}", ack);
                Ok(()) // Continue anyway
            }
            Err(e) => Err(format!("Failed to read acknowledgment: {}", e)),
        }
    }

    /// Receive a packet from GDB server
    pub fn recv_packet(&mut self) -> Result<Vec<u8>, String> {
        let mut buf = [0u8; 4096];
        let n = self
            .reader
            .read(&mut buf)
            .map_err(|e| format!("Read error: {}", e))?;

        if n == 0 {
            return Err("Connection closed".to_string());
        }

        // Find packet boundaries
        let data = &buf[..n];

        // Skip any acknowledgment characters
        let start = data.iter().position(|&b| b == b'$').unwrap_or(0);
        let data = &data[start..];

        // Find end marker (#)
        if let Some(end_pos) = data.iter().position(|&b| b == b'#') {
            // Extract packet data (between $ and #)
            let packet_data = &data[1..end_pos];

            // Verify checksum
            if data.len() > end_pos + 2 {
                let checksum_hex = &data[end_pos + 1..end_pos + 3];
                let expected = parse_hex_bytes(checksum_hex);
                let computed: u8 = packet_data.iter().fold(0u8, |a, &b| a.wrapping_add(b));

                if expected != computed {
                    log::warn!(
                        "Checksum mismatch: expected {:02x}, got {:02x}",
                        expected,
                        computed
                    );
                }
            }

            // Send acknowledgment
            self.writer
                .write_all(&[b'+'])
                .map_err(|e| format!("Failed to send ACK: {}", e))?;
            self.writer
                .flush()
                .map_err(|e| format!("Flush error: {}", e))?;

            Ok(packet_data.to_vec())
        } else {
            Err("Incomplete packet".to_string())
        }
    }

    /// Read a register value
    pub fn read_register(&mut self, reg: u8) -> Result<u32, String> {
        let cmd = format!("p{:02x}", reg);
        self.send_packet(&cmd)?;

        let response = self.recv_packet()?;
        let hex_str = String::from_utf8_lossy(&response);

        if hex_str.starts_with("E") {
            return Err(format!("GDB error: {}", hex_str));
        }

        // Parse hex value (little-endian in GDB)
        parse_hex_u32(&hex_str)
    }

    /// Write a register value
    pub fn write_register(&mut self, reg: u8, value: u32) -> Result<(), String> {
        let cmd = format!("P{:02x}={:08x}", reg, value);
        self.send_packet(&cmd)?;

        let response = self.recv_packet()?;
        let resp_str = String::from_utf8_lossy(&response);

        if resp_str != "OK" {
            return Err(format!("Write register failed: {}", resp_str));
        }

        Ok(())
    }

    /// Read memory
    pub fn read_memory(&mut self, addr: u32, len: usize) -> Result<Vec<u8>, String> {
        let cmd = format!("m{:08x},{:x}", addr, len);
        self.send_packet(&cmd)?;

        let response = self.recv_packet()?;
        let hex_str = String::from_utf8_lossy(&response);

        if hex_str.starts_with("E") {
            return Err(format!("Memory read error: {}", hex_str));
        }

        // Parse hex bytes
        let mut bytes = Vec::with_capacity(len);
        for i in (0..hex_str.len()).step_by(2) {
            if i + 2 <= hex_str.len() {
                let byte_hex = &hex_str[i..i + 2];
                bytes.push(u8::from_str_radix(byte_hex, 16).unwrap_or(0));
            }
        }

        Ok(bytes)
    }

    /// Write memory
    pub fn write_memory(&mut self, addr: u32, data: &[u8]) -> Result<(), String> {
        let hex_data: String = data.iter().map(|b| format!("{:02x}", b)).collect();
        let cmd = format!("M{:08x},{:x}:{}", addr, data.len(), hex_data);
        self.send_packet(&cmd)?;

        let response = self.recv_packet()?;
        let resp_str = String::from_utf8_lossy(&response);

        if resp_str != "OK" {
            return Err(format!("Memory write failed: {}", resp_str));
        }

        Ok(())
    }

    /// Single step
    pub fn step(&mut self) -> Result<(), String> {
        self.send_packet("s")?;

        // Read response (usually S05 = SIGTRAP)
        let response = self.recv_packet()?;
        let resp_str = String::from_utf8_lossy(&response);

        log::trace!("Step response: {}", resp_str);

        Ok(())
    }

    /// Continue execution
    pub fn continue_exec(&mut self) -> Result<(), String> {
        self.send_packet("c")?;

        // This will block until the program stops
        let response = self.recv_packet()?;
        let resp_str = String::from_utf8_lossy(&response);

        log::debug!("Continue response: {}", resp_str);

        Ok(())
    }

    /// Disconnect from GDB server
    pub fn disconnect(&mut self) -> Result<(), String> {
        self.send_packet("D")?;

        let response = self.recv_packet()?;
        let resp_str = String::from_utf8_lossy(&response);

        log::info!("Disconnected: {}", resp_str);

        Ok(())
    }
}

/// Parse hex bytes
fn parse_hex_bytes(hex: &[u8]) -> u8 {
    if hex.len() < 2 {
        return 0;
    }
    let high = char::from(hex[0]).to_digit(16).unwrap_or(0);
    let low = char::from(hex[1]).to_digit(16).unwrap_or(0);
    ((high << 4) | low) as u8
}

/// Parse a hex string as u32 (little-endian)
fn parse_hex_u32(hex: &str) -> Result<u32, String> {
    if hex.len() < 8 {
        return Err(format!("Invalid hex length: {}", hex.len()));
    }

    // GDB returns values in target byte order (little-endian for RISC-V)
    let bytes: [u8; 4] = [
        u8::from_str_radix(&hex[0..2], 16).map_err(|e| e.to_string())?,
        u8::from_str_radix(&hex[2..4], 16).map_err(|e| e.to_string())?,
        u8::from_str_radix(&hex[4..6], 16).map_err(|e| e.to_string())?,
        u8::from_str_radix(&hex[6..8], 16).map_err(|e| e.to_string())?,
    ];

    Ok(u32::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_bytes() {
        assert_eq!(parse_hex_bytes(b"00"), 0x00);
        assert_eq!(parse_hex_bytes(b"FF"), 0xFF);
        assert_eq!(parse_hex_bytes(b"5a"), 0x5A);
    }

    #[test]
    fn test_parse_hex_u32() {
        // Little-endian: 0x12345678 stored as "78563412"
        let result = parse_hex_u32("78563412");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0x12345678);
    }

    #[test]
    fn test_parse_hex_u32_zero() {
        let result = parse_hex_u32("00000000");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }
}
