use std::time::Instant;

/// Implementation Shortfall (IS) Execution Algorithm.
/// Dynamically accelerates or decelerates order execution slicing based on 
/// how the market price has moved relative to the "Arrival Price" (the price
/// at the exact microsecond the Orchestrator decided to trade).
pub struct ImplementationShortfall {
    pub arrival_price: f64,
    pub is_buy_order: bool,
    pub arrival_time: Instant,
    
    /// Base participation rate (e.g. 0.10 means target 10% of market volume)
    pub base_participation_rate: f64,
}

impl ImplementationShortfall {
    pub fn new(arrival_price: f64, is_buy_order: bool, base_participation_rate: f64) -> Self {
        Self {
            arrival_price,
            is_buy_order,
            arrival_time: Instant::now(),
            base_participation_rate,
        }
    }

    /// Calculates the dynamic urgency multiplier and updated participation rate
    /// based on the current market price.
    ///
    /// # Arguments
    /// * `current_price` - The current L1 best bid/ask
    ///
    /// # Returns
    /// * `(urgency_multiplier, target_participation_rate)`
    pub fn compute_urgency(&self, current_price: f64) -> (f64, f64) {
        let price_delta_pct = (current_price - self.arrival_price) / self.arrival_price;
        
        // If Buy order and price went up (positive delta), it's adverse.
        // If Sell order and price went down (negative delta), it's adverse.
        let is_adverse = if self.is_buy_order {
            price_delta_pct > 0.0
        } else {
            price_delta_pct < 0.0
        };

        // Urgency scales with the magnitude of the move. 
        // Example: a 1% adverse move increases urgency significantly.
        let abs_delta = price_delta_pct.abs();
        
        let urgency_multiplier = if is_adverse {
            // Price is slipping away, accelerate to capture fills before it gets worse
            1.0 + (abs_delta * 100.0) // E.g., 1% move -> 2.0x urgency
        } else {
            // Price is moving in our favor, decelerate to capture better prices passively
            // Minimum urgency is 0.2x to ensure we don't completely stop trading
            (1.0 - (abs_delta * 50.0)).max(0.2) 
        };

        let target_participation = self.base_participation_rate * urgency_multiplier;
        
        // Cap participation at 50% to avoid being the entire market
        let capped_participation = target_participation.min(0.50);

        (urgency_multiplier, capped_participation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_adverse_acceleration_buy() {
        let arrival_price = 100.0;
        let is_algo = ImplementationShortfall::new(arrival_price, true, 0.10);

        // Price goes UP to 101.0 (1% adverse move for a buyer)
        let current_price = 101.0;
        
        let (urgency, participation) = is_algo.compute_urgency(current_price);
        
        // 1% move * 100.0 = 1.0. Urgency should be 1.0 + 1.0 = 2.0
        assert_eq!(urgency, 2.0);
        assert_eq!(participation, 0.20); // 10% * 2.0 = 20% participation
    }

    #[test]
    fn test_is_favorable_deceleration_sell() {
        let arrival_price = 100.0;
        // Sell order
        let is_algo = ImplementationShortfall::new(arrival_price, false, 0.10);

        // Price goes UP to 101.0 (1% favorable move for a seller, since we can sell higher)
        let current_price = 101.0;
        
        let (urgency, participation) = is_algo.compute_urgency(current_price);
        
        // 1% move * 50.0 = 0.5. Urgency should be 1.0 - 0.5 = 0.5
        assert_eq!(urgency, 0.5);
        assert_eq!(participation, 0.05); // 10% * 0.5 = 5% participation
    }
}
