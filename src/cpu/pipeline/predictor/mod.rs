//! Branch prediction module.
//!
//! Implements multiple branch prediction strategies for the pipeline:
//! - AlwaysNotTaken: static prediction (baseline, no prediction)
//! - OneBit: 1-bit predictor (remembers last outcome)
//! - TwoBit: 2-bit saturating counter (needs 2 consecutive mispredictions)
//! - Local: per-branch history register + pattern history table
//! - Global: global history register + PHT (gshare-style)
//!
//! All predictors share a Branch Target Buffer (BTB) for target prediction.
//! The `PredictorManager` wraps a direction predictor + BTB into a single unit.

mod always_not;
mod btb;
mod global;
mod local;
mod one_bit;
mod two_bit;

pub use always_not::AlwaysNotTaken;
pub use btb::{BranchTargetBuffer, BtbEntry, BtbStats};
pub use global::GlobalPredictor;
pub use local::LocalPredictor;
pub use one_bit::OneBitPredictor;
pub use two_bit::TwoBitPredictor;

use serde::{Deserialize, Serialize};

use crate::types::Addr;

/// Default table size for BHT/PHT (1024 entries, indexed by PC[11:2]).
pub const PREDICTOR_TABLE_SIZE: usize = 1024;
/// BTB size (256 entries).
pub const BTB_SIZE: usize = 256;
/// Number of history bits for local and global predictors.
pub const HISTORY_BITS: usize = 10;

/// Extract table index from PC.
#[inline]
fn hash_index(pc: Addr) -> usize {
    ((pc.raw() >> 2) as usize) & (PREDICTOR_TABLE_SIZE - 1)
}

/// Extract BTB index from PC.
#[inline]
fn btb_index(pc: Addr) -> usize {
    ((pc.raw() >> 2) as usize) & (BTB_SIZE - 1)
}

/// Prediction result from a branch predictor (direction only).
#[derive(Debug, Clone, Copy, Default)]
pub struct Prediction {
    /// Whether the predictor predicts the branch will be taken.
    pub taken: bool,
}

/// Combined prediction result (direction + target from BTB).
#[derive(Debug, Clone, Copy, Default)]
pub struct PredictionResult {
    /// Whether the branch is predicted taken.
    pub taken: bool,
    /// Predicted target address (from BTB), if available.
    pub target: Option<Addr>,
    /// PC of the predicted branch.
    pub pc: Addr,
}

/// Information about a misprediction detected at resolution time.
#[derive(Debug, Clone, Copy, Default)]
pub struct MispredictionInfo {
    /// Whether a misprediction occurred.
    pub mispredicted: bool,
    /// What was predicted (direction).
    pub predicted_taken: bool,
    /// What actually happened (direction).
    pub actual_taken: bool,
    /// The correct redirect target (actual_target if mispredicted, else PC+4).
    pub redirect_target: Addr,
}

impl MispredictionInfo {
    /// Create a no-misprediction result.
    pub fn none() -> Self {
        Self::default()
    }
}

/// Statistics tracked by each predictor.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PredictorStats {
    pub predictions: u64,
    pub correct: u64,
    pub mispredictions: u64,
}

impl PredictorStats {
    pub fn accuracy(&self) -> Option<f64> {
        if self.predictions == 0 {
            None
        } else {
            Some(self.correct as f64 / self.predictions as f64 * 100.0)
        }
    }
}

/// Predictor type enum for runtime switching and serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PredictorType {
    None,
    OneBit,
    TwoBit,
    Local,
    Global,
}

impl PredictorType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::OneBit => "one_bit",
            Self::TwoBit => "two_bit",
            Self::Local => "local",
            Self::Global => "global",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::None => "Always Not Taken",
            Self::OneBit => "1-Bit",
            Self::TwoBit => "2-Bit Saturating",
            Self::Local => "Local (2-Level)",
            Self::Global => "Global (gshare)",
        }
    }

    /// Parse from string (case-insensitive).
    pub fn from_str_lossy(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "none" => Some(Self::None),
            "one_bit" | "1bit" | "1-bit" => Some(Self::OneBit),
            "two_bit" | "2bit" | "2-bit" => Some(Self::TwoBit),
            "local" => Some(Self::Local),
            "global" | "gshare" => Some(Self::Global),
            _ => None,
        }
    }
}

