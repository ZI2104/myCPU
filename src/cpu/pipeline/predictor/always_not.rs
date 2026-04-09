//! Always-not-taken predictor (baseline, no prediction).

use serde::{Deserialize, Serialize};

use super::{BranchPredictor, Prediction, PredictorStats, PredictorType};
use crate::types::Addr;

/// Always predicts "not taken". This is the baseline with no prediction.
#[derive(Debug, Clone, Default)]
pub struct AlwaysNotTaken {
    stats: PredictorStatsCore,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PredictorStatsCore {
    predictions: u64,
    correct: u64,
    mispredictions: u64,
}

impl AlwaysNotTaken {
    pub fn new() -> Self {
        Self::default()
    }
}

impl BranchPredictor for AlwaysNotTaken {
    fn predict(&self, _pc: Addr) -> Prediction {
        Prediction { taken: false }
    }

    fn update(&mut self, _pc: Addr, _taken: bool) {
        // No state to update
    }

    fn reset(&mut self) {
        self.stats = PredictorStatsCore::default();
    }

    fn stats(&self) -> PredictorStats {
        PredictorStats {
            predictions: self.stats.predictions,
            correct: self.stats.correct,
            mispredictions: self.stats.mispredictions,
        }
    }

    fn predictor_type(&self) -> PredictorType {
        PredictorType::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Addr;

    #[test]
    fn test_always_not_taken_predicts_false() {
        let p = AlwaysNotTaken::new();
        assert!(!p.predict(Addr::new(0x100)).taken);
        assert!(!p.predict(Addr::new(0x200)).taken);
    }

    #[test]
    fn test_always_not_taken_no_state_change() {
        let mut p = AlwaysNotTaken::new();
        p.update(Addr::new(0x100), true);
        p.update(Addr::new(0x100), false);
        assert!(!p.predict(Addr::new(0x100)).taken);
    }
}
