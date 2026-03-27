//! RISC-V instruction decoder.
//!
//! This module provides instruction decoding from raw 32-bit words to typed instruction formats.

use super::format::{BType, IType, JType, RType, SType, UType};
use super::opcode::opcode;
use crate::error::{Result, SimError};
use crate::types::{Addr, RegIdx};

/// Decoded instruction representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodedInstr {
    /// R-type: register-to-register operations
    R(RType),
    /// I-type: immediate ALU operations (OP_IMM)
    I(IType),
    /// Load: load operations (LB, LH, LW, LBU, LHU)
    Load(IType),
    /// Jalr: jump and link register
    Jalr(IType),
    /// S-type: store operations
    S(SType),
    /// B-type: conditional branches
    B(BType),
    /// U-type: LUI, AUIPC
    U(UType),
    /// J-type: JAL
    J(JType),
    /// System: ECALL, EBREAK, FENCE
    System {
        /// Function 3 field
        funct3: u8,
        /// Immediate field (used for fence variants)
        imm: u32,
    },
}

/// Instruction decoder.
pub struct Decoder;

impl Decoder {
    /// Extract opcode (bits [6:0])
    #[inline]
    pub fn opcode(instr: u32) -> u8 {
        (instr & 0x7F) as u8
    }

    /// Extract rd (bits [11:7])
    #[inline]
    pub fn rd(instr: u32) -> RegIdx {
        RegIdx::new(((instr >> 7) & 0x1F) as u8)
    }

    /// Extract funct3 (bits [14:12])
    #[inline]
    pub fn funct3(instr: u32) -> u8 {
        ((instr >> 12) & 0x7) as u8
    }

    /// Extract rs1 (bits [19:15])
    #[inline]
    pub fn rs1(instr: u32) -> RegIdx {
        RegIdx::new(((instr >> 15) & 0x1F) as u8)
    }

    /// Extract rs2 (bits [24:20])
    #[inline]
    pub fn rs2(instr: u32) -> RegIdx {
        RegIdx::new(((instr >> 20) & 0x1F) as u8)
    }

    /// Extract funct7 (bits [31:25])
    #[inline]
    pub fn funct7(instr: u32) -> u8 {
        ((instr >> 25) & 0x7F) as u8
    }

    /// Decode I-type immediate (bits [31:20], sign-extended)
    #[inline]
    pub fn imm_i(instr: u32) -> i32 {
        let imm = (instr >> 20) & 0xFFF;
        sign_extend_12(imm)
    }

    /// Decode S-type immediate (bits [31:25] and [11:7], sign-extended)
    #[inline]
    pub fn imm_s(instr: u32) -> i32 {
        let imm_11_5 = (instr >> 25) & 0x7F;
        let imm_4_0 = (instr >> 7) & 0x1F;
        let imm = (imm_11_5 << 5) | imm_4_0;
        sign_extend_12(imm)
    }

    /// Decode B-type immediate (sign-extended, multiple of 2)
    #[inline]
    pub fn imm_b(instr: u32) -> i32 {
        let imm_12 = ((instr >> 31) & 0x1) << 12;
        let imm_11 = ((instr >> 7) & 0x1) << 11;
        let imm_10_5 = ((instr >> 25) & 0x3F) << 5;
        let imm_4_1 = ((instr >> 8) & 0xF) << 1;
        let imm = imm_12 | imm_11 | imm_10_5 | imm_4_1;
        sign_extend_13(imm)
    }

    /// Decode U-type immediate (bits [31:12])
    #[inline]
    pub fn imm_u(instr: u32) -> u32 {
        instr & 0xFFFFF000
    }

    /// Decode J-type immediate (sign-extended, multiple of 2)
    #[inline]
    pub fn imm_j(instr: u32) -> i32 {
        let imm_20 = ((instr >> 31) & 0x1) << 20;
        let imm_19_12 = ((instr >> 12) & 0xFF) << 12;
        let imm_11 = ((instr >> 20) & 0x1) << 11;
        let imm_10_1 = ((instr >> 21) & 0x3FF) << 1;
        let imm = imm_20 | imm_19_12 | imm_11 | imm_10_1;
        sign_extend_21(imm)
    }

