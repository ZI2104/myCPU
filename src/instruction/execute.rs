//! RISC-V instruction execution.
//!
//! This module provides instruction execution logic for all RV32I instructions.

use super::decoder::DecodedInstr;
use super::format::{BType, IType, JType, RType, SType, UType};
use super::opcode::{funct3, funct7, opcode};
use crate::cpu::csr::{CsrOp, ExceptionCause};
use crate::cpu::Cpu;
use crate::error::{Result, SimError};
use crate::peripheral::{
    LPU_BASE, LPU_OPCODE_BYTE_TOKENIZE, LPU_OPCODE_EMBEDDING_BAG, LPU_OPCODE_GREEDY_DECODE,
    LPU_OPCODE_TOPK_SAMPLE_DECODE, LPU_OPCODE_TOPP_SAMPLE_DECODE, NPU_BASE,
};
use crate::types::{Addr, Byte, Half, PrivilegeLevel, RegIdx, Word};

impl Cpu {
    /// Execute CUSTOM-0 instructions for coprocessor fast-path.
    ///
    /// Encoding (R-type layout):
    /// - opcode = CUSTOM_0 (0x0B)
    /// - funct3 = 0 => NPU, 1 => LPU
    /// - funct7[4:0] => coprocessor opcode
    /// - rs1/rs2 are source operands
    /// - rd is destination register
    pub fn execute_custom0(&mut self, instruction: u32) -> Result<()> {
        const REG_CONTROL: u32 = 0x00;
        const REG_OP_A: u32 = 0x08;
        const REG_OP_B: u32 = 0x0C;
        const REG_RESULT: u32 = 0x10;
        const REG_OPCODE: u32 = 0x14;
        const CONTROL_START: u32 = 1 << 0;

        let rd = RegIdx::new(((instruction >> 7) & 0x1F) as u8);
        let funct3 = ((instruction >> 12) & 0x7) as u8;
        let rs1 = RegIdx::new(((instruction >> 15) & 0x1F) as u8);
        let rs2 = RegIdx::new(((instruction >> 20) & 0x1F) as u8);
        let funct7 = ((instruction >> 25) & 0x7F) as u8;
        let cop_op = (funct7 & 0x1F) as u32;

        let op_a = self.registers().read(rs1).raw();
        let op_b = self.registers().read(rs2).raw();

        let base = match funct3 {
            0 => {
                if cop_op > 3 {
                    return Err(SimError::UnsupportedInstruction {
                        pc: self.pc(),
                        message: format!("Invalid NPU custom opcode: {}", cop_op),
                    });
                }
                NPU_BASE
            }
            1 => {
                let is_language = cop_op == LPU_OPCODE_BYTE_TOKENIZE
                    || cop_op == LPU_OPCODE_EMBEDDING_BAG
                    || cop_op == LPU_OPCODE_GREEDY_DECODE
                    || cop_op == LPU_OPCODE_TOPK_SAMPLE_DECODE
                    || cop_op == LPU_OPCODE_TOPP_SAMPLE_DECODE;
                if !is_language {
                    return Err(SimError::UnsupportedInstruction {
                        pc: self.pc(),
                        message: format!("Invalid LPU custom opcode: {}", cop_op),
                    });
                }
                LPU_BASE
            }
            _ => {
                return Err(SimError::UnsupportedInstruction {
                    pc: self.pc(),
                    message: format!("Unsupported CUSTOM-0 funct3={:03b}", funct3),
                });
            }
        };

        self.write_word(Addr::new(base + REG_OP_A), Word::new(op_a))?;
        self.write_word(Addr::new(base + REG_OP_B), Word::new(op_b))?;
        self.write_word(Addr::new(base + REG_OPCODE), Word::new(cop_op))?;
        self.write_word(Addr::new(base + REG_CONTROL), Word::new(CONTROL_START))?;

        let result = self.read_word(Addr::new(base + REG_RESULT))?;
        self.registers_mut().write(rd, result);
        self.increment_pc();
        Ok(())
    }

    /// Execute a decoded instruction.
    ///
    /// This method dispatches to the appropriate execution handler based on
    /// the instruction format.
    pub fn execute_decoded(&mut self, decoded: DecodedInstr) -> Result<()> {
        match decoded {
            DecodedInstr::R(r) => self.execute_r_type(r),
            DecodedInstr::I(i) => self.execute_i_type(i),
            DecodedInstr::Load(i) => {
                let rs1_val = self.registers().read(i.rs1);
                self.execute_load(i, rs1_val)
            }
            DecodedInstr::Jalr(i) => self.execute_jalr(i),
            DecodedInstr::S(s) => self.execute_s_type(s),
            DecodedInstr::B(b) => self.execute_b_type(b),
            DecodedInstr::U(u) => self.execute_u_type(u),
            DecodedInstr::J(j) => self.execute_j_type(j),
            DecodedInstr::System {
                opcode,
                rd,
                rs1,
                funct3,
                imm,
            } => self.execute_system(opcode, rd, rs1, funct3, imm),
        }
    }