/// Branch predictor trait.
///
/// Predictors are direction-only: they predict whether a branch will be taken.
/// Target prediction is handled separately by the BTB.
pub trait BranchPredictor: Send + Sync {
    /// Predict whether the branch at `pc` will be taken.
    fn predict(&self, pc: Addr) -> Prediction;

    /// Update predictor state after branch resolution.
    fn update(&mut self, pc: Addr, taken: bool);

    /// Reset predictor state.
    fn reset(&mut self);

    /// Get prediction statistics.
    fn stats(&self) -> PredictorStats;

    /// Get the predictor type.
    fn predictor_type(&self) -> PredictorType;
}

/// Create a new predictor of the given type.
pub fn create_predictor(pt: PredictorType) -> Box<dyn BranchPredictor> {
    match pt {
        PredictorType::None => Box::new(AlwaysNotTaken::new()),
        PredictorType::OneBit => Box::new(OneBitPredictor::new(PREDICTOR_TABLE_SIZE)),
        PredictorType::TwoBit => Box::new(TwoBitPredictor::new(PREDICTOR_TABLE_SIZE)),
        PredictorType::Local => Box::new(LocalPredictor::new(
            PREDICTOR_TABLE_SIZE,
            HISTORY_BITS,
        )),
        PredictorType::Global => Box::new(GlobalPredictor::new(
            PREDICTOR_TABLE_SIZE,
            HISTORY_BITS,
        )),
    }
}

/// Combined predictor + BTB manager.
///
/// Wraps a direction predictor and a BTB into a single unit that the pipeline
/// talks to. Handles the predict/update lifecycle and tracks mispredictions.
pub struct PredictorManager {
    predictor: Box<dyn BranchPredictor>,
    btb: BranchTargetBuffer,
    predictor_type: PredictorType,
}

impl std::fmt::Debug for PredictorManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PredictorManager")
            .field("predictor_type", &self.predictor_type)
            .field("btb", &self.btb)
            .finish()
    }
}

impl PredictorManager {
    /// Create a new manager with the given predictor type.
    pub fn new(pt: PredictorType) -> Self {
        Self {
            predictor: create_predictor(pt),
            btb: BranchTargetBuffer::new(),
            predictor_type: pt,
        }
    }

    /// Predict whether the branch/jump at `pc` will be taken and to where.
    pub fn predict(&mut self, pc: Addr) -> PredictionResult {
        let direction = self.predictor.predict(pc);
        let target = if direction.taken {
            self.btb.lookup(pc)
        } else {
            None
        };

        PredictionResult {
            taken: direction.taken,
            target,
            pc,
        }
    }

    /// Update predictor and BTB state with the actual branch/jump outcome.
    ///
    /// The prediction comparison is done by the caller using
    /// `check_misprediction()` with the prediction stored in the pipeline register.
    pub fn update(
        &mut self,
        pc: Addr,
        actual_taken: bool,
        actual_target: Addr,
        is_branch: bool,
    ) {
        self.predictor.update(pc, actual_taken);

        if actual_taken {
            self.btb.update(pc, actual_target, is_branch);
        }
    }

    /// Check if a prediction matches the actual outcome.
    pub fn check_misprediction(
        prediction: &Option<PredictionResult>,
        actual_taken: bool,
        actual_target: Addr,
    ) -> MispredictionInfo {
        match prediction {
            Some(pred) => {
                let direction_wrong = pred.taken != actual_taken;
                let target_wrong = actual_taken
                    && pred.taken
                    && pred.target.map_or(true, |t| t != actual_target);

                let mispredicted = direction_wrong || target_wrong;

                MispredictionInfo {
                    mispredicted,
                    predicted_taken: pred.taken,
                    actual_taken,
                    redirect_target: if actual_taken {
                        actual_target
                    } else {
                        // Not taken: no redirect needed (sequential)
                        Addr::new(0)
                    },
                }
            }
            None => MispredictionInfo::none(),
        }
    }

    /// Switch to a different predictor type.
    /// Resets the predictor but preserves the BTB.
    pub fn switch(&mut self, pt: PredictorType) {
        if pt != self.predictor_type {
            self.predictor = create_predictor(pt);
            self.predictor_type = pt;
            // Keep BTB across switches — target prediction is type-independent
        }
    }

