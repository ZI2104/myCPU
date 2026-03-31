//! RISC-V opcode and function field constants.
//!
//! This module defines all opcode, funct3, and funct7 constants for RV32I instructions.
//! All values are from the RISC-V specification.

/// RISC-V opcode field (bits [6:0])
pub mod opcode {
    /// LUI - Load Upper Immediate
    pub const LUI: u8 = 0b0110111;

    /// AUIPC - Add Upper Immediate to PC
    pub const AUIPC: u8 = 0b0010111;

    /// JAL - Jump and Link
    pub const JAL: u8 = 0b1101111;

    /// JALR - Jump and Link Register
    pub const JALR: u8 = 0b1100111;

    /// Branch instructions (BEQ, BNE, BLT, BGE, BLTU, BGEU)
    pub const BRANCH: u8 = 0b1100011;

    /// Load instructions (LB, LH, LW, LBU, LHU)
    pub const LOAD: u8 = 0b0000011;

    /// Store instructions (SB, SH, SW)
    pub const STORE: u8 = 0b0100011;

    /// Immediate arithmetic (ADDI, SLTI, SLTIU, ANDI, ORI, XORI, SLLI, SRLI, SRAI)
    pub const OP_IMM: u8 = 0b0010011;

    /// Register arithmetic (ADD, SUB, SLL, SLT, SLTU, XOR, SRL, SRA, AND, OR)
    pub const OP: u8 = 0b0110011;

    /// FENCE instructions
    pub const FENCE: u8 = 0b0001111;

    /// System instructions (ECALL, EBREAK)
    pub const SYSTEM: u8 = 0b1110011;
}

/// Function 3 field (bits [14:12])
pub mod funct3 {
    // === OP and OP_IMM shared ===
    /// ADD / ADDI / SUB
    pub const ADD_SUB: u8 = 0b000;

    /// SLL / SLLI
    pub const SLL: u8 = 0b001;

    /// SLT / SLTI
    pub const SLT: u8 = 0b010;

    /// SLTU / SLTIU
    pub const SLTU: u8 = 0b011;

    /// XOR / XORI
    pub const XOR: u8 = 0b100;

    /// SRL / SRLI / SRA / SRAI
    pub const SRL_SRA: u8 = 0b101;

    /// OR / ORI
    pub const OR: u8 = 0b110;

    /// AND / ANDI
    pub const AND: u8 = 0b111;

    // === Branch conditions ===
    /// BEQ - Branch if Equal
    pub const BEQ: u8 = 0b000;

    /// BNE - Branch if Not Equal
    pub const BNE: u8 = 0b001;

    /// BLT - Branch if Less Than (signed)
    pub const BLT: u8 = 0b100;

    /// BGE - Branch if Greater or Equal (signed)
    pub const BGE: u8 = 0b101;

    /// BLTU - Branch if Less Than Unsigned
    pub const BLTU: u8 = 0b110;

    /// BGEU - Branch if Greater or Equal Unsigned
    pub const BGEU: u8 = 0b111;

    // === Load variants ===
    /// LB - Load Byte (sign-extended)
    pub const LB: u8 = 0b000;

    /// LH - Load Halfword (sign-extended)
    pub const LH: u8 = 0b001;

    /// LW - Load Word
    pub const LW: u8 = 0b010;

    /// LBU - Load Byte Unsigned
    pub const LBU: u8 = 0b100;

    /// LHU - Load Halfword Unsigned
    pub const LHU: u8 = 0b101;

    // === Store variants ===
    /// SB - Store Byte
    pub const SB: u8 = 0b000;

    /// SH - Store Halfword
    pub const SH: u8 = 0b001;

    /// SW - Store Word
    pub const SW: u8 = 0b010;
}

/// Function 7 field (bits [31:25])
pub mod funct7 {
    /// ADD, SLL, SRL (and others with funct7 = 0)
    pub const BASE: u8 = 0b0000000;

    /// SUB, SRA
    pub const ALT: u8 = 0b0100000;

    /// RV32M extension (MUL/DIV/REM family)
    pub const M_EXT: u8 = 0b0000001;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opcode_values() {
        // Verify opcodes match RISC-V spec
        assert_eq!(opcode::LUI, 0x37);
        assert_eq!(opcode::AUIPC, 0x17);
        assert_eq!(opcode::JAL, 0x6F);
        assert_eq!(opcode::JALR, 0x67);
        assert_eq!(opcode::BRANCH, 0x63);
        assert_eq!(opcode::LOAD, 0x03);
        assert_eq!(opcode::STORE, 0x23);
        assert_eq!(opcode::OP_IMM, 0x13);
        assert_eq!(opcode::OP, 0x33);
        assert_eq!(opcode::SYSTEM, 0x73);
    }

    #[test]
    fn test_funct3_values() {
        assert_eq!(funct3::ADD_SUB, 0x0);
        assert_eq!(funct3::SLL, 0x1);
        assert_eq!(funct3::SLT, 0x2);
        assert_eq!(funct3::SLTU, 0x3);
        assert_eq!(funct3::XOR, 0x4);
        assert_eq!(funct3::SRL_SRA, 0x5);
        assert_eq!(funct3::OR, 0x6);
        assert_eq!(funct3::AND, 0x7);
    }
}