    /// Decode a raw instruction word.
    ///
    /// # Arguments
    /// * `instr` - The raw 32-bit instruction word
    /// * `pc` - The current PC (for error reporting)
    ///
    /// # Returns
    /// A decoded instruction, or an error for unknown opcodes.
    pub fn decode(instr: u32, pc: Addr) -> Result<DecodedInstr> {
        let opcode = Self::opcode(instr);
        let rd = Self::rd(instr);
        let rs1 = Self::rs1(instr);
        let rs2 = Self::rs2(instr);
        let funct3 = Self::funct3(instr);
        let funct7 = Self::funct7(instr);

        match opcode {
            opcode::OP => Ok(DecodedInstr::R(RType {
                rd,
                rs1,
                rs2,
                funct3,
                funct7,
            })),

            opcode::OP_IMM => Ok(DecodedInstr::I(IType {
                rd,
                rs1,
                imm: Self::imm_i(instr),
                funct3,
            })),

            opcode::LOAD => Ok(DecodedInstr::Load(IType {
                rd,
                rs1,
                imm: Self::imm_i(instr),
                funct3,
            })),

            opcode::JALR => Ok(DecodedInstr::Jalr(IType {
                rd,
                rs1,
                imm: Self::imm_i(instr),
                funct3,
            })),

            opcode::STORE => Ok(DecodedInstr::S(SType {
                rs1,
                rs2,
                imm: Self::imm_s(instr),
                funct3,
            })),

            opcode::BRANCH => Ok(DecodedInstr::B(BType {
                rs1,
                rs2,
                imm: Self::imm_b(instr),
                funct3,
            })),

            opcode::LUI | opcode::AUIPC => Ok(DecodedInstr::U(UType { rd, imm: Self::imm_u(instr) })),

            opcode::JAL => Ok(DecodedInstr::J(JType {
                rd,
                imm: Self::imm_j(instr),
            })),

            opcode::SYSTEM | opcode::FENCE => Ok(DecodedInstr::System {
                funct3,
                imm: instr >> 20,
            }),

            _ => Err(SimError::UnsupportedInstruction {
                pc,
                message: format!("Unknown opcode: 0x{:02x}", opcode),
            }),
        }
    }
}

/// Decode a raw instruction word (convenience function).
#[inline]
pub fn decode(instr: u32, pc: Addr) -> Result<DecodedInstr> {
    Decoder::decode(instr, pc)
}

/// Sign-extend a 12-bit value to i32.
#[inline]
fn sign_extend_12(value: u32) -> i32 {
    // If bit 11 is set, fill bits [31:12] with 1s
    if value & 0x800 != 0 {
        ((value) | 0xFFFFF000) as i32
    } else {
        value as i32
    }
}

/// Sign-extend a 13-bit value to i32.
#[inline]
fn sign_extend_13(value: u32) -> i32 {
    if value & 0x1000 != 0 {
        ((value) | 0xFFFFE000) as i32
    } else {
        value as i32
    }
}

