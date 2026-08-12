/// Continuous Automated Greeks Hedger.
/// Maintains a Delta-neutral portfolio by issuing micro-hedge orders
/// in the underlying spot market whenever the options portfolio delta drifts.
pub struct AutoHedger {
    /// The maximum allowable net Delta before a hedge order is fired.
    pub delta_threshold: f64,
    /// The maximum allowable net Gamma before a hedge order is fired.
    pub gamma_threshold: f64,
    /// The maximum allowable net Vega before a hedge order is fired.
    pub vega_threshold: f64,
    
    // In a real system, this would maintain a connection to the execution engine.
    // For now, we simulate firing hedge orders by returning them.
}

#[derive(Debug, Clone, PartialEq)]
pub enum HedgeAction {
    /// Buy/Sell shares of the underlying asset to flatten Delta.
    SpotHedge { symbol: String, size: f64 },
    /// Requires trading other options to flatten Gamma or Vega (more complex).
    OptionsHedge { symbol: String, target_gamma: f64, target_vega: f64 },
    /// Portfolio is within risk limits.
    None,
}

impl AutoHedger {
    pub fn new(delta_threshold: f64, gamma_threshold: f64, vega_threshold: f64) -> Self {
        Self {
            delta_threshold,
            gamma_threshold,
            vega_threshold,
        }
    }

    /// Evaluates the portfolio's current net Greeks and fires hedge orders if thresholds are breached.
    /// Returns a list of required hedge actions.
    pub fn evaluate_portfolio_greeks(
        &self,
        symbol: &str,
        net_delta: f64,
        net_gamma: f64,
        net_vega: f64,
    ) -> Vec<HedgeAction> {
        let mut actions = Vec::new();

        // 1. Delta Hedging (First order risk)
        // If net delta is > 500, we are long 500 shares equivalent. We need to SELL 500 shares of spot.
        // If net delta is < -500, we are short 500 shares equivalent. We need to BUY 500 shares of spot.
        if net_delta.abs() > self.delta_threshold {
            actions.push(HedgeAction::SpotHedge {
                symbol: symbol.to_string(),
                size: -net_delta, // Inverse of current delta to flatten it
            });
        }

        // 2. Gamma / Vega Hedging (Second order risk)
        // Gamma and Vega cannot be hedged with spot (spot has 0 Gamma and 0 Vega).
        // They require buying/selling other options.
        if net_gamma.abs() > self.gamma_threshold || net_vega.abs() > self.vega_threshold {
            actions.push(HedgeAction::OptionsHedge {
                symbol: symbol.to_string(),
                target_gamma: -net_gamma,
                target_vega: -net_vega,
            });
        }

        if actions.is_empty() {
            actions.push(HedgeAction::None);
        }

        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delta_hedger_fires_spot_order() {
        let hedger = AutoHedger::new(100.0, 50.0, 50.0);
        
        // Portfolio is long 150 deltas (breaches 100 threshold)
        let actions = hedger.evaluate_portfolio_greeks("AAPL", 150.0, 10.0, 10.0);
        
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], HedgeAction::SpotHedge { symbol: "AAPL".to_string(), size: -150.0 });
    }

    #[test]
    fn test_gamma_vega_hedger_fires_options_order() {
        let hedger = AutoHedger::new(100.0, 50.0, 50.0);
        
        // Portfolio is short 60 Gamma (breaches 50 threshold)
        let actions = hedger.evaluate_portfolio_greeks("MSFT", 0.0, -60.0, 10.0);
        
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], HedgeAction::OptionsHedge { symbol: "MSFT".to_string(), target_gamma: 60.0, target_vega: -10.0 });
    }

    #[test]
    fn test_portfolio_within_limits() {
        let hedger = AutoHedger::new(100.0, 50.0, 50.0);
        
        // Everything within thresholds
        let actions = hedger.evaluate_portfolio_greeks("GOOG", 50.0, 20.0, 30.0);
        
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], HedgeAction::None);
    }
}
