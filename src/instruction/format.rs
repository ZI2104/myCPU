//! RISC-V instruction format types.
//!
//! This module defines data structures for each RISC-V instruction format.
//! All formats store decoded field values for use by the executor.

use crate::types::RegIdx;

/// R-type instruction format.
///
/// Used for register-to-register operations (ADD, SUB, AND, OR, etc.)
///
/// Format: `funct7[31:25] | rs2[24:20] | rs1[19:15] | funct3[14:12] | rd[11:7] | opcode[6:0]`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RType {
    /// Destination register
    pub rd: RegIdx,
    /// Source register 1
    pub rs1: RegIdx,
    /// Source register 2
    pub rs2: RegIdx,
    /// Function 3 field
    pub funct3: u8,
    /// Function 7 field
    pub funct7: u8,
}

/// I-type instruction format.
///
/// Used for immediate ALU operations, loads, and JALR.
///
/// Format: `imm[11:0][31:20] | rs1[19:15] | funct3[14:12] | rd[11:7] | opcode[6:0]`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IType {
    /// Destination register
    pub rd: RegIdx,
    /// Source register 1 (base address for loads)
    pub rs1: RegIdx,
    /// Sign-extended 12-bit immediate
    pub imm: i32,
    /// Function 3 field
    pub funct3: u8,
}

/// S-type instruction format.
///
/// Used for store operations.
///
/// Format: `imm[11:5][31:25] | rs2[24:20] | rs1[19:15] | funct3[14:12] | imm[4:0][11:7] | opcode[6:0]`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SType {
    /// Base address register
    pub rs1: RegIdx,
    /// Source register (value to store)
    pub rs2: RegIdx,
    /// Sign-extended immediate (offset)
    pub imm: i32,
    /// Function 3 field (store width)
    pub funct3: u8,
}

/// B-type instruction format.
///
/// Used for conditional branches.
///
/// Format: `imm[12|10:5][31:25] | rs2[24:20] | rs1[19:15] | funct3[14:12] | imm[4:1|11][11:7] | opcode[6:0]`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BType {
    /// Source register 1
    pub rs1: RegIdx,
    /// Source register 2
    pub rs2: RegIdx,
    /// Sign-extended branch offset (always a multiple of 2)
    pub imm: i32,
    /// Function 3 field (branch condition)
    pub funct3: u8,
}

/// U-type instruction format.
///
/// Used for LUI and AUIPC.
///
/// Format: `imm[31:12][31:12] | rd[11:7] | opcode[6:0]`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UType {
    /// Destination register
    pub rd: RegIdx,
    /// 20-bit immediate in bits [31:12]
    pub imm: u32,
}

/// J-type instruction format.
///
/// Used for JAL.
///
/// Format: `imm[20|10:1|11|19:12][31:12] | rd[11:7] | opcode[6:0]`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JType {
    /// Destination register (for return address)
    pub rd: RegIdx,
    /// Sign-extended jump offset (always a multiple of 2)
    pub imm: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_r_type_size() {
        // Ensure types are reasonably sized (no hidden bloat)
        assert!(std::mem::size_of::<RType>() <= 8);
        assert!(std::mem::size_of::<IType>() <= 16);
        assert!(std::mem::size_of::<SType>() <= 16);
        assert!(std::mem::size_of::<BType>() <= 16);
        assert!(std::mem::size_of::<UType>() <= 8);
        assert!(std::mem::size_of::<JType>() <= 8);
    }
}