    /// Execute R-type instructions (register-to-register operations).
    ///
    /// Instructions: ADD, SUB, AND, OR, XOR, SLL, SRL, SRA, SLT, SLTU
    fn execute_r_type(&mut self, instr: RType) -> Result<()> {
        let rs1_val = self.registers().read(instr.rs1);
        let rs2_val = self.registers().read(instr.rs2);

        let result = match (instr.funct3, instr.funct7) {
            // ADD: rd = rs1 + rs2
            (funct3::ADD_SUB, funct7::BASE) => Word::new(rs1_val.raw().wrapping_add(rs2_val.raw())),

            // SUB: rd = rs1 - rs2
            (funct3::ADD_SUB, funct7::ALT) => Word::new(rs1_val.raw().wrapping_sub(rs2_val.raw())),

            // MUL: lower 32 bits of signed * signed
            (funct3::ADD_SUB, funct7::M_EXT) => {
                Word::new(rs1_val.raw().wrapping_mul(rs2_val.raw()))
            }

            // AND: rd = rs1 & rs2
            (funct3::AND, funct7::BASE) => Word::new(rs1_val.raw() & rs2_val.raw()),

            // REMU: unsigned remainder
            (funct3::AND, funct7::M_EXT) => {
                let dividend = rs1_val.raw();
                let divisor = rs2_val.raw();
                if divisor == 0 {
                    Word::new(dividend)
                } else {
                    Word::new(dividend % divisor)
                }
            }

            // OR: rd = rs1 | rs2
            (funct3::OR, funct7::BASE) => Word::new(rs1_val.raw() | rs2_val.raw()),

            // REM: signed remainder
            (funct3::OR, funct7::M_EXT) => {
                let dividend = rs1_val.as_signed();
                let divisor = rs2_val.as_signed();
                if divisor == 0 {
                    Word::new(dividend as u32)
                } else if dividend == i32::MIN && divisor == -1 {
                    Word::new(0)
                } else {
                    Word::new((dividend % divisor) as u32)
                }
            }

            // XOR: rd = rs1 ^ rs2
            (funct3::XOR, funct7::BASE) => Word::new(rs1_val.raw() ^ rs2_val.raw()),

            // DIV: signed division
            (funct3::XOR, funct7::M_EXT) => {
                let dividend = rs1_val.as_signed();
                let divisor = rs2_val.as_signed();
                if divisor == 0 {
                    Word::new(u32::MAX)
                } else if dividend == i32::MIN && divisor == -1 {
                    Word::new(i32::MIN as u32)
                } else {
                    Word::new((dividend / divisor) as u32)
                }
            }

            // SLL: rd = rs1 << (rs2 & 0x1F)
            (funct3::SLL, funct7::BASE) => Word::new(rs1_val.raw() << (rs2_val.raw() & 0x1F)),

            // MULH: upper 32 bits of signed * signed
            (funct3::SLL, funct7::M_EXT) => {
                let a = rs1_val.as_signed() as i128;
                let b = rs2_val.as_signed() as i128;
                Word::new(((a * b) >> 32) as u32)
            }

            // SRL: rd = rs1 >> (rs2 & 0x1F) (logical)
            (funct3::SRL_SRA, funct7::BASE) => Word::new(rs1_val.raw() >> (rs2_val.raw() & 0x1F)),

            // SRA: rd = rs1 >>> (rs2 & 0x1F) (arithmetic)
            (funct3::SRL_SRA, funct7::ALT) => {
                let shift = rs2_val.raw() & 0x1F;
                Word::new((rs1_val.as_signed() >> shift) as u32)
            }

            // DIVU: unsigned division
            (funct3::SRL_SRA, funct7::M_EXT) => {
                let dividend = rs1_val.raw();
                let divisor = rs2_val.raw();
                if divisor == 0 {
                    Word::new(u32::MAX)
                } else {
                    Word::new(dividend / divisor)
                }
            }

            // SLT: rd = (rs1 < rs2) ? 1 : 0 (signed)
            (funct3::SLT, funct7::BASE) => {
                Word::new(if rs1_val.as_signed() < rs2_val.as_signed() {
                    1
                } else {
                    0
                })
            }

            // MULHSU: upper 32 bits of signed * unsigned
            (funct3::SLT, funct7::M_EXT) => {
                let a = rs1_val.as_signed() as i128;
                let b = rs2_val.raw() as i128;
                Word::new(((a * b) >> 32) as u32)
            }

            // SLTU: rd = (rs1 < rs2) ? 1 : 0 (unsigned)
            (funct3::SLTU, funct7::BASE) => {
                Word::new(if rs1_val.raw() < rs2_val.raw() { 1 } else { 0 })
            }

            // MULHU: upper 32 bits of unsigned * unsigned
            (funct3::SLTU, funct7::M_EXT) => {
                let a = rs1_val.raw() as u128;
                let b = rs2_val.raw() as u128;
                Word::new(((a * b) >> 32) as u32)
            }

            _ => {
                return Err(SimError::UnsupportedInstruction {
                    pc: self.pc(),
                    message: format!(
                        "Invalid R-type: funct3={:03b} funct7={:07b}",
                        instr.funct3, instr.funct7
                    ),
                });
            }
        };

        self.registers_mut().write(instr.rd, result);
        self.increment_pc();
        Ok(())
    }

    /// Execute I-type instructions (immediate ALU operations).
    ///
    /// Instructions: ADDI, ANDI, ORI, XORI, SLTI, SLTIU, SLLI, SRLI, SRAI
    /// Note: LOAD instructions are handled separately via DecodedInstr::Load
    fn execute_i_type(&mut self, instr: IType) -> Result<()> {
        let rs1_val = self.registers().read(instr.rs1);

        // Determine the operation based on funct3
        match instr.funct3 {
            // ADDI: rd = rs1 + imm
            funct3::ADD_SUB => {
                let result = Word::new(rs1_val.raw().wrapping_add(instr.imm as u32));
                self.registers_mut().write(instr.rd, result);
                self.increment_pc();
            }

            // SLTI: rd = (rs1 < imm) ? 1 : 0 (signed)
            funct3::SLT => {
                let result = Word::new(if rs1_val.as_signed() < instr.imm {
                    1
                } else {
                    0
                });
                self.registers_mut().write(instr.rd, result);
                self.increment_pc();
            }

            // SLTIU: rd = (rs1 < imm) ? 1 : 0 (unsigned, but imm is still sign-extended)
            funct3::SLTU => {
                let result = Word::new(if rs1_val.raw() < (instr.imm as u32) {
                    1
                } else {
                    0
                });
                self.registers_mut().write(instr.rd, result);
                self.increment_pc();
            }

            // ANDI: rd = rs1 & imm
            funct3::AND => {
                let result = Word::new(rs1_val.raw() & (instr.imm as u32));
                self.registers_mut().write(instr.rd, result);
                self.increment_pc();
            }

            // ORI: rd = rs1 | imm
            funct3::OR => {
                let result = Word::new(rs1_val.raw() | (instr.imm as u32));
                self.registers_mut().write(instr.rd, result);
                self.increment_pc();
            }

            // XORI: rd = rs1 ^ imm
            funct3::XOR => {
                let result = Word::new(rs1_val.raw() ^ (instr.imm as u32));
                self.registers_mut().write(instr.rd, result);
                self.increment_pc();
            }

            // SLLI: rd = rs1 << shamt
            funct3::SLL => {
                let shamt = (instr.imm as u32) & 0x1F;
                let result = Word::new(rs1_val.raw() << shamt);
                self.registers_mut().write(instr.rd, result);
                self.increment_pc();
            }

            // SRLI / SRAI: rd = rs1 >> shamt
            funct3::SRL_SRA => {
                let shamt = (instr.imm as u32) & 0x1F;
                let is_arithmetic = (instr.imm & 0x400) != 0; // bit 10

                let result = if is_arithmetic {
                    // SRAI
                    Word::new((rs1_val.as_signed() >> shamt) as u32)
                } else {
                    // SRLI
                    Word::new(rs1_val.raw() >> shamt)
                };
                self.registers_mut().write(instr.rd, result);
                self.increment_pc();
            }

            _ => {
                return Err(SimError::UnsupportedInstruction {
                    pc: self.pc(),
                    message: format!("Unknown I-type funct3: {:03b}", instr.funct3),
                });
            }
        }

        Ok(())
    }

    /// Execute load instructions (LB, LH, LW, LBU, LHU).
    fn execute_load(&mut self, instr: IType, rs1_val: Word) -> Result<()> {
        let addr = Addr::new(rs1_val.raw().wrapping_add(instr.imm as u32));

        let result = match instr.funct3 {
            // LB: Load byte, sign-extend
            funct3::LB => {
                let byte = self.read_byte(addr)?;
                Word::from_byte(byte.raw())
            }

            // LH: Load halfword, sign-extend
            funct3::LH => {
                let half = self.read_half(addr)?;
                Word::from_half(half.raw())
            }

            // LW: Load word
            funct3::LW => self.read_word(addr)?,

            // LBU: Load byte, zero-extend
            funct3::LBU => {
                let byte = self.read_byte(addr)?;
                Word::from_byte_zero(byte.raw())
            }

            // LHU: Load halfword, zero-extend
            funct3::LHU => {
                let half = self.read_half(addr)?;
                Word::from_half_zero(half.raw())
            }

            _ => unreachable!(), // Already filtered in execute_i_type
        };

        self.registers_mut().write(instr.rd, result);
        self.increment_pc();
        Ok(())
    }

