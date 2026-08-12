use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketRegime {
    LowVolBull,
    ModerateTrend,
    HighVolBear,
    CrashMode,
}

#[derive(Debug)]
pub struct RegimeDetector {
    current_regime: MarketRegime,
}

impl Default for RegimeDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl RegimeDetector {
    pub fn new() -> Self {
        Self {
            current_regime: MarketRegime::ModerateTrend,
        }
    }

    /// Classifies the market regime based on the CBOE VIX index (or a proxy).
    pub fn classify_vix(&mut self, vix_value: f64) -> MarketRegime {
        self.current_regime = if vix_value < 15.0 {
            MarketRegime::LowVolBull
        } else if (15.0..25.0).contains(&vix_value) {
            MarketRegime::ModerateTrend
        } else if (25.0..40.0).contains(&vix_value) {
            MarketRegime::HighVolBear
        } else {
            MarketRegime::CrashMode
        };
        self.current_regime
    }

    /// Returns the active market regime.
    pub fn current_regime(&self) -> MarketRegime {
        self.current_regime
    }

    /// Returns the recommended position size scaling factor (0.0 to 1.0)
    /// based on the current regime to mathematically minimize tail-risk exposure.
    pub fn position_sizing_multiplier(&self) -> f64 {
        match self.current_regime {
            MarketRegime::LowVolBull => 1.0,        // 100% size
            MarketRegime::ModerateTrend => 0.8,     // 80% size
            MarketRegime::HighVolBear => 0.4,       // 40% size
            MarketRegime::CrashMode => 0.1,         // 10% size (Extreme defense)
        }
    }
}
