//! Built-in RV32I framebuffer demo program.
//!
//! This module provides a tiny pre-encoded RV32I program that writes pixels
//! directly to the Linux framebuffer preset address. It is used to validate
//! the end-to-end path: "CPU program -> RAM framebuffer -> WebSocket -> frontend".

use crate::error::Result;
use crate::memory::Bus;
use crate::types::Addr;

/// Linux framebuffer preset base address.
pub const LINUX_FB_ADDR: u32 = 0x80E0_0000;
/// Linux framebuffer preset width.
pub const LINUX_FB_WIDTH: u32 = 320;
/// Linux framebuffer preset height.
pub const LINUX_FB_HEIGHT: u32 = 240;
/// Linux framebuffer preset source format.
pub const LINUX_FB_FORMAT: &str = "rgb565";

/// Default program load address.
pub const DEFAULT_PROGRAM_BASE: u32 = 0x8000_0000;

/// Number of pixels written by the built-in demo program.
///
/// 640 pixels = 2 scanlines of 320x240, enough to be clearly visible.
pub const DEMO_PIXELS: u16 = 640;

/// RGB565 color used by the built-in demo program (blue).
const DEMO_COLOR_RGB565: i16 = 0x001F;

/// Build the built-in framebuffer writer program as little-endian bytes.
///
/// Program logic (RV32I):
/// - t0 = framebuffer base (0x80E0_0000)
/// - t2 = color (0x001F)
/// - t3 = pixel counter (640)
/// - loop: sh t2, 0(t0); addi t0, t0, 2; addi t3, t3, -1; bne t3, x0, loop
/// - final: jal x0, 0 (self loop)
pub fn demo_program_bytes() -> Vec<u8> {
    let words = [
        encode_lui(5, LINUX_FB_ADDR >> 12),
        encode_addi(7, 0, DEMO_COLOR_RGB565),
        encode_addi(28, 0, DEMO_PIXELS as i16),
        encode_sh(7, 5, 0),
        encode_addi(5, 5, 2),
        encode_addi(28, 28, -1),
        encode_bne(28, 0, -12),
        encode_jal(0, 0),
    ];

    let mut bytes = Vec::with_capacity(words.len() * 4);
    for word in words {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes
}

/// Load the built-in framebuffer writer program into memory.
pub fn load_demo_program(bus: &mut Bus, base_addr: Addr) -> Result<usize> {
    let bytes = demo_program_bytes();
    bus.write_bytes(base_addr, &bytes)
}

fn encode_lui(rd: u8, imm20: u32) -> u32 {
    (imm20 << 12) | ((rd as u32) << 7) | 0x37
}

fn encode_addi(rd: u8, rs1: u8, imm: i16) -> u32 {
    let imm12 = (imm as i32 as u32) & 0x0FFF;
    (imm12 << 20) | ((rs1 as u32) << 15) | ((rd as u32) << 7) | 0x13
}

fn encode_sh(rs2: u8, rs1: u8, imm: i16) -> u32 {
    let imm12 = (imm as i32 as u32) & 0x0FFF;
    let imm11_5 = (imm12 >> 5) & 0x7F;
    let imm4_0 = imm12 & 0x1F;

    (imm11_5 << 25) | ((rs2 as u32) << 20) | ((rs1 as u32) << 15) | (1 << 12) | (imm4_0 << 7) | 0x23
}

fn encode_bne(rs1: u8, rs2: u8, imm: i16) -> u32 {
    debug_assert!(imm % 2 == 0, "branch immediate must be 2-byte aligned");

    let imm13 = (imm as i32 as u32) & 0x1FFF;
    let imm12 = (imm13 >> 12) & 0x1;
    let imm10_5 = (imm13 >> 5) & 0x3F;
    let imm4_1 = (imm13 >> 1) & 0x0F;
    let imm11 = (imm13 >> 11) & 0x1;

    (imm12 << 31)
        | (imm10_5 << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | (1 << 12)
        | (imm4_1 << 8)
        | (imm11 << 7)
        | 0x63
}

fn encode_jal(rd: u8, imm: i32) -> u32 {
    debug_assert!(imm % 2 == 0, "jal immediate must be 2-byte aligned");

    let imm21 = (imm as u32) & 0x1F_FFFF;
    let imm20 = (imm21 >> 20) & 0x1;
    let imm10_1 = (imm21 >> 1) & 0x3FF;
    let imm11 = (imm21 >> 11) & 0x1;
    let imm19_12 = (imm21 >> 12) & 0xFF;

    (imm20 << 31) | (imm19_12 << 12) | (imm11 << 20) | (imm10_1 << 21) | ((rd as u32) << 7) | 0x6F
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::Cpu;
    use crate::memory::Ram;

    #[test]
    fn test_demo_program_size() {
        let bytes = demo_program_bytes();
        assert_eq!(bytes.len(), 8 * 4);
    }

    #[test]
    fn test_demo_program_writes_linux_fb_address() {
        let mut bus = Bus::new();
        bus.attach_memory(
            Addr::new(DEFAULT_PROGRAM_BASE),
            Ram::new(16 * 1024 * 1024),
            "RAM",
        );

        load_demo_program(&mut bus, Addr::new(DEFAULT_PROGRAM_BASE)).expect("load demo program");

        let mut cpu = Cpu::with_pc(bus, Addr::new(DEFAULT_PROGRAM_BASE));
        cpu.run(3_000).expect("run demo program");

        let mut buf = vec![0u8; 64];
        cpu.bus()
            .read_bytes(Addr::new(LINUX_FB_ADDR), &mut buf)
            .expect("read framebuffer bytes");

        assert!(buf.iter().any(|byte| *byte != 0));
    }
}