    /// Execute S-type instructions (store operations).
    ///
    /// Instructions: SB, SH, SW
    fn execute_s_type(&mut self, instr: SType) -> Result<()> {
        let rs1_val = self.registers().read(instr.rs1);
        let rs2_val = self.registers().read(instr.rs2);
        let addr = Addr::new(rs1_val.raw().wrapping_add(instr.imm as u32));

        match instr.funct3 {
            // SB: Store byte
            funct3::SB => {
                self.write_byte(addr, Byte::new(rs2_val.raw() as u8))?;
            }

            // SH: Store halfword
            funct3::SH => {
                self.write_half(addr, Half::new(rs2_val.raw() as u16))?;
            }

            // SW: Store word
            funct3::SW => {
                self.write_word(addr, rs2_val)?;
            }

            _ => {
                return Err(SimError::UnsupportedInstruction {
                    pc: self.pc(),
                    message: format!("Unknown S-type funct3: {:03b}", instr.funct3),
                });
            }
        }

        self.increment_pc();
        Ok(())
    }

    /// Execute B-type instructions (conditional branches).
    ///
    /// Instructions: BEQ, BNE, BLT, BGE, BLTU, BGEU
    fn execute_b_type(&mut self, instr: BType) -> Result<()> {
        let rs1_val = self.registers().read(instr.rs1);
        let rs2_val = self.registers().read(instr.rs2);

        let take_branch = match instr.funct3 {
            // BEQ: Branch if equal
            funct3::BEQ => rs1_val.raw() == rs2_val.raw(),

            // BNE: Branch if not equal
            funct3::BNE => rs1_val.raw() != rs2_val.raw(),

            // BLT: Branch if less than (signed)
            funct3::BLT => rs1_val.as_signed() < rs2_val.as_signed(),

            // BGE: Branch if greater or equal (signed)
            funct3::BGE => rs1_val.as_signed() >= rs2_val.as_signed(),

            // BLTU: Branch if less than (unsigned)
            funct3::BLTU => rs1_val.raw() < rs2_val.raw(),

            // BGEU: Branch if greater or equal (unsigned)
            funct3::BGEU => rs1_val.raw() >= rs2_val.raw(),

            _ => {
                return Err(SimError::UnsupportedInstruction {
                    pc: self.pc(),
                    message: format!("Unknown B-type funct3: {:03b}", instr.funct3),
                });
            }
        };

        if take_branch {
            // Branch target: PC + imm (imm is already in bytes)
            let target = Addr::new(self.pc().raw().wrapping_add(instr.imm as u32));
            self.set_pc(target);
        } else {
            self.increment_pc();
        }

        Ok(())
    }

    /// Execute U-type instructions (upper immediate).
    ///
    /// Instructions: LUI, AUIPC (handled by caller with opcode context)
    fn execute_u_type(&mut self, instr: UType) -> Result<()> {
        // This is called only for LUI by default
        // AUIPC is handled separately in execute() with opcode context
        self.registers_mut().write(instr.rd, Word::new(instr.imm));
        self.increment_pc();
        Ok(())
    }

    /// Execute LUI instruction.
    pub fn execute_lui(&mut self, rd: RegIdx, imm: u32) -> Result<()> {
        self.registers_mut().write(rd, Word::new(imm));
        self.increment_pc();
        Ok(())
    }

    /// Execute AUIPC instruction.
    pub fn execute_auipc(&mut self, rd: RegIdx, imm: u32) -> Result<()> {
        let result = Word::new(self.pc().raw().wrapping_add(imm));
        self.registers_mut().write(rd, result);
        self.increment_pc();
        Ok(())
    }

    /// Execute J-type instructions (JAL).
    fn execute_j_type(&mut self, instr: JType) -> Result<()> {
        // Save return address: rd = PC + 4
        let return_addr = Word::new(self.pc().raw().wrapping_add(4));
        self.registers_mut().write(instr.rd, return_addr);

        // Jump to target: PC = PC + imm
        let target = Addr::new(self.pc().raw().wrapping_add(instr.imm as u32));
        self.set_pc(target);

        Ok(())
    }

    /// Execute JALR instruction.
    pub fn execute_jalr(&mut self, instr: IType) -> Result<()> {
        let rs1_val = self.registers().read(instr.rs1);

        // Save return address: rd = PC + 4
        let return_addr = Word::new(self.pc().raw().wrapping_add(4));
        self.registers_mut().write(instr.rd, return_addr);

        // Jump to target: PC = (rs1 + imm) & ~1
        let target = rs1_val.raw().wrapping_add(instr.imm as u32) & !1;
        self.set_pc(Addr::new(target));

        Ok(())
    }

    /// Execute a minimal subset of RV32A atomic instructions.
    ///
    /// Currently supported:
    /// - `lr.w` / `sc.w` (including aq/rl variants)
    /// - `amoadd.w` (including aq/rl variants)
    /// - `amoswap.w` (including aq/rl variants)
    pub fn execute_amo(&mut self, instruction: u32) -> Result<()> {
        let funct3 = ((instruction >> 12) & 0x7) as u8;
        let rd = RegIdx::new(((instruction >> 7) & 0x1F) as u8);
        let rs1 = RegIdx::new(((instruction >> 15) & 0x1F) as u8);
        let rs2 = RegIdx::new(((instruction >> 20) & 0x1F) as u8);
        let funct5 = ((instruction >> 27) & 0x1F) as u8;

        // RV32A word operations use funct3=010.
        if funct3 != 0b010 {
            return Err(SimError::UnsupportedInstruction {
                pc: self.pc(),
                message: format!("Unsupported AMO width funct3={:03b}", funct3),
            });
        }

        match funct5 {
            // LR.W
            0b00010 => {
                let addr = Addr::new(self.registers().read(rs1).raw());
                let old = self.read_word(addr)?;
                self.set_atomic_reservation(addr);
                self.registers_mut().write(rd, old);
                self.increment_pc();
                Ok(())
            }
            // SC.W
            0b00011 => {
                let addr = Addr::new(self.registers().read(rs1).raw());
                let src = self.registers().read(rs2);
                let success = self.has_atomic_reservation(addr);

                if success {
                    self.write_word(addr, src)?;
                    self.registers_mut().write(rd, Word::new(0));
                } else {
                    self.registers_mut().write(rd, Word::new(1));
                }

                self.clear_atomic_reservation();
                self.increment_pc();
                Ok(())
            }
            // AMOADD.W
            0b00000 => {
                let addr = Addr::new(self.registers().read(rs1).raw());
                let src = self.registers().read(rs2).raw();
                let old = self.read_word(addr)?;
                let new_value = Word::new(old.raw().wrapping_add(src));
                self.write_word(addr, new_value)?;
                self.registers_mut().write(rd, old);
                self.increment_pc();
                Ok(())
            }
            // AMOSWAP.W
            0b00001 => {
                let addr = Addr::new(self.registers().read(rs1).raw());
                let src = self.registers().read(rs2);
                let old = self.read_word(addr)?;
                self.write_word(addr, src)?;
                self.registers_mut().write(rd, old);
                self.increment_pc();
                Ok(())
            }
            // AMOXOR.W
            0b00100 => {
                let addr = Addr::new(self.registers().read(rs1).raw());
                let src = self.registers().read(rs2).raw();
                let old = self.read_word(addr)?;
                let new_value = Word::new(old.raw() ^ src);
                self.write_word(addr, new_value)?;
                self.registers_mut().write(rd, old);
                self.increment_pc();
                Ok(())
            }
            // AMOAND.W
            0b01100 => {
                let addr = Addr::new(self.registers().read(rs1).raw());
                let src = self.registers().read(rs2).raw();
                let old = self.read_word(addr)?;
                let new_value = Word::new(old.raw() & src);
                self.write_word(addr, new_value)?;
                self.registers_mut().write(rd, old);
                self.increment_pc();
                Ok(())
            }
            // AMOOR.W
            0b01000 => {
                let addr = Addr::new(self.registers().read(rs1).raw());
                let src = self.registers().read(rs2).raw();
                let old = self.read_word(addr)?;
                let new_value = Word::new(old.raw() | src);
                self.write_word(addr, new_value)?;
                self.registers_mut().write(rd, old);
                self.increment_pc();
                Ok(())
            }
            // AMOMIN.W (signed)
            0b10000 => {
                let addr = Addr::new(self.registers().read(rs1).raw());
                let src = self.registers().read(rs2).raw();
                let old = self.read_word(addr)?;
                let new_raw = if old.as_signed() < (src as i32) {
                    old.raw()
                } else {
                    src
                };
                self.write_word(addr, Word::new(new_raw))?;
                self.registers_mut().write(rd, old);
                self.increment_pc();
                Ok(())
            }
            // AMOMAX.W (signed)
            0b10100 => {
                let addr = Addr::new(self.registers().read(rs1).raw());
                let src = self.registers().read(rs2).raw();
                let old = self.read_word(addr)?;
                let new_raw = if old.as_signed() > (src as i32) {
                    old.raw()
                } else {
                    src
                };
                self.write_word(addr, Word::new(new_raw))?;
                self.registers_mut().write(rd, old);
                self.increment_pc();
                Ok(())
            }
            // AMOMINU.W (unsigned)
            0b11000 => {
                let addr = Addr::new(self.registers().read(rs1).raw());
                let src = self.registers().read(rs2).raw();
                let old = self.read_word(addr)?;
                let new_raw = if old.raw() < src { old.raw() } else { src };
                self.write_word(addr, Word::new(new_raw))?;
                self.registers_mut().write(rd, old);
                self.increment_pc();
                Ok(())
            }
            // AMOMAXU.W (unsigned)
            0b11100 => {
                let addr = Addr::new(self.registers().read(rs1).raw());
                let src = self.registers().read(rs2).raw();
                let old = self.read_word(addr)?;
                let new_raw = if old.raw() > src { old.raw() } else { src };
                self.write_word(addr, Word::new(new_raw))?;
                self.registers_mut().write(rd, old);
                self.increment_pc();
                Ok(())
            }
            _ => Err(SimError::UnsupportedInstruction {
                pc: self.pc(),
                message: format!("Unsupported AMO funct5={:05b}", funct5),
            }),
        }
    }

