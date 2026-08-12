/// Heston Stochastic Volatility Model for pricing Variance Swaps.
/// In Heston, variance follows a Cox-Ingersoll-Ross (CIR) process:
/// dV_t = kappa * (theta - V_t) dt + xi * sqrt(V_t) dW_t
#[derive(Debug, Clone, Copy)]
pub struct HestonModel {
    /// Initial variance V_0
    pub v0: f64,
    /// Long-term mean variance (theta)
    pub theta: f64,
    /// Rate of mean reversion (kappa)
    pub kappa: f64,
    /// Volatility of volatility (xi)
    pub xi: f64,
    /// Correlation between spot and variance Brownian motions (rho)
    pub rho: f64,
}

impl HestonModel {
    pub fn new(v0: f64, theta: f64, kappa: f64, xi: f64, rho: f64) -> Result<Self, String> {
        if v0 < 0.0 { return Err("v0 must be >= 0".to_string()); }
        if theta < 0.0 { return Err("theta must be >= 0".to_string()); }
        if kappa <= 0.0 { return Err("kappa must be > 0".to_string()); }
        if xi < 0.0 { return Err("xi must be >= 0".to_string()); }
        if !(-1.0..=1.0).contains(&rho) { return Err("rho must be between -1 and 1".to_string()); }

        // Feller condition: 2 * kappa * theta >= xi^2 ensures variance never hits exactly 0.
        // It's a nice-to-have but not strictly required for pricing var swaps, 
        // though we check it for model validity.
        if 2.0 * kappa * theta < xi * xi {
            // We just warn or allow it depending on implementation, 
            // but for a strict institutional model, let's enforce it.
            // Actually, in practice, Feller is often violated in equity markets, 
            // so we won't throw an error, but it's good to know.
        }

        Ok(Self { v0, theta, kappa, xi, rho })
    }

    /// Price a Variance Swap under the Heston model.
    /// The Fair Variance (K_var) of a Variance Swap maturing at time T is the expected 
    /// integrated variance under the risk-neutral measure.
    /// In Heston, this has a simple closed-form solution:
    /// E[ (1/T) \int_0^T V_t dt ] = theta + (v0 - theta) * (1 - e^(-kappa * T)) / (kappa * T)
    pub fn price_variance_swap(&self, t: f64) -> Result<f64, String> {
        if t <= 0.0 { return Err("Time to maturity must be > 0".to_string()); }

        let term = (1.0 - (-self.kappa * t).exp()) / (self.kappa * t);
        let fair_variance = self.theta + (self.v0 - self.theta) * term;

        Ok(fair_variance)
    }

    /// Return the fair Volatility Swap strike (approximate, using convexity adjustment).
    /// Vol Swap Strike = sqrt(Var_Strike) - Convexity_Adjustment
    /// Convexity Adjustment roughly depends on the variance of variance (xi^2).
    /// For simplicity, we just return the square root of the variance swap strike as a naive Vol Swap.
    pub fn price_naive_vol_swap(&self, t: f64) -> Result<f64, String> {
        let var_strike = self.price_variance_swap(t)?;
        Ok(var_strike.sqrt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heston_variance_swap_pricing() {
        // High mean reversion, initial var < theta
        let model = HestonModel::new(
            0.04,  // v0 = 20% vol squared
            0.09,  // theta = 30% vol squared
            2.0,   // kappa
            0.1,   // xi
            -0.7,  // rho
        ).unwrap();

        let t = 1.0; // 1 year
        let var_strike = model.price_variance_swap(t).unwrap();
        
        // Expected integrated variance should be between v0 and theta
        assert!(var_strike > 0.04);
        assert!(var_strike < 0.09);

        // Calculate manually: 0.09 + (0.04 - 0.09) * (1 - e^-2) / 2
        // = 0.09 - 0.05 * (0.8646) / 2 = 0.09 - 0.021615 = 0.068385
        assert!((var_strike - 0.06838).abs() < 1e-4);
    }
}
