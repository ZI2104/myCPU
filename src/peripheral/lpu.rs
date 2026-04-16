//! LPU MMIO coprocessor (Language Processing Unit).

use crate::error::{Result, SimError};
use crate::peripheral::dma;
use crate::traits::Peripheral;
use crate::types::Addr;

/// LPU base address (V2 topology).
pub const LPU_BASE: u32 = 0x2001_1000;
/// LPU MMIO region size.
pub const LPU_SIZE: usize = 0x100;

const REG_CONTROL: u32 = 0x00;
const REG_STATUS: u32 = 0x04;
const REG_OP_A: u32 = 0x08;
const REG_OP_B: u32 = 0x0C;
const REG_RESULT: u32 = 0x10;
const REG_OPCODE: u32 = 0x14;
const REG_CYCLES: u32 = 0x18;
const REG_DESC_ADDR_LOW: u32 = 0x20;
const REG_DESC_ADDR_HIGH: u32 = 0x24;
const REG_DESC_LEN: u32 = 0x28;
const REG_DESC_NOTIFY: u32 = 0x2C;
const REG_TASKS_DONE: u32 = 0x30;
const REG_TASKS_ERROR: u32 = 0x34;
const REG_DESC_NOTIFY_COUNT: u32 = 0x38;
const REG_DECODE_TOP_K: u32 = 0x3C;
const REG_DECODE_TEMPERATURE: u32 = 0x40;
const REG_DECODE_SEED: u32 = 0x44;
const REG_SAMPLED_DECODES: u32 = 0x48;
const REG_DECODE_TOP_P: u32 = 0x4C;
const REG_NUCLEUS_DECODES: u32 = 0x50;

const LPU_DESC_STRIDE: u64 = 16;

/// Language opcode: classify one input byte into a simple token class.
///
/// Token classes:
/// - 0: whitespace
/// - 1: alphabetic
/// - 2: digit
/// - 3: punctuation/ASCII symbol
/// - 4: other
pub const LPU_OPCODE_BYTE_TOKENIZE: u32 = 0x10;

/// Language opcode: fixed-table EmbeddingBag (sum pooling).
///
/// MVP semantics:
/// - Single-step mode (`START`): `op_a` is a token id, `result` is embedding value.
/// - Descriptor mode: `[opcode, input_addr, bag_len, output_addr]`
///   reads `bag_len` u32 token ids from `input_addr`, looks up fixed embeddings,
///   and writes one pooled u32 sum to `output_addr`.
pub const LPU_OPCODE_EMBEDDING_BAG: u32 = 0x11;

/// Language opcode: GreedyDecode (argmax).
///
/// MVP semantics:
/// - Single-step mode (`START`): binary greedy decode over two candidate scores
///   (`op_a` vs `op_b`), result is token id `0` or `1`.
/// - Descriptor mode: `[opcode, input_addr, vocab_size, output_addr]`
///   reads `vocab_size` u32 scores and writes argmax token id to `output_addr`.
pub const LPU_OPCODE_GREEDY_DECODE: u32 = 0x12;

/// Language opcode: TopKSampleDecode (top-k + temperature sampling).
///
/// MVP semantics:
/// - Decode parameters are configured via MMIO:
///   - `REG_DECODE_TOP_K (0x3C)`
///   - `REG_DECODE_TEMPERATURE (0x40, milli scale; 1000 = 1.0)`
///   - `REG_DECODE_SEED (0x44)`
/// - Single-step mode (`START`): binary candidate sampling over `op_a/op_b`.
/// - Descriptor mode: `[opcode, input_addr, vocab_size, output_addr]`
///   reads `vocab_size` u32 scores and writes sampled token id to `output_addr`.
pub const LPU_OPCODE_TOPK_SAMPLE_DECODE: u32 = 0x13;

/// Language opcode: TopPSampleDecode (nucleus + temperature sampling).
///
/// MVP semantics:
/// - Decode parameters are configured via MMIO:
///   - `REG_DECODE_TOP_P (0x4C, milli scale; 900 = 0.9)`
///   - `REG_DECODE_TEMPERATURE (0x40, milli scale; 1000 = 1.0)`
///   - `REG_DECODE_SEED (0x44)`
/// - Single-step mode (`START`): binary candidate sampling over `op_a/op_b`.
/// - Descriptor mode: `[opcode, input_addr, vocab_size, output_addr]`
///   reads `vocab_size` u32 scores and writes sampled token id to `output_addr`.
pub const LPU_OPCODE_TOPP_SAMPLE_DECODE: u32 = 0x14;

mod control_bits {
    pub const START: u32 = 1 << 0;
    pub const IRQ_EN: u32 = 1 << 1;
}