    /// Execute SYSTEM/FENCE instructions.
    fn execute_system(
        &mut self,
        op: u8,
        rd: RegIdx,
        rs1: RegIdx,
        funct3: u8,
        imm: u32,
    ) -> Result<()> {
        if op == opcode::FENCE {
            // FENCE/FENCE.I are treated as NOP in this single-core in-order simulator.
            self.increment_pc();
            return Ok(());
        }

        match funct3 {
            0 => {
                // PRIV instructions (ECALL, EBREAK, MRET, SRET, WFI, SFENCE.VMA, ...)
                match imm & 0xFFF {
                    0 => {
                        // ECALL - Environment call
                        let cause = ExceptionCause::ecall_from(self.privilege());
                        self.raise_exception(cause, 0);
                    }
                    1 => {
                        // EBREAK - Environment break
                        self.raise_exception(ExceptionCause::Breakpoint, 0);
                    }
                    0x002 => {
                        // URET - Return from User-mode trap (not yet modeled)
                        self.increment_pc();
                    }
                    0x102 => {
                        // SRET - Return from Supervisor-mode trap
                        self.execute_sret()?;
                    }
                    0x105 => {
                        // WFI - wait for interrupt (modeled as NOP)
                        self.increment_pc();
                    }
                    0x120 => {
                        // SFENCE.VMA - TLB flush (no cached TLB in current model)
                        self.increment_pc();
                    }
                    0x302 => {
                        // MRET - Return from Machine-mode trap
                        self.execute_mret()?;
                    }
                    _ => {
                        return Err(SimError::UnsupportedInstruction {
                            pc: self.pc(),
                            message: format!("Unknown PRIV instruction: 0x{:03x}", imm & 0xFFF),
                        });
                    }
                }
            }
            0b001 | 0b010 | 0b011 | 0b101 | 0b110 | 0b111 => {
                let csr_addr = (imm & 0xFFF) as u16;
                let op = match funct3 {
                    0b001 => CsrOp::ReadWrite,
                    0b010 => CsrOp::ReadSet,
                    0b011 => CsrOp::ReadClear,
                    0b101 => CsrOp::ReadWriteImm,
                    0b110 => CsrOp::ReadSetImm,
                    0b111 => CsrOp::ReadClearImm,
                    _ => unreachable!(),
                };

                let rs1_val = if matches!(
                    op,
                    CsrOp::ReadWriteImm | CsrOp::ReadSetImm | CsrOp::ReadClearImm
                ) {
                    rs1.raw() as u32
                } else {
                    self.registers().read(rs1).raw()
                };

                let privilege = self.privilege();
                let old = self.csr_mut().execute(op, csr_addr, rs1_val, privilege)?;

                if !rd.is_zero() {
                    self.registers_mut().write(rd, Word::new(old));
                }

                self.increment_pc();
            }
            _ => {
                return Err(SimError::UnsupportedInstruction {
                    pc: self.pc(),
                    message: format!("Unknown SYSTEM funct3: {:03b}", funct3),
                });
            }
        }

        Ok(())
    }

    /// Execute MRET instruction.
    ///
    /// MRET is used to return from a trap taken in M-mode.
    /// It restores the PC from mepc and privilege level from mstatus.MPP.
    fn execute_mret(&mut self) -> Result<()> {
        // MRET can only be executed in M-mode
        if self.privilege() != PrivilegeLevel::Machine {
            return Err(SimError::InvalidInstruction {
                pc: self.pc(),
                instruction: 0x30200073, // MRET instruction encoding
            });
        }

        // Get return PC from mepc
        let return_pc = self.csr().mepc.get();

        // Restore privilege level from mstatus.MPP
        let new_priv = self.csr().mstatus.mpp();

        // Restore interrupt enable from mstatus.MPIE to mstatus.MIE
        let mpie = self.csr().mstatus.mpie();
        self.csr_mut().mstatus.set_mie(mpie);

        // Set MPIE to 1
        self.csr_mut().mstatus.set_mpie(true);

        // Set MPP to U-mode (0)
        self.csr_mut().mstatus.set_mpp(PrivilegeLevel::User);

        // Update privilege level
        self.set_privilege(new_priv);

        // Jump to return PC
        self.set_pc(return_pc);

        Ok(())
    }

