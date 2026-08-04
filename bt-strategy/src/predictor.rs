use bt_core::events::Bar;
use bt_core::types::{Side, Decimal};
use rust_decimal::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketRegime {
    StrongUptrend,
    MildUptrend,
    Rangebound,
    MildDowntrend,
    StrongDowntrend,
    HighVolatilityShock,
}

impl MarketRegime {
    pub fn description(&self) -> &'static str {
        match self {
            MarketRegime::StrongUptrend => "Strong Bullish Trend (Breakout Accumulation)",
            MarketRegime::MildUptrend => "Steady Bullish Drift",
            MarketRegime::Rangebound => "Rangebound / Consolidation Channel",
            MarketRegime::MildDowntrend => "Steady Bearish Distribution",
            MarketRegime::StrongDowntrend => "Strong Bearish Trend (Sell-off / Breakdown)",
            MarketRegime::HighVolatilityShock => "High Volatility Shock (Erratic / News Event)",
        }
    }

    pub fn recommended_strategy(&self) -> &'static str {
        match self {
            MarketRegime::StrongUptrend => "Momentum Breakout & Trailing Stops",
            MarketRegime::MildUptrend => "Moving Average Crossover & Trend Following",
            MarketRegime::Rangebound => "RSI & Bollinger Band Mean Reversion Scalping",
            MarketRegime::MildDowntrend => "Defensive Hedging / Short EMA Continually",
            MarketRegime::StrongDowntrend => "Short Momentum & Strict Cash Preservation",
            MarketRegime::HighVolatilityShock => "Reduce Sizing by 80% & Tight Envelopes",
        }
    }

    pub fn tag(&self) -> &'static str {
        match self {
            MarketRegime::StrongUptrend => "BULL BREAKOUT",
            MarketRegime::MildUptrend => "MILD UPTREND",
            MarketRegime::Rangebound => "RANGEBOUND",
            MarketRegime::MildDowntrend => "MILD DOWNTREND",
            MarketRegime::StrongDowntrend => "BEAR BREAKDOWN",
            MarketRegime::HighVolatilityShock => "VOLATILITY SHOCK",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionResult {
    pub symbol: String,
    pub current_price: Decimal,
    pub predicted_price_1bar: Decimal,
    pub predicted_price_5bar: Decimal,
    pub regime: MarketRegime,
    pub confidence_score: f64,
    pub trend_slope: f64,
    pub r2_score: f64,
    pub recommended_side: Option<Side>,
    pub optimal_kelly_size: f64,
    pub garch_volatility: f64,
    pub volume_momentum: f64,
    pub ensemble_factors: Vec<(String, f64)>,
}

pub struct TrendPredictor;

impl TrendPredictor {
    /// Analyzes historical bar sequences using Ordinary Least Squares (OLS) linear regression,
    /// volumetric velocity, and statistical volatility to predict next moves with confidence scoring.
    pub fn analyze(symbol: &str, bars: &[Bar]) -> Option<PredictionResult> {
        if bars.len() < 5 {
            return None;
        }

        let len = bars.len().min(30); // Use rolling window of up to 30 recent bars
        let window = &bars[bars.len() - len..];

        let closes: Vec<f64> = window
            .iter()
            .map(|b| b.close.to_f64().unwrap_or(0.0))
            .collect();
        let current_close = *closes.last().unwrap_or(&0.0);
        if current_close <= 0.0 {
            return None;
        }

        // Compute Ordinary Least Squares (OLS) Linear Regression: y = m*x + b
        let n = closes.len() as f64;
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_xx = 0.0;

        for (i, &y) in closes.iter().enumerate() {
            let x = i as f64;
            sum_x += x;
            sum_y += y;
            sum_xy += x * y;
            sum_xx += x * x;
        }

        let mean_x = sum_x / n;
        let mean_y = sum_y / n;

        let var_x = sum_xx - (sum_x * sum_x) / n;
        let cov_xy = sum_xy - (sum_x * sum_y) / n;

        let slope = if var_x != 0.0 { cov_xy / var_x } else { 0.0 };
        let intercept = mean_y - slope * mean_x;

        // Calculate R^2 coefficient of determination (Goodness of fit)
        let mut ss_tot = 0.0;
        let mut ss_res = 0.0;
        for (i, &y) in closes.iter().enumerate() {
            let pred = slope * (i as f64) + intercept;
            ss_tot += (y - mean_y).powi(2);
            ss_res += (y - pred).powi(2);
        }
        let r2_score = if ss_tot != 0.0 { (1.0 - (ss_res / ss_tot)).clamp(0.0, 1.0) } else { 0.0 };

        // Volatility assessment via standard deviation of normalized percentage returns
        let mut sum_return_sq = 0.0;
        for i in 1..closes.len() {
            let ret = (closes[i] - closes[i - 1]) / closes[i - 1];
            sum_return_sq += ret * ret;
        }
        let volatility = (sum_return_sq / (closes.len().saturating_sub(1)).max(1) as f64).sqrt();
        let pct_slope = (slope / mean_y) * 100.0; // Normalized percentage slope per bar

        // Classify Market Regime
        let regime = if volatility > 0.035 {
            MarketRegime::HighVolatilityShock
        } else if pct_slope > 0.45 {
            MarketRegime::StrongUptrend
        } else if pct_slope > 0.12 {
            MarketRegime::MildUptrend
        } else if pct_slope < -0.45 {
            MarketRegime::StrongDowntrend
        } else if pct_slope < -0.12 {
            MarketRegime::MildDowntrend
        } else {
            MarketRegime::Rangebound
        };

        // Price projections based on OLS line vector & momentum
        let target_1 = (slope * n + intercept).max(0.01);
        let target_5 = (slope * (n + 4.0) + intercept).max(0.01);

        let mut returns = Vec::new();
        for i in 1..window.len() {
            let ret = (closes[i] - closes[i - 1]) / closes[i - 1];
            returns.push(ret);
        }
        
        let mut garch_volatility = 0.0;
        if let Some(params) = crate::garch::GarchModel::fit(&returns) {
            let cond_var = crate::garch::GarchModel::conditional_variance(&params, &returns);
            garch_volatility = crate::garch::GarchModel::annualized_vol(cond_var);
        }

        let mut sum_ret_vol = 0.0;
        let mut sum_vol = 0.0;
        for i in 1..window.len() {
            let ret = (closes[i] - closes[i - 1]) / closes[i - 1];
            let vol = window[i].volume.to_f64().unwrap_or(0.0);
            sum_ret_vol += ret * vol;
            sum_vol += vol;
        }
        let volume_momentum = if sum_vol > 0.0 { sum_ret_vol / sum_vol } else { 0.0 };

        // Calculate Statistical Confidence Score (0.00 to 1.00)
        // High R2 means consistent trend; lower volatility boosts predictability
        let trend_strength = (pct_slope.abs() / 1.0).min(0.4);
        let vol_penalty = (volatility * 10.0).min(0.3);
        
        let vwm_alignment = if (volume_momentum > 0.0 && pct_slope > 0.0) || (volume_momentum < 0.0 && pct_slope < 0.0) { 0.1 } else { -0.1 };
        let garch_regime = if garch_volatility < 0.15 { 0.1 } else { -0.1 };
        
        let mut confidence = (r2_score * 0.5 + trend_strength - vol_penalty + vwm_alignment + garch_regime).clamp(0.15, 0.99);

        // In rangebound regimes with consistent oscillating volatility, mean-reversion predictability is high
        if regime == MarketRegime::Rangebound && volatility < 0.030 {
            confidence = (confidence + 0.45).min(0.92);
        }

        let mut ensemble_factors = Vec::new();
        ensemble_factors.push(("r2_score".to_string(), r2_score));
        ensemble_factors.push(("trend_strength".to_string(), trend_strength));
        ensemble_factors.push(("vol_penalty".to_string(), -vol_penalty));
        ensemble_factors.push(("garch_volatility".to_string(), garch_volatility));
        ensemble_factors.push(("volume_momentum".to_string(), volume_momentum));

        // Recommend side based on regime and confidence threshold
        let recommended_side = match regime {
            MarketRegime::StrongUptrend | MarketRegime::MildUptrend if confidence >= 0.60 => Some(Side::Buy),
            MarketRegime::StrongDowntrend | MarketRegime::MildDowntrend if confidence >= 0.60 => Some(Side::Sell),
            MarketRegime::Rangebound if confidence >= 0.60 => {
                // Mean reversion: if price is at or below linear mean, buy for scalp bounce; if above, sell
                if current_close <= mean_y { Some(Side::Buy) } else { Some(Side::Sell) }
            }
            _ => None,
        };

        // Calculate Optimal Position Size using Kelly Criterion (half-Kelly for safety)
        // Kelly f* = (b*p - q) / b, where p = win probability (confidence), q = 1 - p, b = win/loss win ratio (2:1 assumed)
        let win_prob = confidence;
        let loss_prob = 1.0 - win_prob;
        let reward_risk_ratio = 2.0;
        let raw_kelly = (reward_risk_ratio * win_prob - loss_prob) / reward_risk_ratio;
        let optimal_kelly_size = (raw_kelly * 0.5).clamp(0.01, 0.25); // Half-Kelly capped at 25% account equity

        Some(PredictionResult {
            symbol: symbol.to_string(),
            current_price: Decimal::from_f64_retain(current_close).unwrap_or_default().round_dp(2),
            predicted_price_1bar: Decimal::from_f64_retain(target_1).unwrap_or_default().round_dp(2),
            predicted_price_5bar: Decimal::from_f64_retain(target_5).unwrap_or_default().round_dp(2),
            regime,
            confidence_score: confidence,
            trend_slope: pct_slope,
            r2_score,
            recommended_side,
            optimal_kelly_size,
            garch_volatility,
            volume_momentum,
            ensemble_factors,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn build_bar(close_price: f64) -> Bar {
        Bar {
            timestamp: Utc::now(),
            symbol: bt_core::types::Symbol::new(bt_core::types::Venue::Nse, "RELIANCE", bt_core::types::AssetClass::Equity),
            timeframe: bt_core::events::Timeframe::Minute5,
            venue: bt_core::types::Venue::Nse,
            open: Decimal::from_f64_retain(close_price).unwrap(),
            high: Decimal::from_f64_retain(close_price + 1.0).unwrap(),
            low: Decimal::from_f64_retain(close_price - 1.0).unwrap(),
            close: Decimal::from_f64_retain(close_price).unwrap(),
            volume: Decimal::new(1000, 0),
            vwap: None,
            trade_count: None,
        }
    }

    #[test]
    fn test_bull_trend_prediction() {
        let mut bars = Vec::new();
        let mut p = 100.0;
        for _ in 0..20 {
            bars.push(build_bar(p));
            p += 2.0; // Strong upward progression of 2% per bar
        }

        let pred = TrendPredictor::analyze("NSE:RELIANCE", &bars).expect("Prediction failed");
        assert_eq!(pred.regime, MarketRegime::StrongUptrend);
        assert!(pred.predicted_price_1bar > pred.current_price);
        assert!(pred.predicted_price_5bar > pred.predicted_price_1bar);
        assert!(pred.confidence_score > 0.70);
        assert_eq!(pred.recommended_side, Some(Side::Buy));
    }

    #[test]
    fn test_rangebound_prediction() {
        let mut bars = Vec::new();
        let prices = [100.0, 100.5, 99.8, 100.2, 99.9, 100.1, 100.3, 99.7, 100.0, 100.2];
        for &p in &prices {
            bars.push(build_bar(p));
        }

        let pred = TrendPredictor::analyze("BSE:TCS", &bars).expect("Prediction failed");
        assert_eq!(pred.regime, MarketRegime::Rangebound);
        assert!(pred.r2_score < 0.50); // Little directional trend fit in flat range
    }
}
