//! RISC-V instruction execution.
//!
//! This module provides instruction execution logic for all RV32I instructions.

use super::decoder::DecodedInstr;
use super::format::{BType, IType, JType, RType, SType, UType};
use super::opcode::{funct3, funct7};
use crate::cpu::Cpu;
use crate::error::{Result, SimError};
use crate::types::{Addr, Byte, Half, RegIdx, Word};

impl Cpu {
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
            DecodedInstr::System { funct3, imm } => self.execute_system(funct3, imm),
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
            (funct3::ADD_SUB, funct7::BASE) => {
                Word::new(rs1_val.raw().wrapping_add(rs2_val.raw()))
            }

            // SUB: rd = rs1 - rs2
            (funct3::ADD_SUB, funct7::ALT) => {
                Word::new(rs1_val.raw().wrapping_sub(rs2_val.raw()))
            }

            // AND: rd = rs1 & rs2
            (funct3::AND, _) => Word::new(rs1_val.raw() & rs2_val.raw()),

            // OR: rd = rs1 | rs2
            (funct3::OR, _) => Word::new(rs1_val.raw() | rs2_val.raw()),

            // XOR: rd = rs1 ^ rs2
            (funct3::XOR, _) => Word::new(rs1_val.raw() ^ rs2_val.raw()),

            // SLL: rd = rs1 << (rs2 & 0x1F)
            (funct3::SLL, _) => Word::new(rs1_val.raw() << (rs2_val.raw() & 0x1F)),

            // SRL: rd = rs1 >> (rs2 & 0x1F) (logical)
            (funct3::SRL_SRA, funct7::BASE) => {
                Word::new(rs1_val.raw() >> (rs2_val.raw() & 0x1F))
            }

            // SRA: rd = rs1 >>> (rs2 & 0x1F) (arithmetic)
            (funct3::SRL_SRA, funct7::ALT) => {
                let shift = rs2_val.raw() & 0x1F;
                Word::new((rs1_val.as_signed() >> shift) as u32)
            }

            // SLT: rd = (rs1 < rs2) ? 1 : 0 (signed)
            (funct3::SLT, _) => {
                Word::new(if rs1_val.as_signed() < rs2_val.as_signed() { 1 } else { 0 })
            }

            // SLTU: rd = (rs1 < rs2) ? 1 : 0 (unsigned)
            (funct3::SLTU, _) => {
                Word::new(if rs1_val.raw() < rs2_val.raw() { 1 } else { 0 })
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
                let result = Word::new(if rs1_val.as_signed() < instr.imm { 1 } else { 0 });
                self.registers_mut().write(instr.rd, result);
                self.increment_pc();
            }

            // SLTIU: rd = (rs1 < imm) ? 1 : 0 (unsigned, but imm is still sign-extended)
            funct3::SLTU => {
                let result = Word::new(if rs1_val.raw() < (instr.imm as u32) { 1 } else { 0 });
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
                let byte = self.bus().read_byte(addr)?;
                Word::from_byte(byte.raw())
            }

            // LH: Load halfword, sign-extend
            funct3::LH => {
                let half = self.bus().read_half(addr)?;
                Word::from_half(half.raw())
            }

            // LW: Load word
            funct3::LW => self.bus().read_word(addr)?,

            // LBU: Load byte, zero-extend
            funct3::LBU => {
                let byte = self.bus().read_byte(addr)?;
                Word::from_byte_zero(byte.raw())
            }

            // LHU: Load halfword, zero-extend
            funct3::LHU => {
                let half = self.bus().read_half(addr)?;
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
                self.bus_mut().write_byte(addr, Byte::new(rs2_val.raw() as u8))?;
            }

            // SH: Store halfword
            funct3::SH => {
                self.bus_mut().write_half(addr, Half::new(rs2_val.raw() as u16))?;
            }

            // SW: Store word
            funct3::SW => {
                self.bus_mut().write_word(addr, rs2_val)?;
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

    /// Execute system instructions (ECALL, EBREAK, FENCE).
    fn execute_system(&mut self, funct3: u8, imm: u32) -> Result<()> {
        match funct3 {
            0 => {
                // ECALL or EBREAK
                match imm & 0xFFF {
                    0 => {
                        // ECALL
                        return Err(SimError::Ecall {
                            mode: format!("{:?}", self.privilege()),
                        });
                    }
                    1 => {
                        // EBREAK
                        return Err(SimError::Ebreak(self.pc()));
                    }
                    _ => {
                        return Err(SimError::UnsupportedInstruction {
                            pc: self.pc(),
                            message: format!("Unknown SYSTEM imm: 0x{:03x}", imm & 0xFFF),
                        });
                    }
                }
            }
            1 => {
                // FENCE.I - instruction cache flush (NOP for single-threaded simulator)
                self.increment_pc();
            }
            _ => {
                // Other FENCE variants - treat as NOP
                self.increment_pc();
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Ram;

    fn create_test_cpu() -> Cpu {
        let mut bus = crate::memory::Bus::new();
        let ram = Ram::new(4096);
        bus.attach_memory(Addr::new(0), ram, "RAM");
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
}
