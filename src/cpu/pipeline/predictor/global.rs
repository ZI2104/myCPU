//! Global history predictor (gshare-style).
//!
//! Uses a single Global History Register (GHR) that records the outcomes
//! of ALL recent branches. The PHT is indexed by GHR XOR PC, allowing
//! the predictor to capture correlations between different branches.

use super::{hash_index, BranchPredictor, Prediction, PredictorStats, PredictorType};
use crate::types::Addr;

/// Global (gshare) predictor: GHR XOR PC indexes PHT of 2-bit counters.
#[derive(Debug, Clone)]
pub struct GlobalPredictor {
    /// Global History Register: records recent branch outcomes.
    ghr: u16,
    /// Number of history bits used.
    history_bits: usize,
    /// Pattern History Table: 2-bit counters.
    pht: Vec<u8>,
    predictions: u64,
    correct: u64,
    mispredictions: u64,
}

impl GlobalPredictor {
    pub fn new(table_size: usize, history_bits: usize) -> Self {
        Self {
            ghr: 0,
            history_bits,
            pht: vec![1; table_size], // Weakly Not Taken
            predictions: 0,
            correct: 0,
            mispredictions: 0,
        }
    }

    /// Compute PHT index: GHR XOR hash(PC).
    #[inline]
    fn pht_index(&self, pc: Addr) -> usize {
        let pc_idx = hash_index(pc) as u16;
        ((self.ghr ^ pc_idx) & ((1 << self.history_bits) as u16 - 1)) as usize
    }
}

impl BranchPredictor for GlobalPredictor {
    fn predict(&self, pc: Addr) -> Prediction {
        let idx = self.pht_index(pc);
        Prediction {
            taken: self.pht[idx] >= 2,
        }
    }

    fn update(&mut self, pc: Addr, taken: bool) {
        let idx = self.pht_index(pc);
        let predicted = self.pht[idx] >= 2;
        self.predictions += 1;
        if predicted == taken {
            self.correct += 1;
        } else {
            self.mispredictions += 1;
        }

        // Update PHT counter (saturating)
        if taken {
            self.pht[idx] = self.pht[idx].saturating_add(1).min(3);
        } else {
            self.pht[idx] = self.pht[idx].saturating_sub(1);
        }

        // Shift GHR: oldest bit drops, newest bit added
        self.ghr = ((self.ghr << 1) | (taken as u16)) & ((1 << self.history_bits) as u16 - 1);
    }

    fn reset(&mut self) {
        self.ghr = 0;
        for entry in &mut self.pht {
            *entry = 1;
        }
        self.predictions = 0;
        self.correct = 0;
        self.mispredictions = 0;
    }

    fn stats(&self) -> PredictorStats {
        PredictorStats {
            predictions: self.predictions,
            correct: self.correct,
            mispredictions: self.mispredictions,
        }
    }

    fn predictor_type(&self) -> PredictorType {
        PredictorType::Global
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Addr;

    #[test]
    fn test_global_initial_not_taken() {
        let p = GlobalPredictor::new(1024, 10);
        assert!(!p.predict(Addr::new(0x100)).taken);
    }

    #[test]
    fn test_global_ghr_updates() {
        let mut p = GlobalPredictor::new(1024, 4);
        assert_eq!(p.ghr, 0);
        p.update(Addr::new(0x100), true);
        assert_eq!(p.ghr, 0b0001);
        p.update(Addr::new(0x200), false);
        assert_eq!(p.ghr, 0b0010);
        p.update(Addr::new(0x100), true);
        assert_eq!(p.ghr, 0b0101);
    }

    #[test]
    fn test_global_xor_indexing() {
        let mut p = GlobalPredictor::new(1024, 4);
        let pc = Addr::new(0x100);
        // GHR=0, index = hash(pc) XOR 0
        let idx0 = p.pht_index(pc);
        // Update with taken, GHR becomes 1
        p.update(pc, true);
        // GHR=1, index = hash(pc) XOR 1 (different entry!)
        let idx1 = p.pht_index(pc);
        assert_ne!(idx0, idx1);
    }

    #[test]
    fn test_global_correlation() {
        let mut p = GlobalPredictor::new(1024, 10);
        // Branch A at 0x100, always taken
        // Branch B at 0x200, always not taken
        // After training, predictor should learn from global history
        for _ in 0..5 {
            p.update(Addr::new(0x100), true);
            p.update(Addr::new(0x200), false);
        }
        assert!(p.stats().correct > 0);
    }

    #[test]
    fn test_global_reset() {
        let mut p = GlobalPredictor::new(1024, 10);
        p.update(Addr::new(0x100), true);
        p.update(Addr::new(0x100), true);
        p.reset();
        assert_eq!(p.ghr, 0);
        assert!(!p.predict(Addr::new(0x100)).taken);
        assert_eq!(p.stats().predictions, 0);
    }
}