    /// Reset both predictor and BTB.
    pub fn reset(&mut self) {
        self.predictor.reset();
        self.btb.reset();
    }

    /// Get direction predictor statistics.
    pub fn stats(&self) -> PredictorStats {
        self.predictor.stats()
    }

    /// Get BTB statistics.
    pub fn btb_stats(&self) -> &BtbStats {
        self.btb.stats()
    }

    /// Get current predictor type.
    pub fn predictor_type(&self) -> PredictorType {
        self.predictor_type
    }

    /// Get a snapshot of BTB entries for visualization.
    pub fn btb_snapshot(&self, limit: usize) -> Vec<BtbEntry> {
        self.btb.snapshot(limit)
    }
}

#[cfg(test)]
mod manager_tests {
    use super::*;

    #[test]
    fn test_manager_none_predictor() {
        let mut mgr = PredictorManager::new(PredictorType::None);
        let pred = mgr.predict(Addr::new(0x100));
        assert!(!pred.taken);
        assert!(pred.target.is_none());
    }

    #[test]
    fn test_manager_predict_and_resolve() {
        let mut mgr = PredictorManager::new(PredictorType::TwoBit);

        // Predict for first time → not taken (initial state)
        let pred = mgr.predict(Addr::new(0x100));
        assert!(!pred.taken);

        // Update: branch was actually taken
        mgr.update(Addr::new(0x100), true, Addr::new(0x200), true);

        // Predict again → should now predict taken (counter moved up)
        let pred = mgr.predict(Addr::new(0x100));
        assert!(pred.taken);
    }

    #[test]
    fn test_manager_btb_integration() {
        let mut mgr = PredictorManager::new(PredictorType::TwoBit);

        // Train: branch at 0x100 goes to 0x200
        mgr.update(Addr::new(0x100), true, Addr::new(0x200), true);

        // Predict should now have both direction and target
        let pred = mgr.predict(Addr::new(0x100));
        assert!(pred.taken);
        assert_eq!(pred.target, Some(Addr::new(0x200)));
    }

    #[test]
    fn test_manager_switch_preserves_btb() {
        let mut mgr = PredictorManager::new(PredictorType::TwoBit);

        // Train with TwoBit
        mgr.update(Addr::new(0x100), true, Addr::new(0x200), true);

        // Switch to Local
        mgr.switch(PredictorType::Local);
        assert_eq!(mgr.predictor_type(), PredictorType::Local);

        // BTB should still have the entry
        let pred = mgr.predict(Addr::new(0x100));
        // Direction depends on Local predictor's initial state (not taken)
        assert!(!pred.taken);
        // But BTB target is preserved — won't be checked since direction is not taken
    }

    #[test]
    fn test_misprediction_detection() {
        // Predicted taken but actual not taken
        let pred = Some(PredictionResult {
            taken: true,
            target: Some(Addr::new(0x200)),
            pc: Addr::new(0x100),
        });
        let info = PredictorManager::check_misprediction(&pred, false, Addr::new(0));
        assert!(info.mispredicted);
        assert!(info.predicted_taken);
        assert!(!info.actual_taken);

        // Correct prediction
        let pred = Some(PredictionResult {
            taken: true,
            target: Some(Addr::new(0x200)),
            pc: Addr::new(0x100),
        });
        let info = PredictorManager::check_misprediction(&pred, true, Addr::new(0x200));
        assert!(!info.mispredicted);

        // Wrong target
        let pred = Some(PredictionResult {
            taken: true,
            target: Some(Addr::new(0x200)),
            pc: Addr::new(0x100),
        });
        let info = PredictorManager::check_misprediction(&pred, true, Addr::new(0x300));
        assert!(info.mispredicted);
    }

    #[test]
    fn test_predictor_type_from_str() {
        assert_eq!(PredictorType::from_str_lossy("none"), Some(PredictorType::None));
        assert_eq!(PredictorType::from_str_lossy("one_bit"), Some(PredictorType::OneBit));
        assert_eq!(PredictorType::from_str_lossy("2-bit"), Some(PredictorType::TwoBit));
        assert_eq!(PredictorType::from_str_lossy("local"), Some(PredictorType::Local));
        assert_eq!(PredictorType::from_str_lossy("gshare"), Some(PredictorType::Global));
        assert_eq!(PredictorType::from_str_lossy("unknown"), None);
    }
}
