use std::collections::HashMap;
use std::time::{Instant, Duration};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HiddenLiquidity {
    None,
    BuyIceberg,
    SellIceberg,
}

/// A lightweight, lock-free pattern recognition model to detect 
/// institutional Iceberg Orders (hidden liquidity) on the L2 order book.
pub struct IcebergDetector {
    /// Track the cumulative volume traded at specific price levels within a short time window.
    /// Key: Price level (multiplied by 10000 and cast to i64 to avoid float hashing issues)
    /// Value: (Cumulative Volume Traded, Last Update Time)
    price_level_volume: HashMap<i64, (f64, Instant)>,
    
    /// The threshold multiplier. If Traded Volume > Displayed Volume * Multiplier, flag as iceberg.
    reload_threshold_multiplier: f64,
    
    /// Time window to reset cumulative volume
    decay_window: Duration,
}

impl Default for IcebergDetector {
    fn default() -> Self {
        Self::new(2.5, Duration::from_millis(500))
    }
}

impl IcebergDetector {
    pub fn new(reload_threshold_multiplier: f64, decay_window: Duration) -> Self {
        Self {
            price_level_volume: HashMap::new(),
            reload_threshold_multiplier,
            decay_window,
        }
    }

    fn price_to_key(price: f64) -> i64 {
        (price * 10_000.0).round() as i64
    }

    /// Process a new aggressive market trade against a resting limit order.
    /// 
    /// # Arguments
    /// * `price` - The execution price level
    /// * `trade_volume` - The volume of the aggressive trade
    /// * `displayed_volume_remaining` - The amount of volume still visible on L2 at this price AFTER the trade
    /// * `is_buyer_maker` - True if a resting buy limit was hit (someone sold market). False if resting sell limit hit.
    pub fn process_trade(
        &mut self,
        price: f64,
        trade_volume: f64,
        displayed_volume_remaining: f64,
        is_buyer_maker: bool, // True -> bid was hit. False -> ask was hit.
    ) -> HiddenLiquidity {
        let key = Self::price_to_key(price);
        let now = Instant::now();

        let (mut cumulative_vol, last_time) = self.price_level_volume.get(&key).copied().unwrap_or((0.0, now));
        
        // Decay/reset if time window passed
        if now.duration_since(last_time) > self.decay_window {
            cumulative_vol = 0.0;
        }

        cumulative_vol += trade_volume;
        self.price_level_volume.insert(key, (cumulative_vol, now));

        // Detection Logic:
        // If we have traded significantly more volume at this price than what is currently displayed,
        // and the level hasn't collapsed (displayed > 0), someone is constantly reloading it (Iceberg).
        
        if displayed_volume_remaining > 0.0 && cumulative_vol > (displayed_volume_remaining * self.reload_threshold_multiplier) {
            // Strong evidence of an iceberg
            if is_buyer_maker {
                // Hitting the bid repeatedly, but bid doesn't break -> Buy Iceberg (Hidden Support)
                HiddenLiquidity::BuyIceberg
            } else {
                // Hitting the ask repeatedly, but ask doesn't break -> Sell Iceberg (Hidden Resistance)
                HiddenLiquidity::SellIceberg
            }
        } else {
            HiddenLiquidity::None
        }
    }
    
    pub fn clear_stale_levels(&mut self) {
        let now = Instant::now();
        let decay = self.decay_window;
        self.price_level_volume.retain(|_, &mut (_, last_time)| now.duration_since(last_time) <= decay);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_detect_buy_iceberg() {
        let mut detector = IcebergDetector::default();
        let price = 150.25;
        
        // 1. Initial hit. 100 shares traded, 100 shares left visible. (Traded 100 < 100 * 2.5) -> None
        assert_eq!(detector.process_trade(price, 100.0, 100.0, true), HiddenLiquidity::None);
        
        // 2. Second hit. 100 more shares traded. (Traded 200 < 100 * 2.5) -> None
        assert_eq!(detector.process_trade(price, 100.0, 100.0, true), HiddenLiquidity::None);

        // 3. Third hit. 100 more shares traded. Total 300 traded. Visible remains 100. (Traded 300 > 100 * 2.5) -> Buy Iceberg!
        assert_eq!(detector.process_trade(price, 100.0, 100.0, true), HiddenLiquidity::BuyIceberg);
    }

    #[test]
    fn test_decay_prevents_false_positives() {
        let mut detector = IcebergDetector::new(2.5, Duration::from_millis(50));
        let price = 150.25;
        
        assert_eq!(detector.process_trade(price, 100.0, 100.0, true), HiddenLiquidity::None);
        
        // Sleep to trigger decay
        sleep(Duration::from_millis(60));
        
        // Because of decay, cumulative volume resets. This won't trigger the iceberg.
        assert_eq!(detector.process_trade(price, 200.0, 100.0, true), HiddenLiquidity::None);
    }
}