    /// Execute SRET instruction.
    ///
    /// SRET is used to return from a trap taken in S-mode.
    /// It restores the PC from sepc and privilege level from sstatus.SPP.
    fn execute_sret(&mut self) -> Result<()> {
        if self.privilege() != PrivilegeLevel::Supervisor {
            return Err(SimError::InvalidInstruction {
                pc: self.pc(),
                instruction: 0x10200073, // SRET instruction encoding
            });
        }

        // Get return PC from sepc
        let return_pc = self.csr().sepc.get();

        // Restore privilege level from sstatus.SPP
        let new_priv = if self.csr().sstatus.spp() {
            PrivilegeLevel::Supervisor
        } else {
            PrivilegeLevel::User
        };

        // Restore interrupt enable from sstatus.SPIE to sstatus.SIE
        let spie = self.csr().sstatus.spie();
        self.csr_mut().sstatus.set_sie(spie);

        // Set SPIE to 1 and SPP to U per spec
        self.csr_mut().sstatus.set_spie(true);
        self.csr_mut().sstatus.set_spp(PrivilegeLevel::User);

        // Update privilege level and jump to return PC
        self.set_privilege(new_priv);
        self.set_pc(return_pc);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::csr::machine::medeleg_bits;
    use crate::cpu::csr::{csr_addr, exception_code, TrapVectorMode};
    use crate::memory::Ram;
    use crate::peripheral::{Lpu, Npu};

    fn create_test_cpu() -> Cpu {
        let mut bus = crate::memory::Bus::new();
        let ram = Ram::new(4096);
        bus.attach_memory(Addr::new(0), ram, "RAM");
        Cpu::new(bus)
    }

    fn create_test_cpu_with_coprocessors() -> Cpu {
        let mut bus = crate::memory::Bus::new();
        let ram = Ram::new(0x4000);
        bus.attach_memory(Addr::new(0), ram, "RAM");
        bus.attach_peripheral(Npu::new());
        bus.attach_peripheral(Lpu::new());
        Cpu::new(bus)
    }

    #[test]
    fn test_add() {
        let mut cpu = create_test_cpu();
        cpu.registers_mut().write(RegIdx::new(1), Word::new(10));
        cpu.registers_mut().write(RegIdx::new(2), Word::new(20));

        // ADD x3, x1, x2
        let instr = RType {
            rd: RegIdx::new(3),
            rs1: RegIdx::new(1),
            rs2: RegIdx::new(2),
            funct3: funct3::ADD_SUB,
            funct7: funct7::BASE,
        };

        cpu.execute_r_type(instr).unwrap();
        assert_eq!(cpu.registers().read(RegIdx::new(3)).raw(), 30);
    }

    #[test]
    fn test_sub() {
        let mut cpu = create_test_cpu();
        cpu.registers_mut().write(RegIdx::new(1), Word::new(30));
        cpu.registers_mut().write(RegIdx::new(2), Word::new(12));

        // SUB x3, x1, x2
        let instr = RType {
            rd: RegIdx::new(3),
            rs1: RegIdx::new(1),
            rs2: RegIdx::new(2),
            funct3: funct3::ADD_SUB,
            funct7: funct7::ALT,
        };

        cpu.execute_r_type(instr).unwrap();
        assert_eq!(cpu.registers().read(RegIdx::new(3)).raw(), 18);
    }

    #[test]
    fn test_addi() {
        let mut cpu = create_test_cpu();
        cpu.registers_mut().write(RegIdx::new(1), Word::new(10));

        // ADDI x2, x1, 5
        let instr = IType {
            rd: RegIdx::new(2),
            rs1: RegIdx::new(1),
            imm: 5,
            funct3: funct3::ADD_SUB,
        };

        cpu.execute_i_type(instr).unwrap();
        assert_eq!(cpu.registers().read(RegIdx::new(2)).raw(), 15);
    }

    #[test]
    fn test_addi_negative() {
        let mut cpu = create_test_cpu();
        cpu.registers_mut().write(RegIdx::new(1), Word::new(10));

        // ADDI x2, x1, -5
        let instr = IType {
            rd: RegIdx::new(2),
            rs1: RegIdx::new(1),
            imm: -5,
            funct3: funct3::ADD_SUB,
        };

        cpu.execute_i_type(instr).unwrap();
        assert_eq!(cpu.registers().read(RegIdx::new(2)).raw(), 5);
    }

    #[test]
    fn test_beq_taken() {
        let mut cpu = create_test_cpu();
        cpu.registers_mut().write(RegIdx::new(1), Word::new(10));
        cpu.registers_mut().write(RegIdx::new(2), Word::new(10));
        cpu.set_pc(Addr::new(0x1000));

        // BEQ x1, x2, +16
        let instr = BType {
            rs1: RegIdx::new(1),
            rs2: RegIdx::new(2),
            imm: 16,
            funct3: funct3::BEQ,
        };

        cpu.execute_b_type(instr).unwrap();
        assert_eq!(cpu.pc().raw(), 0x1010); // Branch taken
    }

    #[test]
    fn test_beq_not_taken() {
        let mut cpu = create_test_cpu();
        cpu.registers_mut().write(RegIdx::new(1), Word::new(10));
        cpu.registers_mut().write(RegIdx::new(2), Word::new(20));
        cpu.set_pc(Addr::new(0x1000));

        // BEQ x1, x2, +16
        let instr = BType {
            rs1: RegIdx::new(1),
            rs2: RegIdx::new(2),
            imm: 16,
            funct3: funct3::BEQ,
        };

        cpu.execute_b_type(instr).unwrap();
        assert_eq!(cpu.pc().raw(), 0x1004); // PC incremented by 4
    }

    #[test]
    fn test_jal() {
        let mut cpu = create_test_cpu();
        cpu.set_pc(Addr::new(0x1000));

        // JAL x1, +100
        let instr = JType {
            rd: RegIdx::new(1),
            imm: 100,
        };

        cpu.execute_j_type(instr).unwrap();
        assert_eq!(cpu.pc().raw(), 0x1064); // PC = 0x1000 + 100
        assert_eq!(cpu.registers().read(RegIdx::new(1)).raw(), 0x1004); // Return address
    }

    #[test]
    fn test_lui() {
        let mut cpu = create_test_cpu();

        // LUI x1, 0x12345
        cpu.execute_lui(RegIdx::new(1), 0x12345000).unwrap();
        assert_eq!(cpu.registers().read(RegIdx::new(1)).raw(), 0x12345000);
    }

    #[test]
    fn test_auipc() {
        let mut cpu = create_test_cpu();
        cpu.set_pc(Addr::new(0x1000));

        // AUIPC x1, 0x12345
        cpu.execute_auipc(RegIdx::new(1), 0x12345000).unwrap();
        assert_eq!(cpu.registers().read(RegIdx::new(1)).raw(), 0x12346000);
    }

    #[test]
    fn test_rv32m_mul_div_rem() {
        let mut cpu = create_test_cpu();
        cpu.registers_mut().write(RegIdx::new(1), Word::new(21));
        cpu.registers_mut().write(RegIdx::new(2), Word::new(6));

        // MUL x3, x1, x2 => 126
        cpu.execute_r_type(RType {
            rd: RegIdx::new(3),
            rs1: RegIdx::new(1),
            rs2: RegIdx::new(2),
            funct3: funct3::ADD_SUB,
            funct7: funct7::M_EXT,
        })
        .unwrap();
        assert_eq!(cpu.registers().read(RegIdx::new(3)).raw(), 126);

        // DIV x4, x1, x2 => 3
        cpu.execute_r_type(RType {
            rd: RegIdx::new(4),
            rs1: RegIdx::new(1),
            rs2: RegIdx::new(2),
            funct3: funct3::XOR,
            funct7: funct7::M_EXT,
        })
        .unwrap();
        assert_eq!(cpu.registers().read(RegIdx::new(4)).raw(), 3);

        // REM x5, x1, x2 => 3
        cpu.execute_r_type(RType {
            rd: RegIdx::new(5),
            rs1: RegIdx::new(1),
            rs2: RegIdx::new(2),
            funct3: funct3::OR,
            funct7: funct7::M_EXT,
        })
        .unwrap();
        assert_eq!(cpu.registers().read(RegIdx::new(5)).raw(), 3);
    }

    #[test]
    fn test_rv32m_div_by_zero_behavior() {
        let mut cpu = create_test_cpu();
        cpu.registers_mut()
            .write(RegIdx::new(1), Word::new(0xFFFF_FFF0));
        cpu.registers_mut().write(RegIdx::new(2), Word::new(0));

        // DIV by zero => -1
        cpu.execute_r_type(RType {
            rd: RegIdx::new(6),
            rs1: RegIdx::new(1),
            rs2: RegIdx::new(2),
            funct3: funct3::XOR,
            funct7: funct7::M_EXT,
        })
        .unwrap();
        assert_eq!(cpu.registers().read(RegIdx::new(6)).raw(), u32::MAX);

        // REM by zero => dividend
        cpu.execute_r_type(RType {
            rd: RegIdx::new(7),
            rs1: RegIdx::new(1),
            rs2: RegIdx::new(2),
            funct3: funct3::OR,
            funct7: funct7::M_EXT,
        })
        .unwrap();
        assert_eq!(cpu.registers().read(RegIdx::new(7)).raw(), 0xFFFF_FFF0);
    }

    #[test]
    fn test_rv32m_mulh_divu_remu() {
        let mut cpu = create_test_cpu();

        // Use values that produce a non-zero high part.
        cpu.registers_mut()
            .write(RegIdx::new(1), Word::new(0xFFFF_FFFF)); // -1 signed
        cpu.registers_mut()
            .write(RegIdx::new(2), Word::new(0x8000_0000));

        // MULH(-1, 0x8000_0000) => upper 32 bits of signed product = 0
        cpu.execute_r_type(RType {
            rd: RegIdx::new(8),
            rs1: RegIdx::new(1),
            rs2: RegIdx::new(2),
            funct3: funct3::SLL,
            funct7: funct7::M_EXT,
        })
        .unwrap();
        assert_eq!(cpu.registers().read(RegIdx::new(8)).raw(), 0);

        // MULHU(0xFFFF_FFFF, 0x8000_0000) => upper 32 bits = 0x7FFF_FFFF
        cpu.execute_r_type(RType {
            rd: RegIdx::new(9),
            rs1: RegIdx::new(1),
            rs2: RegIdx::new(2),
            funct3: funct3::SLTU,
            funct7: funct7::M_EXT,
        })
        .unwrap();
        assert_eq!(cpu.registers().read(RegIdx::new(9)).raw(), 0x7FFF_FFFF);

        // DIVU / REMU
        cpu.registers_mut().write(RegIdx::new(3), Word::new(100));
        cpu.registers_mut().write(RegIdx::new(4), Word::new(9));

        cpu.execute_r_type(RType {
            rd: RegIdx::new(10),
            rs1: RegIdx::new(3),
            rs2: RegIdx::new(4),
            funct3: funct3::SRL_SRA,
            funct7: funct7::M_EXT,
        })
        .unwrap();
        assert_eq!(cpu.registers().read(RegIdx::new(10)).raw(), 11);

        cpu.execute_r_type(RType {
            rd: RegIdx::new(11),
            rs1: RegIdx::new(3),
            rs2: RegIdx::new(4),
            funct3: funct3::AND,
            funct7: funct7::M_EXT,
        })
        .unwrap();
        assert_eq!(cpu.registers().read(RegIdx::new(11)).raw(), 1);
    }

    #[test]
    fn test_sret_restores_pc_privilege_and_interrupt_bits() {
        let mut cpu = create_test_cpu();
        cpu.set_privilege(PrivilegeLevel::Supervisor);
        cpu.csr_mut().sepc.set(Addr::new(0x2200));
        cpu.csr_mut().sstatus.set_spp(PrivilegeLevel::User);
        cpu.csr_mut().sstatus.set_spie(true);
        cpu.csr_mut().sstatus.set_sie(false);

        cpu.execute_system(opcode::SYSTEM, RegIdx::new(0), RegIdx::new(0), 0, 0x102)
            .unwrap();

        assert_eq!(cpu.pc(), Addr::new(0x2200));
        assert_eq!(cpu.privilege(), PrivilegeLevel::User);
        assert!(cpu.csr().sstatus.sie());
        assert!(cpu.csr().sstatus.spie());
        assert!(!cpu.csr().sstatus.spp());
    }

    #[test]
    fn test_ecall_takes_machine_trap_by_default() {
        let mut cpu = create_test_cpu();
        cpu.set_pc(Addr::new(0x1000));
        cpu.set_privilege(PrivilegeLevel::User);
        cpu.csr_mut().mtvec.set_base(Addr::new(0x800));
        cpu.csr_mut().mtvec.set_mode(TrapVectorMode::Direct);

        cpu.execute_system(opcode::SYSTEM, RegIdx::new(0), RegIdx::new(0), 0, 0)
            .unwrap();

        assert_eq!(cpu.privilege(), PrivilegeLevel::Machine);
        assert_eq!(cpu.pc(), Addr::new(0x800));
        assert_eq!(cpu.csr().mepc.get(), Addr::new(0x1000));
        assert_eq!(cpu.csr().mcause.code(), exception_code::ECALL_USER);
        assert!(!cpu.csr().mcause.is_interrupt());
    }

    #[test]
    fn test_ecall_delegates_to_supervisor_when_enabled() {
        let mut cpu = create_test_cpu();
        cpu.set_pc(Addr::new(0x1234));
        cpu.set_privilege(PrivilegeLevel::User);
        cpu.csr_mut().stvec.write(0x900);

        cpu.csr_mut()
            .write(
                csr_addr::MEDELEG,
                medeleg_bits::UECL,
                PrivilegeLevel::Machine,
            )
            .unwrap();

        cpu.execute_system(opcode::SYSTEM, RegIdx::new(0), RegIdx::new(0), 0, 0)
            .unwrap();

        assert_eq!(cpu.privilege(), PrivilegeLevel::Supervisor);
        assert_eq!(cpu.pc(), Addr::new(0x900));
        assert_eq!(cpu.csr().sepc.get(), Addr::new(0x1234));
        assert_eq!(cpu.csr().scause.code(), exception_code::ECALL_USER);
        assert!(!cpu.csr().scause.is_interrupt());
    }

    #[test]
    fn test_csrrw_writes_mepc_and_returns_old_value() {
        let mut cpu = create_test_cpu();
        cpu.set_privilege(PrivilegeLevel::Machine);
        cpu.registers_mut().write(RegIdx::new(2), Word::new(0x1234));

        // CSRRW x1, mepc, x2
        cpu.execute_system(
            opcode::SYSTEM,
            RegIdx::new(1),
            RegIdx::new(2),
            0b001,
            crate::cpu::csr::csr_addr::MEPC as u32,
        )
        .unwrap();

        // old mepc defaults to 0
        assert_eq!(cpu.registers().read(RegIdx::new(1)).raw(), 0);
        // mepc is naturally aligned (bit1:0 cleared)
        assert_eq!(cpu.csr().mepc.get(), Addr::new(0x1234));
    }

    #[test]
    fn test_csrrs_with_x0_reads_without_modifying_csr() {
        let mut cpu = create_test_cpu();
        cpu.set_privilege(PrivilegeLevel::Machine);
        cpu.csr_mut()
            .write(csr_addr::MSCRATCH, 0xA5A5_5A5A, PrivilegeLevel::Machine)
            .unwrap();

        // CSRRS x3, mscratch, x0 (read-only access pattern)
        cpu.execute_system(
            opcode::SYSTEM,
            RegIdx::new(3),
            RegIdx::new(0),
            0b010,
            csr_addr::MSCRATCH as u32,
        )
        .unwrap();

        assert_eq!(cpu.registers().read(RegIdx::new(3)).raw(), 0xA5A5_5A5A);
        assert_eq!(
            cpu.csr()
                .read(csr_addr::MSCRATCH, PrivilegeLevel::Machine)
                .unwrap(),
            0xA5A5_5A5A
        );
    }

    #[test]
    fn test_amoswap_w_basic() {
        let mut cpu = create_test_cpu();
        cpu.write_word(Addr::new(0x100), Word::new(0xDEAD_BEEF))
            .unwrap();
        cpu.registers_mut().write(RegIdx::new(1), Word::new(0x100));
        cpu.registers_mut()
            .write(RegIdx::new(2), Word::new(0x1234_5678));

        // amoswap.w x3, x2, (x1)
        // funct5=00001, aq=0, rl=0, rs2=2, rs1=1, funct3=010, rd=3, opcode=0101111
        let instr = (0b00001u32 << 27)
            | (2u32 << 20)
            | (1u32 << 15)
            | (0b010u32 << 12)
            | (3u32 << 7)
            | 0x2F;

        cpu.execute_amo(instr).unwrap();

        assert_eq!(cpu.registers().read(RegIdx::new(3)).raw(), 0xDEAD_BEEF);
        assert_eq!(cpu.read_word(Addr::new(0x100)).unwrap().raw(), 0x1234_5678);
    }

    #[test]
    fn test_amoadd_w_basic() {
        let mut cpu = create_test_cpu();
        cpu.write_word(Addr::new(0x100), Word::new(0x0000_0007))
            .unwrap();
        cpu.registers_mut().write(RegIdx::new(1), Word::new(0x100));
        cpu.registers_mut()
            .write(RegIdx::new(2), Word::new(0x0000_0003));

        // amoadd.w x3, x2, (x1)
        // funct5=00000, aq=0, rl=0, rs2=2, rs1=1, funct3=010, rd=3, opcode=0101111
        let instr = (0b00000u32 << 27)
            | (2u32 << 20)
            | (1u32 << 15)
            | (0b010u32 << 12)
            | (3u32 << 7)
            | 0x2F;

        cpu.execute_amo(instr).unwrap();

        assert_eq!(cpu.registers().read(RegIdx::new(3)).raw(), 0x0000_0007);
        assert_eq!(cpu.read_word(Addr::new(0x100)).unwrap().raw(), 0x0000_000A);
    }

    #[test]
    fn test_lr_sc_w_success() {
        let mut cpu = create_test_cpu();
        cpu.write_word(Addr::new(0x100), Word::new(0xABCD_0001))
            .unwrap();
        cpu.registers_mut().write(RegIdx::new(1), Word::new(0x100));
        cpu.registers_mut()
            .write(RegIdx::new(2), Word::new(0x1234_5678));

        // lr.w x3, (x1)
        let lr_instr = (0b00010u32 << 27)
            | (0u32 << 20)
            | (1u32 << 15)
            | (0b010u32 << 12)
            | (3u32 << 7)
            | 0x2F;
        cpu.execute_amo(lr_instr).unwrap();
        assert_eq!(cpu.registers().read(RegIdx::new(3)).raw(), 0xABCD_0001);

        // sc.w x4, x2, (x1) => success, rd=0, mem overwritten
        let sc_instr = (0b00011u32 << 27)
            | (2u32 << 20)
            | (1u32 << 15)
            | (0b010u32 << 12)
            | (4u32 << 7)
            | 0x2F;
        cpu.execute_amo(sc_instr).unwrap();
        assert_eq!(cpu.registers().read(RegIdx::new(4)).raw(), 0);
        assert_eq!(cpu.read_word(Addr::new(0x100)).unwrap().raw(), 0x1234_5678);
    }

    #[test]
    fn test_sc_w_fail_without_reservation() {
        let mut cpu = create_test_cpu();
        cpu.write_word(Addr::new(0x100), Word::new(0x1111_2222))
            .unwrap();
        cpu.registers_mut().write(RegIdx::new(1), Word::new(0x100));
        cpu.registers_mut()
            .write(RegIdx::new(2), Word::new(0x3333_4444));

        // sc.w x4, x2, (x1) without prior lr.w => fail, rd=1, memory unchanged
        let sc_instr = (0b00011u32 << 27)
            | (2u32 << 20)
            | (1u32 << 15)
            | (0b010u32 << 12)
            | (4u32 << 7)
            | 0x2F;
        cpu.execute_amo(sc_instr).unwrap();

        assert_eq!(cpu.registers().read(RegIdx::new(4)).raw(), 1);
        assert_eq!(cpu.read_word(Addr::new(0x100)).unwrap().raw(), 0x1111_2222);
    }

    #[test]
    fn test_amoor_w_basic() {
        let mut cpu = create_test_cpu();
        cpu.write_word(Addr::new(0x100), Word::new(0x0F00_00F0))
            .unwrap();
        cpu.registers_mut().write(RegIdx::new(1), Word::new(0x100));
        cpu.registers_mut()
            .write(RegIdx::new(2), Word::new(0x00F0_0F00));

        // amoor.w x3, x2, (x1)
        let instr = (0b01000u32 << 27)
            | (2u32 << 20)
            | (1u32 << 15)
            | (0b010u32 << 12)
            | (3u32 << 7)
            | 0x2F;

        cpu.execute_amo(instr).unwrap();

        assert_eq!(cpu.registers().read(RegIdx::new(3)).raw(), 0x0F00_00F0);
        assert_eq!(cpu.read_word(Addr::new(0x100)).unwrap().raw(), 0x0FF0_0FF0);
    }

    #[test]
    fn test_amomaxu_w_basic() {
        let mut cpu = create_test_cpu();
        cpu.write_word(Addr::new(0x100), Word::new(0x0000_00FF))
            .unwrap();
        cpu.registers_mut().write(RegIdx::new(1), Word::new(0x100));
        cpu.registers_mut()
            .write(RegIdx::new(2), Word::new(0xFFFF_0000));

        // amomaxu.w x3, x2, (x1)
        let instr = (0b11100u32 << 27)
            | (2u32 << 20)
            | (1u32 << 15)
            | (0b010u32 << 12)
            | (3u32 << 7)
            | 0x2F;

        cpu.execute_amo(instr).unwrap();

        assert_eq!(cpu.registers().read(RegIdx::new(3)).raw(), 0x0000_00FF);
        assert_eq!(cpu.read_word(Addr::new(0x100)).unwrap().raw(), 0xFFFF_0000);
    }

    #[test]
    fn test_custom0_npu_add_fast_path() {
        let mut cpu = create_test_cpu_with_coprocessors();
        cpu.registers_mut().write(RegIdx::new(1), Word::new(7));
        cpu.registers_mut().write(RegIdx::new(2), Word::new(8));

        // custom0: funct3=000 (NPU), funct7=0 (Add), rd=x3, rs1=x1, rs2=x2, opcode=0x0B
        let instr = (0u32 << 25) | (2u32 << 20) | (1u32 << 15) | (0u32 << 12) | (3u32 << 7) | 0x0B;

        cpu.execute_custom0(instr).unwrap();

        assert_eq!(cpu.registers().read(RegIdx::new(3)).raw(), 15);
        assert_eq!(cpu.read_word(Addr::new(NPU_BASE + 0x18)).unwrap().raw(), 1);
    }

    #[test]
    fn test_custom0_lpu_legacy_opcode_rejected() {
        let mut cpu = create_test_cpu_with_coprocessors();
        cpu.registers_mut().write(RegIdx::new(1), Word::new(0b1010));
        cpu.registers_mut().write(RegIdx::new(2), Word::new(0b1100));

        // custom0: funct3=001 (LPU), funct7=2 is a removed legacy opcode
        let instr = (2u32 << 25) | (2u32 << 20) | (1u32 << 15) | (1u32 << 12) | (3u32 << 7) | 0x0B;

        let err = cpu.execute_custom0(instr).unwrap_err();
        assert!(matches!(err, SimError::UnsupportedInstruction { .. }));
    }

    #[test]
    fn test_custom0_lpu_byte_tokenize_fast_path() {
        let mut cpu = create_test_cpu_with_coprocessors();
        cpu.registers_mut().write(RegIdx::new(1), Word::new('A' as u32));
        cpu.registers_mut().write(RegIdx::new(2), Word::new(0));

        // custom0: funct3=001 (LPU), funct7=0x10 (ByteTokenize)
        let instr = (0x10u32 << 25) | (2u32 << 20) | (1u32 << 15) | (1u32 << 12) | (3u32 << 7) | 0x0B;

        cpu.execute_custom0(instr).unwrap();

        // alphabetic => token class 1
        assert_eq!(cpu.registers().read(RegIdx::new(3)).raw(), 1);
    }

    #[test]
    fn test_custom0_lpu_embedding_bag_fast_path() {
        let mut cpu = create_test_cpu_with_coprocessors();
        cpu.registers_mut().write(RegIdx::new(1), Word::new(2));
        cpu.registers_mut().write(RegIdx::new(2), Word::new(0));

        // custom0: funct3=001 (LPU), funct7=0x11 (EmbeddingBag)
        let instr =
            (0x11u32 << 25) | (2u32 << 20) | (1u32 << 15) | (1u32 << 12) | (3u32 << 7) | 0x0B;

        cpu.execute_custom0(instr).unwrap();

        // token id 2 => embedding value 6
        assert_eq!(cpu.registers().read(RegIdx::new(3)).raw(), 6);
    }

    #[test]
    fn test_custom0_lpu_greedy_decode_fast_path() {
        let mut cpu = create_test_cpu_with_coprocessors();
        cpu.registers_mut().write(RegIdx::new(1), Word::new(7));
        cpu.registers_mut().write(RegIdx::new(2), Word::new(9));

        // custom0: funct3=001 (LPU), funct7=0x12 (GreedyDecode)
        let instr =
            (0x12u32 << 25) | (2u32 << 20) | (1u32 << 15) | (1u32 << 12) | (3u32 << 7) | 0x0B;

        cpu.execute_custom0(instr).unwrap();

        // score1 > score0 => token id 1
        assert_eq!(cpu.registers().read(RegIdx::new(3)).raw(), 1);
    }

    #[test]
    fn test_custom0_lpu_topk_sample_decode_fast_path() {
        let mut cpu = create_test_cpu_with_coprocessors();
        // Configure decode params on LPU MMIO.
        cpu.write_word(Addr::new(LPU_BASE + 0x3C), Word::new(2)).unwrap(); // top_k
        cpu.write_word(Addr::new(LPU_BASE + 0x40), Word::new(10_000))
            .unwrap(); // temperature milli
        cpu.write_word(Addr::new(LPU_BASE + 0x44), Word::new(5)).unwrap(); // seed

        cpu.registers_mut().write(RegIdx::new(1), Word::new(100));
        cpu.registers_mut().write(RegIdx::new(2), Word::new(90));

        // custom0: funct3=001 (LPU), funct7=0x13 (TopKSampleDecode)
        let instr =
            (0x13u32 << 25) | (2u32 << 20) | (1u32 << 15) | (1u32 << 12) | (3u32 << 7) | 0x0B;

        cpu.execute_custom0(instr).unwrap();

        // With seed=5 and weights [2,1], sampling selects token id 1
        assert_eq!(cpu.registers().read(RegIdx::new(3)).raw(), 1);
    }

    #[test]
    fn test_custom0_lpu_topp_sample_decode_fast_path() {
        let mut cpu = create_test_cpu_with_coprocessors();
        // Configure decode params on LPU MMIO.
        cpu.write_word(Addr::new(LPU_BASE + 0x4C), Word::new(1000))
            .unwrap(); // top_p milli
        cpu.write_word(Addr::new(LPU_BASE + 0x40), Word::new(10_000))
            .unwrap(); // temperature milli
        cpu.write_word(Addr::new(LPU_BASE + 0x44), Word::new(5)).unwrap(); // seed

        cpu.registers_mut().write(RegIdx::new(1), Word::new(100));
        cpu.registers_mut().write(RegIdx::new(2), Word::new(90));

        // custom0: funct3=001 (LPU), funct7=0x14 (TopPSampleDecode)
        let instr =
            (0x14u32 << 25) | (2u32 << 20) | (1u32 << 15) | (1u32 << 12) | (3u32 << 7) | 0x0B;

        cpu.execute_custom0(instr).unwrap();

        // With seed=5 and weights [2,1], sampling selects token id 1
        assert_eq!(cpu.registers().read(RegIdx::new(3)).raw(), 1);
    }

    #[test]
    fn test_custom0_invalid_opcode_rejected() {
        let mut cpu = create_test_cpu_with_coprocessors();
        cpu.registers_mut().write(RegIdx::new(1), Word::new(1));
        cpu.registers_mut().write(RegIdx::new(2), Word::new(2));

        // funct3=000 (NPU), funct7=31 -> invalid for current NPU op set
        let instr = (31u32 << 25) | (2u32 << 20) | (1u32 << 15) | (0u32 << 12) | (3u32 << 7) | 0x0B;

        let err = cpu.execute_custom0(instr).unwrap_err();
        assert!(matches!(err, SimError::UnsupportedInstruction { .. }));
    }

    #[test]
    fn test_custom0_invalid_lpu_opcode_rejected() {
        let mut cpu = create_test_cpu_with_coprocessors();
        cpu.registers_mut().write(RegIdx::new(1), Word::new(1));
        cpu.registers_mut().write(RegIdx::new(2), Word::new(2));

        // funct3=001 (LPU), funct7=31 -> invalid for current LPU op set
        let instr = (31u32 << 25) | (2u32 << 20) | (1u32 << 15) | (1u32 << 12) | (3u32 << 7) | 0x0B;

        let err = cpu.execute_custom0(instr).unwrap_err();
        assert!(matches!(err, SimError::UnsupportedInstruction { .. }));
    }
}
