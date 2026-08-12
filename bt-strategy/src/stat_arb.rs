use serde::{Deserialize, Serialize};

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatArbSignal {
    /// Spread > +2.0 SD: Short Leg A / Long Leg B
    ShortA_LongB,
    /// Spread < -2.0 SD: Long Leg A / Short Leg B
    LongA_ShortB,
    /// |Z-Score| <= 0.5: Mean Reversion achieved (Close positions for profit)
    MeanRevertedExit,
    /// |Z-Score| > 4.0: Structural decoupling (Emergency Stop Loss)
    StopLossExit,
    /// Spread within equilibrium (-2.0 <= Z <= +2.0)
    Neutral,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatArbResult {
    pub symbol_a: String,
    pub symbol_b: String,
    pub price_a: f64,
    pub price_b: f64,
    pub hedge_ratio: f64,
    pub current_spread: f64,
    pub mean_spread: f64,
    pub std_dev_spread: f64,
    pub z_score: f64,
    pub signal: StatArbSignal,
    pub confidence: f64,
}

pub struct PairsArbitrageEngine;

impl PairsArbitrageEngine {
    /// Evaluates statistical cointegration and Z-Score between two historical price series
    /// Price series A and B must be of equal length (at least 5 sample intervals)
    pub fn analyze(
        symbol_a: &str,
        symbol_b: &str,
        prices_a: &[f64],
        prices_b: &[f64],
    ) -> Option<StatArbResult> {
        let n = prices_a.len().min(prices_b.len());
        if n < 5 {
            return None;
        }
        
        let pa = &prices_a[..n];
        let pb = &prices_b[..n];

        // 1. Calculate OLS Hedge Ratio (Beta): Price_A = alpha + beta * Price_B
        let sum_b: f64 = pb.iter().sum();
        let sum_a: f64 = pa.iter().sum();
        let mean_b = sum_b / n as f64;
        let mean_a = sum_a / n as f64;

        let mut cov_ab = 0.0;
        let mut var_b = 0.0;
        for i in 0..n {
            let db = pb[i] - mean_b;
            let da = pa[i] - mean_a;
            cov_ab += db * da;
            var_b += db * db;
        }

        let hedge_ratio = if var_b > 1e-9 { cov_ab / var_b } else { 1.0 };

        // 2. Compute historical spread series: S = Price_A - (Beta * Price_B)
        let mut spreads = Vec::with_capacity(n);
        for i in 0..n {
            let spread = pa[i] - (hedge_ratio * pb[i]);
            spreads.push(spread);
        }

        // 3. Compute rolling mean and standard deviation of the spread
        let spread_sum: f64 = spreads.iter().sum();
        let mean_spread = spread_sum / n as f64;

        let mut var_spread = 0.0;
        for s in &spreads {
            let ds = *s - mean_spread;
            var_spread += ds * ds;
        }
        let std_dev_spread = (var_spread / n as f64).sqrt().max(1e-6);

        // 4. Compute current Z-Score from latest prices
        let current_price_a = pa[n - 1];
        let current_price_b = pb[n - 1];
        let current_spread = current_price_a - (hedge_ratio * current_price_b);
        let z_score = (current_spread - mean_spread) / std_dev_spread;

        // 5. Determine trade signal based on Z-Score thresholds
        let signal = if !(-4.0..=4.0).contains(&z_score) {
            StatArbSignal::StopLossExit // Structural divergence anomaly
        } else if z_score > 2.0 {
            StatArbSignal::ShortA_LongB // Leg A overvalued relative to Leg B
        } else if z_score < -2.0 {
            StatArbSignal::LongA_ShortB // Leg A undervalued relative to Leg B
        } else if z_score.abs() <= 0.5 {
            StatArbSignal::MeanRevertedExit // Take profit on convergence
        } else {
            StatArbSignal::Neutral // Within normal oscillations
        };

        // 6. Calculate cointegration confidence score (R² equivalent for spread predictability)
        let confidence = (cov_ab * cov_ab / (var_b * var_b * n as f64)).clamp(0.40, 0.99);

        Some(StatArbResult {
            symbol_a: symbol_a.to_string(),
            symbol_b: symbol_b.to_string(),
            price_a: current_price_a,
            price_b: current_price_b,
            hedge_ratio,
            current_spread,
            mean_spread,
            std_dev_spread,
            z_score,
            signal,
            confidence,
        })
    }

    /// Performs Walk-Forward Out-of-Sample (OOS) Cointegration Validation:
    /// Splits historical series into an In-Sample (IS) training window (default 80%) to derive the OLS hedge ratio & mean spread,
    /// and tests whether spread stationarity holds across the remaining Out-of-Sample (OOS) testing window (20%).
    /// Prevents structural overfitting and spurious parameter calibration in pairs trading.
    pub fn analyze_walk_forward(
        symbol_a: &str,
        symbol_b: &str,
        prices_a: &[f64],
        prices_b: &[f64],
    ) -> Option<StatArbResult> {
        let n = prices_a.len().min(prices_b.len());
        if n < 10 {
            // Need sufficient sample observations for meaningful IS/OOS split
            return Self::analyze(symbol_a, symbol_b, prices_a, prices_b);
        }

        // Split 80% In-Sample (IS) calibration, 20% Out-of-Sample (OOS) verification
        let is_len = (n * 8) / 10;
        let is_pa = &prices_a[..is_len];
        let is_pb = &prices_b[..is_len];

        // 1. Calculate OLS Hedge Ratio strictly from In-Sample window
        let sum_b: f64 = is_pb.iter().sum();
        let sum_a: f64 = is_pa.iter().sum();
        let mean_b = sum_b / is_len as f64;
        let mean_a = sum_a / is_len as f64;

        let mut cov_ab = 0.0;
        let mut var_b = 0.0;
        for i in 0..is_len {
            let db = is_pb[i] - mean_b;
            let da = is_pa[i] - mean_a;
            cov_ab += db * da;
            var_b += db * db;
        }
        let hedge_ratio = if var_b > 1e-9 { cov_ab / var_b } else { 1.0 };

        // 2. Compute spread mean and standard deviation strictly over In-Sample window
        let mut is_spreads = Vec::with_capacity(is_len);
        for i in 0..is_len {
            is_spreads.push(is_pa[i] - (hedge_ratio * is_pb[i]));
        }
        let mean_spread = is_spreads.iter().sum::<f64>() / is_len as f64;
        let mut var_spread = 0.0;
        for s in &is_spreads {
            let ds = *s - mean_spread;
            var_spread += ds * ds;
        }
        let std_dev_spread = (var_spread / is_len as f64).sqrt().max(1e-6);

        // 3. Out-of-Sample (OOS) Stationarity Validation over remaining 20% window
        // Verify that OOS spread variance does not degrade significantly (> 3x IS variance)
        let oos_len = n - is_len;
        let mut oos_var_spread = 0.0;
        for i in is_len..n {
            let spread = prices_a[i] - (hedge_ratio * prices_b[i]);
            let ds = spread - mean_spread;
            oos_var_spread += ds * ds;
        }
        let oos_std_dev = (oos_var_spread / oos_len as f64).sqrt().max(1e-6);

        // 4. Compute current Z-Score from latest OOS data tick
        let current_price_a = prices_a[n - 1];
        let current_price_b = prices_b[n - 1];
        let current_spread = current_price_a - (hedge_ratio * current_price_b);
        let z_score = (current_spread - mean_spread) / std_dev_spread;

        // If OOS variance expanded by >3.5x over IS variance, structural regime break occurred: force StopLossExit / Neutral
        let signal = if oos_std_dev > std_dev_spread * 3.5 || !(-4.0..=4.0).contains(&z_score) {
            StatArbSignal::StopLossExit
        } else if z_score > 2.0 {
            StatArbSignal::ShortA_LongB
        } else if z_score < -2.0 {
            StatArbSignal::LongA_ShortB
        } else if z_score.abs() <= 0.5 {
            StatArbSignal::MeanRevertedExit
        } else {
            StatArbSignal::Neutral
        };

        let confidence = (cov_ab * cov_ab / (var_b * var_b * is_len as f64)).clamp(0.40, 0.99);

        Some(StatArbResult {
            symbol_a: symbol_a.to_string(),
            symbol_b: symbol_b.to_string(),
            price_a: current_price_a,
            price_b: current_price_b,
            hedge_ratio,
            current_spread,
            mean_spread,
            std_dev_spread,
            z_score,
            signal,
            confidence,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pairs_arbitrage_engine_divergence_and_reversion() {
        // Construct simulated cointegrated price series where Leg A suddenly spikes upwards
        let mut prices_a = vec![100.0, 101.0, 102.0, 101.5, 100.5, 101.2, 100.8, 101.0, 101.1, 108.0]; // Spike at end
        let prices_b = vec![10.0, 10.1, 10.2, 10.15, 10.05, 10.12, 10.08, 10.10, 10.11, 10.10];

        let res = PairsArbitrageEngine::analyze("NSE:HDFCBANK", "NSE:ICICIBANK", &prices_a, &prices_b).unwrap();
        
        assert_eq!(res.symbol_a, "NSE:HDFCBANK");
        assert_eq!(res.symbol_b, "NSE:ICICIBANK");
        // Because of the spike in Leg A, Z-Score should be significantly above +2.0
        assert!(res.z_score > 2.0, "Z-Score was {:.2}, expected > 2.0", res.z_score);
        assert_eq!(res.signal, StatArbSignal::ShortA_LongB);

        // Now test mean reversion convergence
        let len = prices_a.len();
        prices_a[len - 1] = 101.0; // Return to equilibrium price
        let res_conv = PairsArbitrageEngine::analyze("NSE:HDFCBANK", "NSE:ICICIBANK", &prices_a, &prices_b).unwrap();
        assert!(res_conv.z_score.abs() <= 0.5);
        assert_eq!(res_conv.signal, StatArbSignal::MeanRevertedExit);
    }

    #[test]
    fn test_walk_forward_oos_validation() {
        let prices_a = vec![100.0, 101.0, 102.0, 101.5, 100.5, 101.2, 100.8, 101.0, 101.1, 108.0];
        let prices_b = vec![10.0, 10.1, 10.2, 10.15, 10.05, 10.12, 10.08, 10.10, 10.11, 10.10];
        let oos_res = PairsArbitrageEngine::analyze_walk_forward("NSE:HDFCBANK", "NSE:ICICIBANK", &prices_a, &prices_b).unwrap();
        assert_eq!(oos_res.symbol_a, "NSE:HDFCBANK");
        assert!(oos_res.z_score > 0.0);
    }
}
