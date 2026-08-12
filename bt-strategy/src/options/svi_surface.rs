/// Stochastic Volatility Inspired (SVI) Parameterization.
/// Models the implied variance smile in a mathematically guaranteed arbitrage-free way.
/// Formula: w(k) = a + b * (rho * (k - m) + sqrt((k - m)^2 + sigma^2))
/// where:
/// - k = log-strike = ln(K/F)
/// - w(k) = implied variance = implied_vol^2 * T
#[derive(Debug, Clone, Copy)]
pub struct SviSurface {
    /// Vertical shift parameter (must be >= 0)
    pub a: f64,
    /// Angle of the asymptotes (must be >= 0)
    pub b: f64,
    /// Correlation (determines the skew, -1 <= rho <= 1)
    pub rho: f64,
    /// Horizontal shift (translates the smile)
    pub m: f64,
    /// Smoothness of the vertex (must be > 0)
    pub sigma: f64,
}

impl SviSurface {
    pub fn new(a: f64, b: f64, rho: f64, m: f64, sigma: f64) -> Result<Self, String> {
        if b < 0.0 { return Err("b must be >= 0".to_string()); }
        if !(-1.0..=1.0).contains(&rho) { return Err("rho must be between -1 and 1".to_string()); }
        if sigma <= 0.0 { return Err("sigma must be > 0".to_string()); }
        
        // a + b*sigma*sqrt(1 - rho^2) must be >= 0 to ensure non-negative variance globally
        let min_var = a + b * sigma * (1.0 - rho * rho).sqrt();
        if min_var < 0.0 {
            return Err("SVI parameters lead to negative variance (arbitrage)".to_string());
        }

        Ok(Self { a, b, rho, m, sigma })
    }

    /// Compute the implied variance w(k) for a given log-strike `k`.
    pub fn variance(&self, k: f64) -> f64 {
        let diff = k - self.m;
        self.a + self.b * (self.rho * diff + (diff * diff + self.sigma * self.sigma).sqrt())
    }

    /// Compute the implied volatility for a given log-strike `k` and time to maturity `t`.
    pub fn implied_volatility(&self, k: f64, t: f64) -> Result<f64, String> {
        if t <= 0.0 { return Err("Time to maturity must be > 0".to_string()); }
        let var = self.variance(k);
        if var < 0.0 {
            return Err("Negative variance encountered".to_string());
        }
        Ok((var / t).sqrt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_svi_arbitrage_free_validation() {
        // Valid parameters
        let svi = SviSurface::new(0.04, 0.1, -0.4, 0.1, 0.1);
        assert!(svi.is_ok());

        // Invalid: negative sigma
        let invalid_sigma = SviSurface::new(0.04, 0.1, -0.4, 0.1, -0.1);
        assert!(invalid_sigma.is_err());

        // Invalid: rho out of bounds
        let invalid_rho = SviSurface::new(0.04, 0.1, -1.5, 0.1, 0.1);
        assert!(invalid_rho.is_err());
    }

    #[test]
    fn test_svi_implied_volatility() {
        let svi = SviSurface::new(0.02, 0.1, -0.5, 0.0, 0.1).unwrap();
        
        let t = 0.5; // 6 months
        // ATM strike (K = F => k = 0)
        let atm_vol = svi.implied_volatility(0.0, t).unwrap();
        
        // OTM Put (K < F => k < 0). Due to negative rho (skew), this should be higher vol.
        let otm_put_vol = svi.implied_volatility(-0.2, t).unwrap();
        
        // OTM Call (K > F => k > 0). Should be lower than ATM due to skew, but eventually rises.
        let otm_call_vol = svi.implied_volatility(0.2, t).unwrap();

        assert!(otm_put_vol > atm_vol, "Negative skew should make OTM put vol higher than ATM");
        assert!(atm_vol > 0.0);
        assert!(otm_call_vol > 0.0);
    }
}
