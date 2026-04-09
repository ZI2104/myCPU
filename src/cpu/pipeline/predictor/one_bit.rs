//! 1-bit branch predictor.
//!
//! Remembers the last outcome for each branch (indexed by PC).
//! Weakness: a loop branch mispredicts twice per loop iteration
//! (once on exit, once on re-entry).

use super::{hash_index, BranchPredictor, Prediction, PredictorStats, PredictorType};
use crate::types::Addr;

/// 1-bit predictor: each entry stores 0 (not taken) or 1 (taken).
#[derive(Debug, Clone)]
pub struct OneBitPredictor {
    bht: Vec<u8>,
    predictions: u64,
    correct: u64,
    mispredictions: u64,
}

impl OneBitPredictor {
    pub fn new(size: usize) -> Self {
        Self {
            bht: vec![0; size],
            predictions: 0,
            correct: 0,
            mispredictions: 0,
        }
    }
}

impl BranchPredictor for OneBitPredictor {
    fn predict(&self, pc: Addr) -> Prediction {
        Prediction {
            taken: self.bht[hash_index(pc)] == 1,
        }
    }

    fn update(&mut self, pc: Addr, taken: bool) {
        let idx = hash_index(pc);
        let predicted = self.bht[idx] == 1;
        self.predictions += 1;
        if predicted == taken {
            self.correct += 1;
        } else {
            self.mispredictions += 1;
        }
        self.bht[idx] = if taken { 1 } else { 0 };
    }

    fn reset(&mut self) {
        for entry in &mut self.bht {
            *entry = 0;
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
        PredictorType::OneBit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Addr;

    #[test]
    fn test_one_bit_initial_not_taken() {
        let p = OneBitPredictor::new(1024);
        assert!(!p.predict(Addr::new(0x100)).taken);
    }

    #[test]
    fn test_one_bit_update_to_taken() {
        let mut p = OneBitPredictor::new(1024);
        p.update(Addr::new(0x100), true);
        assert!(p.predict(Addr::new(0x100)).taken);
    }

    #[test]
    fn test_one_bit_toggle() {
        let mut p = OneBitPredictor::new(1024);
        p.update(Addr::new(0x100), true);
        assert!(p.predict(Addr::new(0x100)).taken);
        p.update(Addr::new(0x100), false);
        assert!(!p.predict(Addr::new(0x100)).taken);
    }

    #[test]
    fn test_one_bit_independent_entries() {
        let mut p = OneBitPredictor::new(1024);
        p.update(Addr::new(0x100), true);
        p.update(Addr::new(0x200), false);
        assert!(p.predict(Addr::new(0x100)).taken);
        assert!(!p.predict(Addr::new(0x200)).taken);
    }

    #[test]
    fn test_one_bit_stats() {
        let mut p = OneBitPredictor::new(1024);
        // First prediction for 0x100: predicted not taken, actual taken → miss
        p.update(Addr::new(0x100), true);
        // Second: predicted taken, actual taken → hit
        p.update(Addr::new(0x100), true);
        let stats = p.stats();
        assert_eq!(stats.predictions, 2);
        assert_eq!(stats.correct, 1);
        assert_eq!(stats.mispredictions, 1);
    }
}
