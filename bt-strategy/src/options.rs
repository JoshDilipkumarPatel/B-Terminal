use serde::{Serialize, Deserialize};
use statrs::distribution::{Normal, Continuous, ContinuousCDF};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum OptionKind { Call, Put }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionPricing {
    pub kind: OptionKind,
    pub spot: f64,
    pub strike: f64,
    pub rate: f64,
    pub volatility: f64,
    pub time_to_expiry: f64,
    pub price: f64,
    pub greeks: Greeks,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Greeks {
    pub delta: f64,
    pub gamma: f64,
    pub theta: f64,
    pub vega: f64,
    pub rho: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrikeData {
    pub strike: f64,
    pub call: Option<OptionPricing>,
    pub put: Option<OptionPricing>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionsChain {
    pub underlying: String,
    pub spot_price: f64,
    pub expiry: String,
    pub risk_free_rate: f64,
    pub strikes: Vec<StrikeData>,
}

pub struct BlackScholes;

impl BlackScholes {
    fn d1(spot: f64, strike: f64, rate: f64, vol: f64, time: f64) -> f64 {
        if time <= 0.0 {
            return if spot > strike { f64::INFINITY } else { f64::NEG_INFINITY };
        }
        ((spot / strike).ln() + (rate + vol * vol / 2.0) * time) / (vol * time.sqrt())
    }

    fn d2(spot: f64, strike: f64, rate: f64, vol: f64, time: f64) -> f64 {
        Self::d1(spot, strike, rate, vol, time) - vol * time.sqrt()
    }

    pub fn price(kind: OptionKind, spot: f64, strike: f64, rate: f64, vol: f64, time: f64) -> f64 {
        if time <= 0.0 {
            return match kind {
                OptionKind::Call => (spot - strike).max(0.0),
                OptionKind::Put => (strike - spot).max(0.0),
            };
        }

        let d1 = Self::d1(spot, strike, rate, vol, time);
        let d2 = Self::d2(spot, strike, rate, vol, time);
        let normal = Normal::new(0.0, 1.0).unwrap();

        match kind {
            OptionKind::Call => spot * normal.cdf(d1) - strike * (-rate * time).exp() * normal.cdf(d2),
            OptionKind::Put => strike * (-rate * time).exp() * normal.cdf(-d2) - spot * normal.cdf(-d1),
        }
    }

    pub fn greeks(kind: OptionKind, spot: f64, strike: f64, rate: f64, vol: f64, time: f64) -> Greeks {
        if time <= 0.0 {
            return Greeks::default();
        }

        let d1 = Self::d1(spot, strike, rate, vol, time);
        let d2 = Self::d2(spot, strike, rate, vol, time);
        let normal = Normal::new(0.0, 1.0).unwrap();

        let n_d1 = normal.pdf(d1);
        let cdf_d1 = normal.cdf(d1);
        let _cdf_neg_d1 = normal.cdf(-d1);
        let cdf_d2 = normal.cdf(d2);
        let cdf_neg_d2 = normal.cdf(-d2);

        let gamma = n_d1 / (spot * vol * time.sqrt());
        let vega = spot * n_d1 * time.sqrt() / 100.0; // Per 1% change

        match kind {
            OptionKind::Call => {
                let delta = cdf_d1;
                let theta = (-spot * n_d1 * vol / (2.0 * time.sqrt()) - rate * strike * (-rate * time).exp() * cdf_d2) / 365.0;
                let rho = strike * time * (-rate * time).exp() * cdf_d2 / 100.0;
                Greeks { delta, gamma, theta, vega, rho }
            },
            OptionKind::Put => {
                let delta = cdf_d1 - 1.0;
                let theta = (-spot * n_d1 * vol / (2.0 * time.sqrt()) + rate * strike * (-rate * time).exp() * cdf_neg_d2) / 365.0;
                let rho = -strike * time * (-rate * time).exp() * cdf_neg_d2 / 100.0;
                Greeks { delta, gamma, theta, vega, rho }
            }
        }
    }

    pub fn implied_volatility(kind: OptionKind, market_price: f64, spot: f64, strike: f64, rate: f64, time: f64) -> Option<f64> {
        let mut vol = 0.5; // Initial guess
        let max_iter = 100;
        let tol = 1e-8;

        for _ in 0..max_iter {
            let price = Self::price(kind, spot, strike, rate, vol, time);
            let diff = price - market_price;
            
            if diff.abs() < tol {
                return Some(vol);
            }

            let greeks = Self::greeks(kind, spot, strike, rate, vol, time);
            let vega = greeks.vega * 100.0; // convert back from per 1%

            if vega == 0.0 {
                break;
            }

            vol -= diff / vega;

            if vol <= 0.0 {
                vol = 1e-5;
            }
        }
        
        None
    }

    pub fn price_with_greeks(kind: OptionKind, spot: f64, strike: f64, rate: f64, vol: f64, time: f64) -> OptionPricing {
        OptionPricing {
            kind,
            spot,
            strike,
            rate,
            volatility: vol,
            time_to_expiry: time,
            price: Self::price(kind, spot, strike, rate, vol, time),
            greeks: Self::greeks(kind, spot, strike, rate, vol, time),
        }
    }

    pub fn generate_chain(underlying: String, spot: f64, rate: f64, vol: f64, time: f64, expiry_label: String, strikes: &[f64]) -> OptionsChain {
        let mut chain_strikes = Vec::new();
        for &strike in strikes {
            let call = Self::price_with_greeks(OptionKind::Call, spot, strike, rate, vol, time);
            let put = Self::price_with_greeks(OptionKind::Put, spot, strike, rate, vol, time);
            chain_strikes.push(StrikeData {
                strike,
                call: Some(call),
                put: Some(put),
            });
        }
        
        OptionsChain {
            underlying,
            spot_price: spot,
            expiry: expiry_label,
            risk_free_rate: rate,
            strikes: chain_strikes,
        }
    }

    pub fn nifty_weekly_chain(spot: f64, rate: f64, vol: f64, days_to_expiry: f64) -> OptionsChain {
        let center = (spot / 50.0).round() * 50.0;
        let start = center - 1000.0;
        let end = center + 1000.0;
        
        let mut strikes = Vec::new();
        let mut curr = start;
        while curr <= end {
            strikes.push(curr);
            curr += 50.0;
        }
        
        let time = days_to_expiry / 365.0;
        Self::generate_chain("NIFTY".to_string(), spot, rate, vol, time, format!("{}D", days_to_expiry), &strikes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bs_call_put_parity() {
        let spot = 100.0;
        let strike = 100.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 1.0;

        let call = BlackScholes::price(OptionKind::Call, spot, strike, rate, vol, time);
        let put = BlackScholes::price(OptionKind::Put, spot, strike, rate, vol, time);

        // C - P = S - K*e^(-rt)
        let diff = call - put;
        let expected = spot - strike * (-rate * time).exp();

        assert!((diff - expected).abs() < 1e-4);
    }
    
    #[test]
    fn test_implied_vol() {
        let spot = 100.0;
        let strike = 100.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 1.0;
        
        let price = BlackScholes::price(OptionKind::Call, spot, strike, rate, vol, time);
        let iv = BlackScholes::implied_volatility(OptionKind::Call, price, spot, strike, rate, time).unwrap();
        
        assert!((iv - vol).abs() < 1e-4);
    }
}
