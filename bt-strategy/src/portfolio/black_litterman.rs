use nalgebra::{DMatrix, DVector};

/// The Black-Litterman Model for Portfolio Optimization.
/// Fuses market equilibrium (prior) with subjective AI Syndicate views (posterior)
/// to generate an optimal set of expected returns and weights.
pub struct BlackLitterman;

impl BlackLitterman {
    /// Computes the Black-Litterman expected returns.
    ///
    /// # Arguments
    /// * `cov` - Covariance matrix of assets (N x N)
    /// * `market_weights` - Market capitalization weights (N x 1)
    /// * `risk_aversion` - Market risk aversion coefficient (lambda)
    /// * `tau` - Weight-on-views scalar (typically 0.025 to 0.05)
    /// * `p` - View projection matrix (K x N) identifying the assets in each view
    /// * `q` - View returns vector (K x 1)
    /// * `omega` - View uncertainty diagonal matrix (K x K)
    pub fn compute_expected_returns(
        cov: &DMatrix<f64>,
        market_weights: &DVector<f64>,
        risk_aversion: f64,
        tau: f64,
        p: &DMatrix<f64>,
        q: &DVector<f64>,
        omega: &DMatrix<f64>,
    ) -> Result<DVector<f64>, String> {
        let _n = cov.nrows();
        
        // 1. Calculate Implied Equilibrium Returns (Pi)
        // Pi = lambda * Cov * w_mkt
        let pi = cov * market_weights * risk_aversion;

        if p.nrows() == 0 {
            // No views provided, return market equilibrium
            return Ok(pi);
        }

        // 2. Calculate tau * Cov
        let tau_cov = cov * tau;
        
        // Attempt to invert tau_cov. If singular, we cannot proceed with standard BL.
        let tau_cov_inv = match tau_cov.clone().try_inverse() {
            Some(inv) => inv,
            None => {
                // In production, we'd use a pseudo-inverse (SVD). 
                // For safety in this high-performance context, if inversion fails, fallback to equilibrium.
                return Ok(pi);
            }
        };

        // Attempt to invert Omega
        let omega_inv = match omega.clone().try_inverse() {
            Some(inv) => inv,
            None => return Ok(pi),
        };

        let p_trans = p.transpose();

        // M1 = ( (tau*Cov)^-1 + P^T * Omega^-1 * P )^-1
        let m1_inner = &tau_cov_inv + (&p_trans * &omega_inv * p);
        let m1 = match m1_inner.try_inverse() {
            Some(inv) => inv,
            None => return Ok(pi),
        };

        // M2 = (tau*Cov)^-1 * Pi + P^T * Omega^-1 * Q
        let m2 = (&tau_cov_inv * &pi) + (&p_trans * &omega_inv * q);

        // BL Expected Returns = M1 * M2
        let bl_returns = &m1 * &m2;

        Ok(bl_returns)
    }

    /// Helper to convert BL Expected Returns into Target Weights via Mean-Variance
    /// w = (lambda * Cov)^-1 * E[R]
    pub fn returns_to_weights(
        cov: &DMatrix<f64>,
        expected_returns: &DVector<f64>,
        risk_aversion: f64,
    ) -> Result<DVector<f64>, String> {
        let cov_inv = cov.clone().try_inverse().ok_or("Covariance matrix is singular")?;
        let unscaled_weights = (cov_inv * expected_returns) / risk_aversion;
        
        // Normalize weights to sum to 1.0 (assuming fully invested, long-only or long/short relaxed)
        let sum: f64 = unscaled_weights.iter().map(|v| v.abs()).sum();
        if sum == 0.0 {
            return Ok(DVector::zeros(unscaled_weights.len()));
        }
        
        let normalized = unscaled_weights / sum;
        Ok(normalized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_black_litterman_fusion() {
        let n = 2;
        let cov = DMatrix::from_row_slice(n, n, &[
            0.04, 0.005,
            0.005, 0.02
        ]);
        let market_weights = DVector::from_vec(vec![0.6, 0.4]);
        let risk_aversion = 2.5;
        let tau = 0.05;

        // View 1: Asset 0 will return 5% (0.05)
        let p = DMatrix::from_row_slice(1, n, &[1.0, 0.0]);
        let q = DVector::from_vec(vec![0.05]);
        
        // Very confident view (low variance in omega)
        let omega = DMatrix::from_row_slice(1, 1, &[0.001]);

        let bl_returns = BlackLitterman::compute_expected_returns(
            &cov, &market_weights, risk_aversion, tau, &p, &q, &omega
        ).unwrap();

        assert_eq!(bl_returns.len(), 2);
        // The BL return for Asset 0 should be pulled strongly toward the 0.05 view
        assert!((bl_returns[0] - 0.05).abs() < 0.015);
    }
}