/// Sign-extend a 21-bit value to i32.
#[inline]
fn sign_extend_21(value: u32) -> i32 {
    if value & 0x100000 != 0 {
        ((value) | 0xFFF00000) as i32
    } else {
        value as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_r_type() {
        // ADD x5, x1, x2 (funct7=0, rs2=2, rs1=1, funct3=0, rd=5, opcode=0x33)
        let instr = 0x002082B3u32;
        let decoded = Decoder::decode(instr, Addr::new(0)).unwrap();

        match decoded {
            DecodedInstr::R(r) => {
                assert_eq!(r.rd.raw(), 5);
                assert_eq!(r.rs1.raw(), 1);
                assert_eq!(r.rs2.raw(), 2);
                assert_eq!(r.funct3, 0);
                assert_eq!(r.funct7, 0);
            }
            _ => panic!("Expected R-type"),
        }
    }

    #[test]
    fn test_decode_i_type() {
        // ADDI x5, x1, 100
        // imm[11:0]=100, rs1=1, funct3=0, rd=5, opcode=0x13
        let instr = 0x06408293u32;
        let decoded = Decoder::decode(instr, Addr::new(0)).unwrap();

        match decoded {
            DecodedInstr::I(i) => {
                assert_eq!(i.rd.raw(), 5);
                assert_eq!(i.rs1.raw(), 1);
                assert_eq!(i.imm, 100);
                assert_eq!(i.funct3, 0);
            }
            _ => panic!("Expected I-type"),
        }
    }

    #[test]
    fn test_decode_i_type_negative() {
        // ADDI x5, x1, -1
        // imm[11:0]=0xFFF, rs1=1, funct3=0, rd=5, opcode=0x13
        let instr = 0xFFF08293u32;
        let decoded = Decoder::decode(instr, Addr::new(0)).unwrap();

        match decoded {
            DecodedInstr::I(i) => {
                assert_eq!(i.imm, -1);
            }
            _ => panic!("Expected I-type"),
        }
    }

    #[test]
    fn test_decode_s_type() {
        // SW x2, 4(x1)
        // imm[11:5]=0, rs2=2, rs1=1, funct3=2, imm[4:0]=4, opcode=0x23
        let instr = 0x0020A223u32;
        let decoded = Decoder::decode(instr, Addr::new(0)).unwrap();

        match decoded {
            DecodedInstr::S(s) => {
                assert_eq!(s.rs1.raw(), 1);
                assert_eq!(s.rs2.raw(), 2);
                assert_eq!(s.imm, 4);
                assert_eq!(s.funct3, 2);
            }
            _ => panic!("Expected S-type"),
        }
    }

    #[test]
    fn test_decode_b_type() {
        // BEQ x1, x2, 8
        // imm[12]=0, imm[10:5]=0, rs2=2, rs1=1, funct3=0, imm[4:1]=4, imm[11]=0, opcode=0x63
        let instr = 0x00208463u32;
        let decoded = Decoder::decode(instr, Addr::new(0)).unwrap();

        match decoded {
            DecodedInstr::B(b) => {
                assert_eq!(b.rs1.raw(), 1);
                assert_eq!(b.rs2.raw(), 2);
                assert_eq!(b.imm, 8);
                assert_eq!(b.funct3, 0);
            }
            _ => panic!("Expected B-type"),
        }
    }

    #[test]
    fn test_decode_u_type() {
        // LUI x5, 0x12345
        // imm[31:12]=0x12345, rd=5, opcode=0x37
        let instr = 0x123452B7u32;
        let decoded = Decoder::decode(instr, Addr::new(0)).unwrap();

        match decoded {
            DecodedInstr::U(u) => {
                assert_eq!(u.rd.raw(), 5);
                assert_eq!(u.imm, 0x12345000);
            }
            _ => panic!("Expected U-type"),
        }
    }

    #[test]
    fn test_decode_j_type() {
        // JAL x1, 100
        // imm[20]=0, imm[10:1]=50, imm[11]=0, imm[19:12]=0, rd=1, opcode=0x6F
        let instr = 0x064000EFu32;
        let decoded = Decoder::decode(instr, Addr::new(0)).unwrap();

        match decoded {
            DecodedInstr::J(j) => {
                assert_eq!(j.rd.raw(), 1);
                assert_eq!(j.imm, 100);
            }
            _ => panic!("Expected J-type"),
        }
    }

    #[test]
    fn test_decode_unknown_opcode() {
        let instr = 0x00000000u32; // Invalid (opcode 0)
        let result = Decoder::decode(instr, Addr::new(0x1000));
        assert!(result.is_err());
    }
}
