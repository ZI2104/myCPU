//! 2-bit saturating counter predictor.
//!
//! Uses a 4-state FSM: Strongly Not Taken (0), Weakly Not Taken (1),
//! Weakly Taken (2), Strongly Taken (3).
//! Prediction is "taken" when counter >= 2.
//! Requires 2 consecutive mispredictions to change prediction direction.

use super::{hash_index, BranchPredictor, Prediction, PredictorStats, PredictorType};
use crate::types::Addr;

/// 2-bit predictor: each entry stores a counter 0-3.
#[derive(Debug, Clone)]
pub struct TwoBitPredictor {
    bht: Vec<u8>,
    predictions: u64,
    correct: u64,
    mispredictions: u64,
}

impl TwoBitPredictor {
    pub fn new(size: usize) -> Self {
        Self {
            bht: vec![1; size], // Initialize to Weakly Not Taken
            predictions: 0,
            correct: 0,
            mispredictions: 0,
        }
    }
}

impl BranchPredictor for TwoBitPredictor {
    fn predict(&self, pc: Addr) -> Prediction {
        Prediction {
            taken: self.bht[hash_index(pc)] >= 2,
        }
    }

    fn update(&mut self, pc: Addr, taken: bool) {
        let idx = hash_index(pc);
        let predicted = self.bht[idx] >= 2;
        self.predictions += 1;
        if predicted == taken {
            self.correct += 1;
        } else {
            self.mispredictions += 1;
        }
        // Saturating increment/decrement
        if taken {
            self.bht[idx] = self.bht[idx].saturating_add(1).min(3);
        } else {
            self.bht[idx] = self.bht[idx].saturating_sub(1);
        }
    }

    fn reset(&mut self) {
        for entry in &mut self.bht {
            *entry = 1; // Weakly Not Taken
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
        PredictorType::TwoBit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Addr;

    #[test]
    fn test_two_bit_initial_not_taken() {
        let p = TwoBitPredictor::new(1024);
        assert!(!p.predict(Addr::new(0x100)).taken);
    }

    #[test]
    fn test_two_bit_needs_two_updates_to_flip() {
        let mut p = TwoBitPredictor::new(1024);
        // Counter starts at 1 (Weakly Not Taken)
        // One "taken" → counter = 2 → predicted taken
        p.update(Addr::new(0x100), true);
        assert!(p.predict(Addr::new(0x100)).taken);
    }

    #[test]
    fn test_two_bit_saturating() {
        let mut p = TwoBitPredictor::new(1024);
        // Drive counter to max (3)
        p.update(Addr::new(0x100), true);
        p.update(Addr::new(0x100), true);
        p.update(Addr::new(0x100), true);
        // Additional taken should not overflow
        p.update(Addr::new(0x100), true);
        assert!(p.predict(Addr::new(0x100)).taken);
    }

    #[test]
    fn test_two_bit_resists_single_misprediction() {
        let mut p = TwoBitPredictor::new(1024);
        // Build strong "taken" prediction: counter = 3
        p.update(Addr::new(0x100), true);
        p.update(Addr::new(0x100), true);
        p.update(Addr::new(0x100), true);
        // Single not-taken: counter = 2, still predicts taken
        p.update(Addr::new(0x100), false);
        assert!(p.predict(Addr::new(0x100)).taken);
        // Second not-taken: counter = 1, now predicts not taken
        p.update(Addr::new(0x100), false);
        assert!(!p.predict(Addr::new(0x100)).taken);
    }

    #[test]
    fn test_two_bit_stats() {
        let mut p = TwoBitPredictor::new(1024);
        // Initial predict not taken, update taken → mispredict (counter 1→2)
        p.update(Addr::new(0x100), true);
        // Predict taken, actual taken → correct (counter 2→3)
        p.update(Addr::new(0x100), true);
        let stats = p.stats();
        assert_eq!(stats.predictions, 2);
        assert_eq!(stats.correct, 1);
        assert_eq!(stats.mispredictions, 1);
    }
}
