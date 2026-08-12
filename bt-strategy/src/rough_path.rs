#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LeadLagSignal {
    PriceLeadsVolume(f64),
    VolumeLeadsPrice(f64),
    Neutral,
}

pub struct RoughPathAnalyzer;

impl RoughPathAnalyzer {
    /// Calculates the Level-2 Rough Path Signature (specifically the Lévy Area)
    /// of a 2-dimensional path to determine the lead-lag relationship.
    /// 
    /// `path` is an array of (Price, Volume) tuples over a time window.
    /// Returns the signed Lévy Area.
    pub fn calculate_levy_area(path: &[(f64, f64)]) -> f64 {
        if path.len() < 2 {
            return 0.0;
        }

        let mut area = 0.0;
        let (p0, v0) = path[0];

        for i in 1..path.len() {
            let p_prev = path[i - 1].0 - p0;
            let v_prev = path[i - 1].1 - v0;
            let p_curr = path[i].0 - p0;
            let v_curr = path[i].1 - v0;

            // Stratonovich iterated integral (Shoelace formula for Lévy Area)
            // A = 0.5 * sum(X_{i-1}*Y_i - X_i*Y_{i-1})
            area += (p_prev * v_curr) - (p_curr * v_prev);
        }

        0.5 * area
    }

    /// Analyzes a Price-Volume path to determine which dimension is driving the market.
    pub fn analyze_lead_lag(path: &[(f64, f64)]) -> LeadLagSignal {
        let area = Self::calculate_levy_area(path);
        
        // Threshold to avoid triggering on pure micro-noise
        let noise_threshold = 1e-6;

        if area > noise_threshold {
            // Price leads Volume (Price moves first, Volume follows)
            LeadLagSignal::PriceLeadsVolume(area)
        } else if area < -noise_threshold {
            // Volume leads Price (Volume arrives first, Price moves later)
            // Negative area means Volume (Y-axis) leads Price (X-axis)
            LeadLagSignal::VolumeLeadsPrice(area.abs())
        } else {
            LeadLagSignal::Neutral
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lead_lag_price_leads_volume() {
        // Price spikes first, then volume follows
        let path = vec![
            (100.0, 1000.0), // baseline
            (105.0, 1000.0), // price jumps, volume dormant
            (105.0, 5000.0), // volume finally reacts
        ];
        
        let signal = RoughPathAnalyzer::analyze_lead_lag(&path);
        match signal {
            LeadLagSignal::PriceLeadsVolume(area) => {
                assert!(area > 0.0);
            },
            _ => panic!("Expected PriceLeadsVolume, got {:?}", signal),
        }
    }

    #[test]
    fn test_lead_lag_volume_leads_price() {
        // Volume spikes first (informed order flow), then price moves
        let path = vec![
            (100.0, 1000.0), // baseline
            (100.0, 5000.0), // massive volume prints at same price
            (105.0, 5000.0), // price finally jumps to catch up
        ];
        
        let signal = RoughPathAnalyzer::analyze_lead_lag(&path);
        match signal {
            LeadLagSignal::VolumeLeadsPrice(area) => {
                assert!(area > 0.0);
            },
            _ => panic!("Expected VolumeLeadsPrice, got {:?}", signal),
        }
    }
}
