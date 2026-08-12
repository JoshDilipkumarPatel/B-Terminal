use std::collections::VecDeque;

/// A single trade tick
#[derive(Debug, Clone, Copy)]
pub struct TradeTick {
    pub price: f64,
    pub volume: f64,
    pub is_buyer_maker: bool, // Indicates if the trade was initiated by a seller hitting the bid
}

/// Represents a volume bucket for VPIN calculation
#[derive(Debug, Clone, Default)]
pub struct VolumeBucket {
    pub buy_volume: f64,
    pub sell_volume: f64,
    pub total_volume: f64,
}

impl VolumeBucket {
    pub fn add_trade(&mut self, trade: &TradeTick) {
        self.total_volume += trade.volume;
        // If buyer is maker, a seller crossed the spread (sell initiated). 
        // Otherwise, a buyer crossed the spread (buy initiated).
        if trade.is_buyer_maker {
            self.sell_volume += trade.volume;
        } else {
            self.buy_volume += trade.volume;
        }
    }
}

/// Volume-Synchronized Probability of Informed Trading (VPIN)
pub struct VpinEstimator {
    pub bucket_volume_threshold: f64,
    pub window_size: usize,
    pub current_bucket: VolumeBucket,
    pub history: VecDeque<VolumeBucket>,
}

impl VpinEstimator {
    /// Creates a new VPIN estimator.
    /// `bucket_volume_threshold`: Total volume required to complete a single bucket.
    /// `window_size`: Number of buckets `n` over which to compute VPIN.
    pub fn new(bucket_volume_threshold: f64, window_size: usize) -> Self {
        Self {
            bucket_volume_threshold,
            window_size,
            current_bucket: VolumeBucket::default(),
            history: VecDeque::with_capacity(window_size),
        }
    }

    /// Ingests a new trade tick. Returns `Some(vpin)` if a new bucket was completed and 
    /// the window is full, `None` otherwise.
    pub fn update(&mut self, trade: &TradeTick) -> Option<f64> {
        let mut remaining_vol = trade.volume;
        let trade_price = trade.price;
        let is_buyer_maker = trade.is_buyer_maker;
        
        let mut latest_vpin = None;

        // Handle trades larger than the bucket threshold by splitting them
        while remaining_vol > 0.0 {
            let volume_needed = self.bucket_volume_threshold - self.current_bucket.total_volume;
            
            let fill_vol = remaining_vol.min(volume_needed);
            
            let partial_trade = TradeTick {
                price: trade_price,
                volume: fill_vol,
                is_buyer_maker,
            };
            
            self.current_bucket.add_trade(&partial_trade);
            remaining_vol -= fill_vol;

            // If the bucket is full, push it to history and compute VPIN
            if self.current_bucket.total_volume >= self.bucket_volume_threshold - 1e-9 {
                if self.history.len() == self.window_size {
                    self.history.pop_front();
                }
                self.history.push_back(self.current_bucket.clone());
                self.current_bucket = VolumeBucket::default();
                
                if self.history.len() == self.window_size {
                    latest_vpin = Some(self.calculate_vpin());
                }
            }
        }
        
        latest_vpin
    }

    /// Computes the VPIN score across the rolling window of buckets.
    /// Formula: sum(|Buy_Vol - Sell_Vol|) / (n * Bucket_Volume)
    fn calculate_vpin(&self) -> f64 {
        let mut absolute_imbalance_sum = 0.0;
        let mut total_vol_sum = 0.0;

        for bucket in &self.history {
            absolute_imbalance_sum += (bucket.buy_volume - bucket.sell_volume).abs();
            total_vol_sum += bucket.total_volume;
        }

        if total_vol_sum == 0.0 {
            0.0
        } else {
            absolute_imbalance_sum / total_vol_sum
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vpin_calculation() {
        // Bucket size 100, window 2
        let mut estimator = VpinEstimator::new(100.0, 2);
        
        // Bucket 1: 100% buy initiated
        assert_eq!(estimator.update(&TradeTick { price: 10.0, volume: 100.0, is_buyer_maker: false }), None);
        
        // Bucket 2: 100% sell initiated
        let vpin = estimator.update(&TradeTick { price: 10.0, volume: 100.0, is_buyer_maker: true });
        
        // Bucket 1 Imbalance = |100 - 0| = 100
        // Bucket 2 Imbalance = |0 - 100| = 100
        // VPIN = (100 + 100) / (200) = 1.0 (Maximum toxicity)
        assert_eq!(vpin, Some(1.0));

        // Bucket 3: 50% buy, 50% sell
        estimator.update(&TradeTick { price: 10.0, volume: 50.0, is_buyer_maker: false });
        let vpin_2 = estimator.update(&TradeTick { price: 10.0, volume: 50.0, is_buyer_maker: true });
        
        // History now has Bucket 2 (Imbalance 100) and Bucket 3 (Imbalance 0)
        // VPIN = (100 + 0) / 200 = 0.5
        assert_eq!(vpin_2, Some(0.5));
    }
}