mod status_bits {
    pub const BUSY: u32 = 1 << 0;
    pub const DONE: u32 = 1 << 1;
    pub const IRQ_PENDING: u32 = 1 << 2;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LpuOp {
    ByteTokenize = LPU_OPCODE_BYTE_TOKENIZE as isize,
    EmbeddingBag = LPU_OPCODE_EMBEDDING_BAG as isize,
    GreedyDecode = LPU_OPCODE_GREEDY_DECODE as isize,
    TopKSampleDecode = LPU_OPCODE_TOPK_SAMPLE_DECODE as isize,
    TopPSampleDecode = LPU_OPCODE_TOPP_SAMPLE_DECODE as isize,
}

#[derive(Debug, Clone, Copy)]
pub struct LpuSnapshot {
    pub control: u32,
    pub status: u32,
    pub opcode: u32,
    pub cycles: u32,
    pub desc_addr: u64,
    pub desc_len: u32,
    pub tasks_done: u32,
    pub tasks_error: u32,
    pub desc_notify_count: u64,
    pub pending_desc_notify: bool,
    pub bytes_processed: u64,
    pub tokens_generated: u64,
    pub embedding_lookups: u64,
    pub embedding_bags: u64,
    pub decode_candidates_evaluated: u64,
    pub decoded_tokens: u64,
    pub decode_top_k: u32,
    pub decode_temperature_milli: u32,
    pub decode_seed: u32,
    pub sampled_decodes: u64,
    pub decode_top_p_milli: u32,
    pub nucleus_decodes: u64,
}

impl LpuOp {
    fn from_u32(v: u32) -> Option<Self> {
        match v {
            LPU_OPCODE_BYTE_TOKENIZE => Some(Self::ByteTokenize),
            LPU_OPCODE_EMBEDDING_BAG => Some(Self::EmbeddingBag),
            LPU_OPCODE_GREEDY_DECODE => Some(Self::GreedyDecode),
            LPU_OPCODE_TOPK_SAMPLE_DECODE => Some(Self::TopKSampleDecode),
            LPU_OPCODE_TOPP_SAMPLE_DECODE => Some(Self::TopPSampleDecode),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Lpu {
    base: Addr,
    control: u32,
    status: u32,
    op_a: u32,
    op_b: u32,
    result: u32,
    opcode: u32,
    cycles: u32,
    desc_addr: u64,
    desc_len: u32,
    tasks_done: u32,
    tasks_error: u32,
    desc_notify_count: u64,
    pending_desc_notify: bool,
    bytes_processed: u64,
    tokens_generated: u64,
    embedding_lookups: u64,
    embedding_bags: u64,
    decode_candidates_evaluated: u64,
    decoded_tokens: u64,
    decode_top_k: u32,
    decode_temperature_milli: u32,
    decode_seed: u32,
    sampled_decodes: u64,
    decode_top_p_milli: u32,
    nucleus_decodes: u64,
}

impl Default for Lpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Lpu {
    pub fn new() -> Self {
        Self::with_base(Addr::new(LPU_BASE))
    }

    pub fn with_base(base: Addr) -> Self {
        Self {
            base,
            control: 0,
            status: 0,
            op_a: 0,
            op_b: 0,
            result: 0,
            opcode: 0,
            cycles: 0,
            desc_addr: 0,
            desc_len: 0,
            tasks_done: 0,
            tasks_error: 0,
            desc_notify_count: 0,
            pending_desc_notify: false,
            bytes_processed: 0,
            tokens_generated: 0,
            embedding_lookups: 0,
            embedding_bags: 0,
            decode_candidates_evaluated: 0,
            decoded_tokens: 0,
            decode_top_k: 1,
            decode_temperature_milli: 1000,
            decode_seed: 1,
            sampled_decodes: 0,
            decode_top_p_milli: 900,
            nucleus_decodes: 0,
        }
    }

    pub fn snapshot(&self) -> LpuSnapshot {
        LpuSnapshot {
            control: self.control,
            status: self.status,
            opcode: self.opcode,
            cycles: self.cycles,
            desc_addr: self.desc_addr,
            desc_len: self.desc_len,
            tasks_done: self.tasks_done,
            tasks_error: self.tasks_error,
            desc_notify_count: self.desc_notify_count,
            pending_desc_notify: self.pending_desc_notify,
            bytes_processed: self.bytes_processed,
            tokens_generated: self.tokens_generated,
            embedding_lookups: self.embedding_lookups,
            embedding_bags: self.embedding_bags,
            decode_candidates_evaluated: self.decode_candidates_evaluated,
            decoded_tokens: self.decoded_tokens,
            decode_top_k: self.decode_top_k,
            decode_temperature_milli: self.decode_temperature_milli,
            decode_seed: self.decode_seed,
            sampled_decodes: self.sampled_decodes,
            decode_top_p_milli: self.decode_top_p_milli,
            nucleus_decodes: self.nucleus_decodes,
        }
    }

    fn read_reg(&self, reg: u32) -> u32 {
        match reg {
            REG_CONTROL => self.control,
            REG_STATUS => self.status,
            REG_OP_A => self.op_a,
            REG_OP_B => self.op_b,
            REG_RESULT => self.result,
            REG_OPCODE => self.opcode,
            REG_CYCLES => self.cycles,
            REG_DESC_ADDR_LOW => self.desc_addr as u32,
            REG_DESC_ADDR_HIGH => (self.desc_addr >> 32) as u32,
            REG_DESC_LEN => self.desc_len,
            REG_TASKS_DONE => self.tasks_done,
            REG_TASKS_ERROR => self.tasks_error,
            REG_DESC_NOTIFY_COUNT => self.desc_notify_count as u32,
            REG_DECODE_TOP_K => self.decode_top_k,
            REG_DECODE_TEMPERATURE => self.decode_temperature_milli,
            REG_DECODE_SEED => self.decode_seed,
            REG_SAMPLED_DECODES => self.sampled_decodes as u32,
            REG_DECODE_TOP_P => self.decode_top_p_milli,
            REG_NUCLEUS_DECODES => self.nucleus_decodes as u32,
            _ => 0,
        }
    }

    fn write_reg(&mut self, reg: u32, value: u32) {
        match reg {
            REG_CONTROL => {
                self.control = value;
                if (self.control & control_bits::START) != 0 {
                    self.execute_once();
                    self.control &= !control_bits::START;
                }
            }
            REG_STATUS => {
                if (value & status_bits::DONE) != 0 {
                    self.status &= !status_bits::DONE;
                }
                if (value & status_bits::IRQ_PENDING) != 0 {
                    self.status &= !status_bits::IRQ_PENDING;
                }
            }
            REG_OP_A => self.op_a = value,
            REG_OP_B => self.op_b = value,
            REG_RESULT => self.result = value,
            REG_OPCODE => self.opcode = value,
            REG_CYCLES => self.cycles = value,
            REG_DESC_ADDR_LOW => {
                self.desc_addr = (self.desc_addr & 0xFFFF_FFFF_0000_0000) | value as u64;
            }
            REG_DESC_ADDR_HIGH => {
                self.desc_addr = (self.desc_addr & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
            }
            REG_DESC_LEN => self.desc_len = value,
            REG_DESC_NOTIFY => {
                if value != 0 {
                    self.pending_desc_notify = true;
                    self.desc_notify_count = self.desc_notify_count.wrapping_add(1);
                }
            }
            REG_TASKS_DONE => self.tasks_done = value,
            REG_TASKS_ERROR => self.tasks_error = value,
            REG_DECODE_TOP_K => self.decode_top_k = value.max(1),
            REG_DECODE_TEMPERATURE => self.decode_temperature_milli = value.max(1),
            REG_DECODE_SEED => self.decode_seed = value,
            REG_SAMPLED_DECODES => self.sampled_decodes = value as u64,
            REG_DECODE_TOP_P => self.decode_top_p_milli = value.clamp(1, 1000),
            REG_NUCLEUS_DECODES => self.nucleus_decodes = value as u64,
            _ => {}
        }
    }

    fn read_u8(&self, offset: u32) -> u8 {
        let reg = offset & !0x3;
        let shift = (offset & 0x3) * 8;
        ((self.read_reg(reg) >> shift) & 0xFF) as u8
    }

    fn write_u8(&mut self, offset: u32, value: u8) {
        let reg = offset & !0x3;
        let shift = (offset & 0x3) * 8;

        if reg == REG_DESC_NOTIFY {
            if shift == 0 {
                self.write_reg(reg, value as u32);
            }
            return;
        }

        let mut current = self.read_reg(reg);
        current &= !(0xFF << shift);
        current |= (value as u32) << shift;
        self.write_reg(reg, current);
    }

    fn compute_result(&mut self, op: LpuOp) -> u32 {
        match op {
            LpuOp::ByteTokenize => Self::classify_byte(self.op_a as u8),
            LpuOp::EmbeddingBag => Self::lookup_embedding(self.op_a),
            LpuOp::GreedyDecode => Self::greedy_decode_binary(self.op_a, self.op_b),
            LpuOp::TopKSampleDecode => self.sample_decode_binary(self.op_a, self.op_b),
            LpuOp::TopPSampleDecode => self.sample_decode_binary_topp(self.op_a, self.op_b),
        }
    }

    fn next_decode_random(&mut self) -> u32 {
        self.decode_seed = self
            .decode_seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        self.decode_seed
    }

    fn sample_from_candidates(&mut self, candidates: &[(u32, u32)]) -> u32 {
        if candidates.is_empty() {
            return 0;
        }

        let mut ranked = candidates.to_vec();
        ranked.sort_unstable_by(|(id_a, score_a), (id_b, score_b)| {
            score_b.cmp(score_a).then(id_a.cmp(id_b))
        });

        let top_k = self.decode_top_k.max(1) as usize;
        ranked.truncate(top_k.min(ranked.len()));

        if ranked.len() == 1 || self.decode_temperature_milli <= 1 {
            return ranked[0].0;
        }

        let min_score = ranked.last().map(|(_, score)| *score).unwrap_or(0);
        let temp = self.decode_temperature_milli.max(1) as u64;

        let weights: Vec<u64> = ranked
            .iter()
            .map(|(_, score)| 1 + (((score.saturating_sub(min_score)) as u64 * 1000) / temp))
            .collect();

        let total_weight = weights
            .iter()
            .fold(0u64, |acc, weight| acc.saturating_add(*weight));
        if total_weight == 0 {
            return ranked[0].0;
        }

        let ticket = (self.next_decode_random() as u64) % total_weight;
        let mut acc = 0u64;
        for ((token_id, _), weight) in ranked.iter().zip(weights.iter()) {
            acc = acc.saturating_add(*weight);
            if ticket < acc {
                return *token_id;
            }
        }

        ranked[0].0
    }

    fn sample_decode_binary(&mut self, score0: u32, score1: u32) -> u32 {
        let candidates = [(0u32, score0), (1u32, score1)];
        self.sample_from_candidates(&candidates)
    }

    fn sample_from_nucleus_candidates(&mut self, candidates: &[(u32, u32)]) -> u32 {
        if candidates.is_empty() {
            return 0;
        }

        let mut ranked = candidates.to_vec();
        ranked.sort_unstable_by(|(id_a, score_a), (id_b, score_b)| {
            score_b.cmp(score_a).then(id_a.cmp(id_b))
        });

        if ranked.len() == 1 || self.decode_temperature_milli <= 1 {
            return ranked[0].0;
        }

        let min_score = ranked.last().map(|(_, score)| *score).unwrap_or(0);
        let temp = self.decode_temperature_milli.max(1) as u64;

        let weighted_ranked: Vec<(u32, u64)> = ranked
            .iter()
            .map(|(token_id, score)| {
                let weight = 1 + (((score.saturating_sub(min_score)) as u64 * 1000) / temp);
                (*token_id, weight)
            })
            .collect();

        let total_weight = weighted_ranked
            .iter()
            .fold(0u64, |acc, (_, weight)| acc.saturating_add(*weight));
        if total_weight == 0 {
            return ranked[0].0;
        }

        let top_p_milli = self.decode_top_p_milli.clamp(1, 1000) as u64;
        let threshold_weight =
            (total_weight.saturating_mul(top_p_milli).saturating_add(999)) / 1000;

        let mut nucleus: Vec<(u32, u64)> = Vec::new();
        let mut nucleus_weight = 0u64;
        for (token_id, weight) in weighted_ranked {
            nucleus_weight = nucleus_weight.saturating_add(weight);
            nucleus.push((token_id, weight));
            if nucleus_weight >= threshold_weight {
                break;
            }
        }

        if nucleus.is_empty() {
            return ranked[0].0;
        }

        if nucleus.len() == 1 {
            return nucleus[0].0;
        }

        let ticket = (self.next_decode_random() as u64) % nucleus_weight;
        let mut acc = 0u64;
        for (token_id, weight) in &nucleus {
            acc = acc.saturating_add(*weight);
            if ticket < acc {
                return *token_id;
            }
        }

        nucleus[0].0
    }

    fn sample_decode_binary_topp(&mut self, score0: u32, score1: u32) -> u32 {
        let candidates = [(0u32, score0), (1u32, score1)];
        self.sample_from_nucleus_candidates(&candidates)
    }

    fn greedy_decode_binary(score0: u32, score1: u32) -> u32 {
        if score0 >= score1 {
            0
        } else {
            1
        }
    }

    fn lookup_embedding(token_id: u32) -> u32 {
        const EMBEDDING_TABLE: [u32; 5] = [1, 4, 6, 3, 2];
        EMBEDDING_TABLE.get(token_id as usize).copied().unwrap_or(0)
    }

    fn classify_byte(byte: u8) -> u32 {
        match byte {
            b' ' | b'\n' | b'\r' | b'\t' => 0,
            b'a'..=b'z' | b'A'..=b'Z' => 1,
            b'0'..=b'9' => 2,
            0x21..=0x2F | 0x3A..=0x40 | 0x5B..=0x60 | 0x7B..=0x7E => 3,
            _ => 4,
        }
    }

    fn read_guest_u8(ram: &mut dma::RamRegions, addr: u64) -> Result<u8> {
        dma::read_u8(ram, addr)
    }

    fn read_guest_u32(ram: &mut dma::RamRegions, addr: u64) -> Result<u32> {
        dma::read_u32(ram, addr)
    }

    fn write_guest_u32(ram: &mut dma::RamRegions, addr: u64, value: u32) -> Result<()> {
        dma::write_u32(ram, addr, value)
    }

    fn execute_descriptor_entry(
        &mut self,
        ram_regions: &mut dma::RamRegions,
        index: u32,
    ) -> Result<()> {
        let desc_base = self.desc_addr + (index as u64) * LPU_DESC_STRIDE;
        self.opcode = Self::read_guest_u32(ram_regions, desc_base)?;

        let op_a_or_addr = Self::read_guest_u32(ram_regions, desc_base + 4)?;
        let op_b_or_len = Self::read_guest_u32(ram_regions, desc_base + 8)?;
        let result_addr = Self::read_guest_u32(ram_regions, desc_base + 12)? as u64;

        if self.opcode == LPU_OPCODE_BYTE_TOKENIZE {
            self.execute_descriptor_byte_tokenize(
                ram_regions,
                op_a_or_addr as u64,
                op_b_or_len as usize,
                result_addr,
            )?;
            self.cycles = self.cycles.wrapping_add(op_b_or_len.max(1));
            self.tasks_done = self.tasks_done.wrapping_add(1);
            return Ok(());
        }

        if self.opcode == LPU_OPCODE_EMBEDDING_BAG {
            self.execute_descriptor_embedding_bag(
                ram_regions,
                op_a_or_addr as u64,
                op_b_or_len as usize,
                result_addr,
            )?;
            self.cycles = self.cycles.wrapping_add(op_b_or_len.max(1));
            self.tasks_done = self.tasks_done.wrapping_add(1);
            return Ok(());
        }

        if self.opcode == LPU_OPCODE_GREEDY_DECODE {
            self.execute_descriptor_greedy_decode(
                ram_regions,
                op_a_or_addr as u64,
                op_b_or_len as usize,
                result_addr,
            )?;
            self.cycles = self.cycles.wrapping_add(op_b_or_len.max(1));
            self.tasks_done = self.tasks_done.wrapping_add(1);
            return Ok(());
        }

        if self.opcode == LPU_OPCODE_TOPK_SAMPLE_DECODE {
            self.execute_descriptor_topk_sample_decode(
                ram_regions,
                op_a_or_addr as u64,
                op_b_or_len as usize,
                result_addr,
            )?;
            self.cycles = self.cycles.wrapping_add(op_b_or_len.max(1));
            self.tasks_done = self.tasks_done.wrapping_add(1);
            return Ok(());
        }

        if self.opcode == LPU_OPCODE_TOPP_SAMPLE_DECODE {
            self.execute_descriptor_topp_sample_decode(
                ram_regions,
                op_a_or_addr as u64,
                op_b_or_len as usize,
                result_addr,
            )?;
            self.cycles = self.cycles.wrapping_add(op_b_or_len.max(1));
            self.tasks_done = self.tasks_done.wrapping_add(1);
            return Ok(());
        }

        let _ = (op_a_or_addr, op_b_or_len, result_addr);
        Err(SimError::Peripheral(format!(
            "Unsupported LPU descriptor opcode: {}",
            self.opcode
        )))
    }

    fn execute_descriptor_byte_tokenize(
        &mut self,
        ram_regions: &mut dma::RamRegions,
        input_addr: u64,
        input_len: usize,
        output_addr: u64,
    ) -> Result<()> {
        for i in 0..input_len {
            let byte = Self::read_guest_u8(ram_regions, input_addr + i as u64)?;
            let token = Self::classify_byte(byte);
            Self::write_guest_u32(ram_regions, output_addr + (i as u64) * 4, token)?;
        }

        self.result = input_len as u32;
        self.bytes_processed = self.bytes_processed.wrapping_add(input_len as u64);
        self.tokens_generated = self.tokens_generated.wrapping_add(input_len as u64);
        Ok(())
    }

    fn execute_descriptor_embedding_bag(
        &mut self,
        ram_regions: &mut dma::RamRegions,
        input_addr: u64,
        bag_len: usize,
        output_addr: u64,
    ) -> Result<()> {
        let mut pooled = 0u32;

        for i in 0..bag_len {
            let token_id = Self::read_guest_u32(ram_regions, input_addr + (i as u64) * 4)?;
            pooled = pooled.wrapping_add(Self::lookup_embedding(token_id));
        }

        Self::write_guest_u32(ram_regions, output_addr, pooled)?;
        self.result = pooled;
        self.embedding_lookups = self.embedding_lookups.wrapping_add(bag_len as u64);
        self.embedding_bags = self.embedding_bags.wrapping_add(1);
        Ok(())
    }

    fn execute_descriptor_greedy_decode(
        &mut self,
        ram_regions: &mut dma::RamRegions,
        input_addr: u64,
        vocab_size: usize,
        output_addr: u64,
    ) -> Result<()> {
        let mut best_id = 0u32;
        let mut best_score = 0u32;

        for i in 0..vocab_size {
            let score = Self::read_guest_u32(ram_regions, input_addr + (i as u64) * 4)?;
            if i == 0 || score > best_score {
                best_score = score;
                best_id = i as u32;
            }
        }

        Self::write_guest_u32(ram_regions, output_addr, best_id)?;
        self.result = best_id;
        self.decode_candidates_evaluated = self
            .decode_candidates_evaluated
            .wrapping_add(vocab_size as u64);
        self.decoded_tokens = self.decoded_tokens.wrapping_add(1);
        Ok(())
    }

    fn execute_descriptor_topk_sample_decode(
        &mut self,
        ram_regions: &mut dma::RamRegions,
        input_addr: u64,
        vocab_size: usize,
        output_addr: u64,
    ) -> Result<()> {
        if vocab_size == 0 {
            return Err(SimError::Peripheral(
                "LPU TopKSampleDecode requires vocab_size > 0".to_string(),
            ));
        }

        let mut candidates = Vec::with_capacity(vocab_size);
        for i in 0..vocab_size {
            let score = Self::read_guest_u32(ram_regions, input_addr + (i as u64) * 4)?;
            candidates.push((i as u32, score));
        }

        let token_id = self.sample_from_candidates(&candidates);
        Self::write_guest_u32(ram_regions, output_addr, token_id)?;
        self.result = token_id;
        self.decode_candidates_evaluated = self
            .decode_candidates_evaluated
            .wrapping_add(vocab_size as u64);
        self.decoded_tokens = self.decoded_tokens.wrapping_add(1);
        self.sampled_decodes = self.sampled_decodes.wrapping_add(1);
        Ok(())
    }

    fn execute_descriptor_topp_sample_decode(
        &mut self,
        ram_regions: &mut dma::RamRegions,
        input_addr: u64,
        vocab_size: usize,
        output_addr: u64,
    ) -> Result<()> {
        if vocab_size == 0 {
            return Err(SimError::Peripheral(
                "LPU TopPSampleDecode requires vocab_size > 0".to_string(),
            ));
        }

        let mut candidates = Vec::with_capacity(vocab_size);
        for i in 0..vocab_size {
            let score = Self::read_guest_u32(ram_regions, input_addr + (i as u64) * 4)?;
            candidates.push((i as u32, score));
        }

        let token_id = self.sample_from_nucleus_candidates(&candidates);
        Self::write_guest_u32(ram_regions, output_addr, token_id)?;
        self.result = token_id;
        self.decode_candidates_evaluated = self
            .decode_candidates_evaluated
            .wrapping_add(vocab_size as u64);
        self.decoded_tokens = self.decoded_tokens.wrapping_add(1);
        self.sampled_decodes = self.sampled_decodes.wrapping_add(1);
        self.nucleus_decodes = self.nucleus_decodes.wrapping_add(1);
        Ok(())
    }

    pub fn has_pending_descriptor_notify(&self) -> bool {
        self.pending_desc_notify
    }

    pub fn process_pending_descriptor_notify(
        &mut self,
        ram_regions: &mut dma::RamRegions,
    ) -> Result<()> {
        if !self.pending_desc_notify {
            return Ok(());
        }

        self.pending_desc_notify = false;
        self.status |= status_bits::BUSY;
        self.status &= !status_bits::DONE;

        for index in 0..self.desc_len {
            if self.execute_descriptor_entry(ram_regions, index).is_err() {
                self.tasks_error = self.tasks_error.wrapping_add(1);
            }
        }

        self.status &= !status_bits::BUSY;
        self.status |= status_bits::DONE;

        if (self.control & control_bits::IRQ_EN) != 0 {
            self.status |= status_bits::IRQ_PENDING;
        }

        Ok(())
    }

    fn execute_once(&mut self) {
        self.status |= status_bits::BUSY;
        self.status &= !status_bits::DONE;

        let Some(op) = LpuOp::from_u32(self.opcode) else {
            self.result = 0;
            self.tasks_error = self.tasks_error.wrapping_add(1);
            self.cycles = self.cycles.wrapping_add(1);
            self.status &= !status_bits::BUSY;
            self.status |= status_bits::DONE;

            if (self.control & control_bits::IRQ_EN) != 0 {
                self.status |= status_bits::IRQ_PENDING;
            }
            return;
        };

        self.result = self.compute_result(op);

        if op == LpuOp::ByteTokenize {
            self.bytes_processed = self.bytes_processed.wrapping_add(1);
            self.tokens_generated = self.tokens_generated.wrapping_add(1);
        }

        if op == LpuOp::EmbeddingBag {
            self.embedding_lookups = self.embedding_lookups.wrapping_add(1);
            self.embedding_bags = self.embedding_bags.wrapping_add(1);
        }

        if op == LpuOp::GreedyDecode {
            self.decode_candidates_evaluated = self.decode_candidates_evaluated.wrapping_add(2);
            self.decoded_tokens = self.decoded_tokens.wrapping_add(1);
        }

        if op == LpuOp::TopKSampleDecode {
            self.decode_candidates_evaluated = self.decode_candidates_evaluated.wrapping_add(2);
            self.decoded_tokens = self.decoded_tokens.wrapping_add(1);
            self.sampled_decodes = self.sampled_decodes.wrapping_add(1);
        }

        if op == LpuOp::TopPSampleDecode {
            self.decode_candidates_evaluated = self.decode_candidates_evaluated.wrapping_add(2);
            self.decoded_tokens = self.decoded_tokens.wrapping_add(1);
            self.sampled_decodes = self.sampled_decodes.wrapping_add(1);
            self.nucleus_decodes = self.nucleus_decodes.wrapping_add(1);
        }

        self.cycles = self.cycles.wrapping_add(1);
        self.status &= !status_bits::BUSY;
        self.status |= status_bits::DONE;

        if (self.control & control_bits::IRQ_EN) != 0 {
            self.status |= status_bits::IRQ_PENDING;
        }
    }
}

impl Peripheral for Lpu {
    fn read(&self, offset: Addr) -> Result<u8> {
        let off = offset.raw();
        if off >= LPU_SIZE as u32 {
            return Err(SimError::InvalidAddress(offset));
        }
        Ok(self.read_u8(off))
    }

    fn write(&mut self, offset: Addr, value: u8) -> Result<()> {
        let off = offset.raw();
        if off >= LPU_SIZE as u32 {
            return Err(SimError::InvalidAddress(offset));
        }
        self.write_u8(off, value);
        Ok(())
    }

    fn base_addr(&self) -> Addr {
        self.base
    }

    fn size(&self) -> usize {
        LPU_SIZE
    }

    fn name(&self) -> &str {
        "LPU"
    }

    fn has_interrupt(&self) -> bool {
        (self.status & status_bits::IRQ_PENDING) != 0
    }

    fn acknowledge_interrupt(&mut self) {
        self.status &= !status_bits::IRQ_PENDING;
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn try_execute_pending(&mut self, ram_regions: &mut dma::RamRegions) -> Result<bool> {
        if !self.pending_desc_notify {
            return Ok(false);
        }
        self.process_pending_descriptor_notify(ram_regions)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_u32(lpu: &mut Lpu, reg: u32, value: u32) {
        for i in 0..4 {
            lpu.write(Addr::new(reg + i), ((value >> (i * 8)) & 0xFF) as u8)
                .unwrap();
        }
    }

    fn read_u32(lpu: &Lpu, reg: u32) -> u32 {
        let mut value = 0u32;
        for i in 0..4 {
            value |= (lpu.read(Addr::new(reg + i)).unwrap() as u32) << (i * 8);
        }
        value
    }

    #[test]
    fn test_lpu_invalid_opcode_single_step_counts_error() {
        let mut lpu = Lpu::new();
        write_u32(&mut lpu, REG_OP_A, 0b1010);
        write_u32(&mut lpu, REG_OP_B, 0b1100);
        write_u32(&mut lpu, REG_OPCODE, 2); // legacy xor opcode should be invalid now
        write_u32(&mut lpu, REG_CONTROL, control_bits::START);
        assert_eq!(read_u32(&lpu, REG_RESULT), 0);
        assert_eq!(read_u32(&lpu, REG_TASKS_ERROR), 1);
    }

    #[test]
    fn test_lpu_interrupt_ack() {
        let mut lpu = Lpu::new();
        write_u32(&mut lpu, REG_OP_A, b'A' as u32);
        write_u32(&mut lpu, REG_OP_B, 0);
        write_u32(&mut lpu, REG_OPCODE, LPU_OPCODE_BYTE_TOKENIZE);
        write_u32(
            &mut lpu,
            REG_CONTROL,
            control_bits::START | control_bits::IRQ_EN,
        );

        assert!(lpu.has_interrupt());
        lpu.acknowledge_interrupt();
        assert!(!lpu.has_interrupt());
    }

    #[test]
    fn test_lpu_descriptor_invalid_opcode_counts_error() {
        use crate::memory::Ram;

        const RAM_BASE: u32 = 0x8000_0000;
        let mut regions: dma::RamRegions =
            vec![(Addr::new(RAM_BASE), 0x4000, Box::new(Ram::new(0x4000)))];

        let mut lpu = Lpu::new();

        let desc_addr = RAM_BASE + 0x300;
        let out0 = RAM_BASE + 0x408;

        Lpu::write_guest_u32(&mut regions, desc_addr as u64, 2).unwrap(); // legacy xor opcode invalid
        Lpu::write_guest_u32(&mut regions, (desc_addr + 4) as u64, 0).unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 8) as u64, 0).unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 12) as u64, out0).unwrap();

        write_u32(&mut lpu, REG_DESC_ADDR_LOW, desc_addr);
        write_u32(&mut lpu, REG_DESC_LEN, 1);
        write_u32(&mut lpu, REG_CONTROL, control_bits::IRQ_EN);
        write_u32(&mut lpu, REG_DESC_NOTIFY, 1);

        assert!(lpu.has_pending_descriptor_notify());
        lpu.process_pending_descriptor_notify(&mut regions).unwrap();

        assert_eq!(Lpu::read_guest_u32(&mut regions, out0 as u64).unwrap(), 0);
        assert_eq!(read_u32(&lpu, REG_TASKS_DONE), 0);
        assert_eq!(read_u32(&lpu, REG_TASKS_ERROR), 1);
        assert_ne!(read_u32(&lpu, REG_STATUS) & status_bits::DONE, 0);
        assert!(lpu.has_interrupt());
    }

    #[test]
    fn test_lpu_byte_tokenize_single_step() {
        let mut lpu = Lpu::new();
        write_u32(&mut lpu, REG_OP_A, b'A' as u32);
        write_u32(&mut lpu, REG_OP_B, 0);
        write_u32(&mut lpu, REG_OPCODE, LPU_OPCODE_BYTE_TOKENIZE);
        write_u32(&mut lpu, REG_CONTROL, control_bits::START);

        // alphabetic => token class 1
        assert_eq!(read_u32(&lpu, REG_RESULT), 1);
        let snap = lpu.snapshot();
        assert_eq!(snap.bytes_processed, 1);
        assert_eq!(snap.tokens_generated, 1);
    }

    #[test]
    fn test_lpu_descriptor_byte_tokenize_batch() {
        use crate::memory::Ram;

        const RAM_BASE: u32 = 0x8000_0000;
        let mut regions: dma::RamRegions =
            vec![(Addr::new(RAM_BASE), 0x4000, Box::new(Ram::new(0x4000)))];

        let mut lpu = Lpu::new();

        let desc_addr = RAM_BASE + 0x500;
        let input_addr = RAM_BASE + 0x600;
        let out_addr = RAM_BASE + 0x700;

        // descriptor: [opcode, input_addr, input_len, out_addr]
        Lpu::write_guest_u32(&mut regions, desc_addr as u64, LPU_OPCODE_BYTE_TOKENIZE).unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 4) as u64, input_addr).unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 8) as u64, 4).unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 12) as u64, out_addr).unwrap();

        // input bytes: 'A' '1' ' ' '?'
        dma::write_u8(&mut regions, input_addr as u64, b'A').unwrap();
        dma::write_u8(&mut regions, (input_addr + 1) as u64, b'1').unwrap();
        dma::write_u8(&mut regions, (input_addr + 2) as u64, b' ').unwrap();
        dma::write_u8(&mut regions, (input_addr + 3) as u64, b'?').unwrap();

        write_u32(&mut lpu, REG_DESC_ADDR_LOW, desc_addr);
        write_u32(&mut lpu, REG_DESC_LEN, 1);
        write_u32(&mut lpu, REG_DESC_NOTIFY, 1);

        lpu.process_pending_descriptor_notify(&mut regions).unwrap();

        assert_eq!(
            Lpu::read_guest_u32(&mut regions, out_addr as u64).unwrap(),
            1
        ); // alpha
        assert_eq!(
            Lpu::read_guest_u32(&mut regions, (out_addr + 4) as u64).unwrap(),
            2
        ); // digit
        assert_eq!(
            Lpu::read_guest_u32(&mut regions, (out_addr + 8) as u64).unwrap(),
            0
        ); // whitespace
        assert_eq!(
            Lpu::read_guest_u32(&mut regions, (out_addr + 12) as u64).unwrap(),
            3
        ); // punctuation

        let snap = lpu.snapshot();
        assert_eq!(snap.tasks_done, 1);
        assert_eq!(snap.bytes_processed, 4);
        assert_eq!(snap.tokens_generated, 4);
    }

    #[test]
    fn test_lpu_embedding_bag_single_step() {
        let mut lpu = Lpu::new();
        write_u32(&mut lpu, REG_OP_A, 2);
        write_u32(&mut lpu, REG_OP_B, 0);
        write_u32(&mut lpu, REG_OPCODE, LPU_OPCODE_EMBEDDING_BAG);
        write_u32(&mut lpu, REG_CONTROL, control_bits::START);

        // token id 2 => embedding value 6
        assert_eq!(read_u32(&lpu, REG_RESULT), 6);
        let snap = lpu.snapshot();
        assert_eq!(snap.embedding_lookups, 1);
        assert_eq!(snap.embedding_bags, 1);
    }

    #[test]
    fn test_lpu_descriptor_embedding_bag_batch() {
        use crate::memory::Ram;

        const RAM_BASE: u32 = 0x8000_0000;
        let mut regions: dma::RamRegions =
            vec![(Addr::new(RAM_BASE), 0x4000, Box::new(Ram::new(0x4000)))];

        let mut lpu = Lpu::new();

        let desc_addr = RAM_BASE + 0x800;
        let input_addr = RAM_BASE + 0x900;
        let out_addr = RAM_BASE + 0xA00;

        // descriptor: [opcode, input_addr, bag_len, out_addr]
        Lpu::write_guest_u32(&mut regions, desc_addr as u64, LPU_OPCODE_EMBEDDING_BAG).unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 4) as u64, input_addr).unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 8) as u64, 4).unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 12) as u64, out_addr).unwrap();

        // token ids: [1, 2, 3, 1] => embeddings [4,6,3,4] => pooled 17
        Lpu::write_guest_u32(&mut regions, input_addr as u64, 1).unwrap();
        Lpu::write_guest_u32(&mut regions, (input_addr + 4) as u64, 2).unwrap();
        Lpu::write_guest_u32(&mut regions, (input_addr + 8) as u64, 3).unwrap();
        Lpu::write_guest_u32(&mut regions, (input_addr + 12) as u64, 1).unwrap();

        write_u32(&mut lpu, REG_DESC_ADDR_LOW, desc_addr);
        write_u32(&mut lpu, REG_DESC_LEN, 1);
        write_u32(&mut lpu, REG_DESC_NOTIFY, 1);

        lpu.process_pending_descriptor_notify(&mut regions).unwrap();

        assert_eq!(
            Lpu::read_guest_u32(&mut regions, out_addr as u64).unwrap(),
            17
        );

        let snap = lpu.snapshot();
        assert_eq!(snap.tasks_done, 1);
        assert_eq!(snap.embedding_lookups, 4);
        assert_eq!(snap.embedding_bags, 1);
    }

    #[test]
    fn test_lpu_greedy_decode_single_step() {
        let mut lpu = Lpu::new();
        write_u32(&mut lpu, REG_OP_A, 7);
        write_u32(&mut lpu, REG_OP_B, 9);
        write_u32(&mut lpu, REG_OPCODE, LPU_OPCODE_GREEDY_DECODE);
        write_u32(&mut lpu, REG_CONTROL, control_bits::START);

        // score1 > score0 => token id 1
        assert_eq!(read_u32(&lpu, REG_RESULT), 1);
        let snap = lpu.snapshot();
        assert_eq!(snap.decode_candidates_evaluated, 2);
        assert_eq!(snap.decoded_tokens, 1);
    }

    #[test]
    fn test_lpu_descriptor_greedy_decode_batch() {
        use crate::memory::Ram;

        const RAM_BASE: u32 = 0x8000_0000;
        let mut regions: dma::RamRegions =
            vec![(Addr::new(RAM_BASE), 0x4000, Box::new(Ram::new(0x4000)))];

        let mut lpu = Lpu::new();

        let desc_addr = RAM_BASE + 0xB00;
        let logits_addr = RAM_BASE + 0xC00;
        let out_addr = RAM_BASE + 0xD00;

        // descriptor: [opcode, logits_addr, vocab_size, out_addr]
        Lpu::write_guest_u32(&mut regions, desc_addr as u64, LPU_OPCODE_GREEDY_DECODE).unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 4) as u64, logits_addr).unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 8) as u64, 4).unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 12) as u64, out_addr).unwrap();

        // scores: [10, 7, 21, 3] => argmax id 2
        Lpu::write_guest_u32(&mut regions, logits_addr as u64, 10).unwrap();
        Lpu::write_guest_u32(&mut regions, (logits_addr + 4) as u64, 7).unwrap();
        Lpu::write_guest_u32(&mut regions, (logits_addr + 8) as u64, 21).unwrap();
        Lpu::write_guest_u32(&mut regions, (logits_addr + 12) as u64, 3).unwrap();

        write_u32(&mut lpu, REG_DESC_ADDR_LOW, desc_addr);
        write_u32(&mut lpu, REG_DESC_LEN, 1);
        write_u32(&mut lpu, REG_DESC_NOTIFY, 1);

        lpu.process_pending_descriptor_notify(&mut regions).unwrap();

        assert_eq!(
            Lpu::read_guest_u32(&mut regions, out_addr as u64).unwrap(),
            2
        );

        let snap = lpu.snapshot();
        assert_eq!(snap.tasks_done, 1);
        assert_eq!(snap.decode_candidates_evaluated, 4);
        assert_eq!(snap.decoded_tokens, 1);
    }

    #[test]
    fn test_lpu_topk_sample_decode_single_step() {
        let mut lpu = Lpu::new();
        write_u32(&mut lpu, REG_DECODE_TOP_K, 2);
        write_u32(&mut lpu, REG_DECODE_TEMPERATURE, 10_000);
        write_u32(&mut lpu, REG_DECODE_SEED, 5);
        write_u32(&mut lpu, REG_OP_A, 100);
        write_u32(&mut lpu, REG_OP_B, 90);
        write_u32(&mut lpu, REG_OPCODE, LPU_OPCODE_TOPK_SAMPLE_DECODE);
        write_u32(&mut lpu, REG_CONTROL, control_bits::START);

        // With seed=5 and weights [2,1], ticket=2 => pick token id 1
        assert_eq!(read_u32(&lpu, REG_RESULT), 1);
        let snap = lpu.snapshot();
        assert_eq!(snap.decode_candidates_evaluated, 2);
        assert_eq!(snap.decoded_tokens, 1);
        assert_eq!(snap.sampled_decodes, 1);
        assert_eq!(snap.decode_seed, 1_022_226_848);
    }

    #[test]
    fn test_lpu_descriptor_topk_sample_decode_batch() {
        use crate::memory::Ram;

        const RAM_BASE: u32 = 0x8000_0000;
        let mut regions: dma::RamRegions =
            vec![(Addr::new(RAM_BASE), 0x4000, Box::new(Ram::new(0x4000)))];

        let mut lpu = Lpu::new();

        let desc_addr = RAM_BASE + 0xE00;
        let logits_addr = RAM_BASE + 0xF00;
        let out_addr = RAM_BASE + 0x1000;

        // descriptor: [opcode, logits_addr, vocab_size, out_addr]
        Lpu::write_guest_u32(
            &mut regions,
            desc_addr as u64,
            LPU_OPCODE_TOPK_SAMPLE_DECODE,
        )
        .unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 4) as u64, logits_addr).unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 8) as u64, 4).unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 12) as u64, out_addr).unwrap();

        // top scores: id0=100, id1=90
        Lpu::write_guest_u32(&mut regions, logits_addr as u64, 100).unwrap();
        Lpu::write_guest_u32(&mut regions, (logits_addr + 4) as u64, 90).unwrap();
        Lpu::write_guest_u32(&mut regions, (logits_addr + 8) as u64, 80).unwrap();
        Lpu::write_guest_u32(&mut regions, (logits_addr + 12) as u64, 10).unwrap();

        write_u32(&mut lpu, REG_DECODE_TOP_K, 2);
        write_u32(&mut lpu, REG_DECODE_TEMPERATURE, 10_000);
        write_u32(&mut lpu, REG_DECODE_SEED, 5);
        write_u32(&mut lpu, REG_DESC_ADDR_LOW, desc_addr);
        write_u32(&mut lpu, REG_DESC_LEN, 1);
        write_u32(&mut lpu, REG_DESC_NOTIFY, 1);

        lpu.process_pending_descriptor_notify(&mut regions).unwrap();

        assert_eq!(
            Lpu::read_guest_u32(&mut regions, out_addr as u64).unwrap(),
            1
        );

        let snap = lpu.snapshot();
        assert_eq!(snap.tasks_done, 1);
        assert_eq!(snap.decode_candidates_evaluated, 4);
        assert_eq!(snap.decoded_tokens, 1);
        assert_eq!(snap.sampled_decodes, 1);
    }

    #[test]
    fn test_lpu_topp_sample_decode_single_step() {
        let mut lpu = Lpu::new();
        write_u32(&mut lpu, REG_DECODE_TOP_P, 1000);
        write_u32(&mut lpu, REG_DECODE_TEMPERATURE, 10_000);
        write_u32(&mut lpu, REG_DECODE_SEED, 5);
        write_u32(&mut lpu, REG_OP_A, 100);
        write_u32(&mut lpu, REG_OP_B, 90);
        write_u32(&mut lpu, REG_OPCODE, LPU_OPCODE_TOPP_SAMPLE_DECODE);
        write_u32(&mut lpu, REG_CONTROL, control_bits::START);

        // With seed=5 and nucleus p=1.0, ticket=2 over weights [2,1] => token id 1
        assert_eq!(read_u32(&lpu, REG_RESULT), 1);
        let snap = lpu.snapshot();
        assert_eq!(snap.decode_candidates_evaluated, 2);
        assert_eq!(snap.decoded_tokens, 1);
        assert_eq!(snap.sampled_decodes, 1);
        assert_eq!(snap.nucleus_decodes, 1);
    }

    #[test]
    fn test_lpu_descriptor_topp_sample_decode_batch() {
        use crate::memory::Ram;

        const RAM_BASE: u32 = 0x8000_0000;
        let mut regions: dma::RamRegions =
            vec![(Addr::new(RAM_BASE), 0x4000, Box::new(Ram::new(0x4000)))];

        let mut lpu = Lpu::new();

        let desc_addr = RAM_BASE + 0x1100;
        let logits_addr = RAM_BASE + 0x1200;
        let out_addr = RAM_BASE + 0x1300;

        // descriptor: [opcode, logits_addr, vocab_size, out_addr]
        Lpu::write_guest_u32(
            &mut regions,
            desc_addr as u64,
            LPU_OPCODE_TOPP_SAMPLE_DECODE,
        )
        .unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 4) as u64, logits_addr).unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 8) as u64, 4).unwrap();
        Lpu::write_guest_u32(&mut regions, (desc_addr + 12) as u64, out_addr).unwrap();

        // scores: id0=100, id1=90, id2=80, id3=10
        Lpu::write_guest_u32(&mut regions, logits_addr as u64, 100).unwrap();
        Lpu::write_guest_u32(&mut regions, (logits_addr + 4) as u64, 90).unwrap();
        Lpu::write_guest_u32(&mut regions, (logits_addr + 8) as u64, 80).unwrap();
        Lpu::write_guest_u32(&mut regions, (logits_addr + 12) as u64, 10).unwrap();

        // p=0.5 keeps nucleus at top-2 for this distribution; seed=5 picks id0.
        write_u32(&mut lpu, REG_DECODE_TOP_P, 500);
        write_u32(&mut lpu, REG_DECODE_TEMPERATURE, 10_000);
        write_u32(&mut lpu, REG_DECODE_SEED, 5);
        write_u32(&mut lpu, REG_DESC_ADDR_LOW, desc_addr);
        write_u32(&mut lpu, REG_DESC_LEN, 1);
        write_u32(&mut lpu, REG_DESC_NOTIFY, 1);

        lpu.process_pending_descriptor_notify(&mut regions).unwrap();

        assert_eq!(
            Lpu::read_guest_u32(&mut regions, out_addr as u64).unwrap(),
            0
        );

        let snap = lpu.snapshot();
        assert_eq!(snap.tasks_done, 1);
        assert_eq!(snap.decode_candidates_evaluated, 4);
        assert_eq!(snap.decoded_tokens, 1);
        assert_eq!(snap.sampled_decodes, 1);
        assert_eq!(snap.nucleus_decodes, 1);
    }
}
