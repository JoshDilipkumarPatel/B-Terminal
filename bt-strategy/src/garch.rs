use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GarchParams {
    pub omega: f64,
    pub alpha: f64,
    pub beta: f64,
    pub long_run_variance: f64,
}

#[derive(Debug, Clone)]
pub struct GarchModel;

impl GarchModel {
    pub fn fit(returns: &[f64]) -> Option<GarchParams> {
        if returns.is_empty() {
            return None;
        }

        let n = returns.len() as f64;
        let mean = returns.iter().sum::<f64>() / n;
        
        let mut variance = 0.0;
        for &r in returns {
            variance += (r - mean).powi(2);
        }
        variance /= n.max(1.0);
        let long_run_variance = variance;

        if long_run_variance <= 0.0 {
            return None;
        }

        let mut best_params: Option<GarchParams> = None;
        let mut best_ll = f64::NEG_INFINITY;

        let alpha_steps = 15; // 0.01 to 0.29
        let beta_steps = 25; // 0.50 to 0.98

        for i in 0..=alpha_steps {
            let alpha = 0.01 + (i as f64) * 0.02;
            for j in 0..=beta_steps {
                let beta = 0.50 + (j as f64) * 0.02;

                if alpha + beta >= 1.0 {
                    continue;
                }

                let omega = long_run_variance * (1.0 - alpha - beta);
                
                let mut ll = 0.0;
                let mut sigma2 = long_run_variance;

                for &r in returns {
                    let eps = r - mean;
                    let eps2 = eps * eps;

                    // Log-likelihood: -0.5 * (ln(sigma2) + eps2/sigma2)
                    ll += -0.5 * (sigma2.ln() + eps2 / sigma2);

                    sigma2 = omega + alpha * eps2 + beta * sigma2;
                }

                if ll > best_ll {
                    best_ll = ll;
                    best_params = Some(GarchParams {
                        omega,
                        alpha,
                        beta,
                        long_run_variance,
                    });
                }
            }
        }

        best_params
    }

    pub fn conditional_variance(params: &GarchParams, returns: &[f64]) -> f64 {
        if returns.is_empty() {
            return params.long_run_variance;
        }
        
        let n = returns.len() as f64;
        let mean = returns.iter().sum::<f64>() / n;
        
        let mut sigma2 = params.long_run_variance;
        for &r in returns {
            let eps2 = (r - mean).powi(2);
            sigma2 = params.omega + params.alpha * eps2 + params.beta * sigma2;
        }
        sigma2
    }

    pub fn forecast(params: &GarchParams, current_variance: f64, horizon: usize) -> Vec<f64> {
        let mut forecasts = Vec::with_capacity(horizon);
        let persistence = params.alpha + params.beta;
        
        for k in 1..=horizon {
            let k_f64 = k as f64;
            let forecast_var = params.long_run_variance + persistence.powf(k_f64) * (current_variance - params.long_run_variance);
            forecasts.push(forecast_var.max(0.0));
        }
        forecasts
    }

    pub fn annualized_vol(variance: f64) -> f64 {
        (variance * 252.0).max(0.0).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_garch_fit_and_forecast() {
        // Synthetic data with some variance
        let returns = vec![
            0.01, -0.02, 0.015, -0.01, 0.03, -0.025, 0.005, -0.005, 0.02, -0.015,
            0.01, -0.02, 0.015, -0.01, 0.03, -0.025, 0.005, -0.005, 0.02, -0.015,
        ];
        
        let params = GarchModel::fit(&returns).unwrap();
        assert!(params.alpha > 0.0);
        assert!(params.beta > 0.0);
        assert!(params.alpha + params.beta < 1.0);
        
        let cond_var = GarchModel::conditional_variance(&params, &returns);
        assert!(cond_var > 0.0);
        
        let forecasts = GarchModel::forecast(&params, cond_var, 5);
        assert_eq!(forecasts.len(), 5);
        for &f in &forecasts {
            assert!(f > 0.0);
        }
        
        let ann_vol = GarchModel::annualized_vol(cond_var);
        assert!(ann_vol > 0.0);
    }
}
