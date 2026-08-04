use bt_core::events::{OrderBook, PriceLevel};
use bt_core::types::Side;
use rust_decimal::Decimal;
use rust_decimal::prelude::*;

/// Analytics engine for Level 2 order book data
pub struct OrderBookAnalyzer;

impl OrderBookAnalyzer {
    /// Compute bid/ask volume imbalance ratio: (bid_vol - ask_vol) / (bid_vol + ask_vol)
    /// Returns value in [-1.0, 1.0]. Positive = bid pressure, Negative = ask pressure
    pub fn imbalance(book: &OrderBook) -> f64 {
        let bid_vol: Decimal = book.bids.iter().map(|l| l.size).sum();
        let ask_vol: Decimal = book.asks.iter().map(|l| l.size).sum();

        let total_vol = bid_vol + ask_vol;
        if total_vol.is_zero() {
            return 0.0;
        }

        let imbal = (bid_vol - ask_vol) / total_vol;
        imbal.to_f64().unwrap_or(0.0)
    }

    /// Current best bid-ask spread
    pub fn spread(book: &OrderBook) -> Option<Decimal> {
        let best_bid = book.bids.first()?;
        let best_ask = book.asks.first()?;

        if best_bid.price >= best_ask.price {
            // Crossed or invalid book
            None
        } else {
            Some(best_ask.price - best_bid.price)
        }
    }

    /// Mid price: (best_bid + best_ask) / 2
    pub fn mid_price(book: &OrderBook) -> Option<Decimal> {
        let best_bid = book.bids.first()?;
        let best_ask = book.asks.first()?;

        Some((best_bid.price + best_ask.price) / Decimal::new(2, 0))
    }

    /// Total volume available within `bps` basis points of mid price on each side
    pub fn depth_within_bps(book: &OrderBook, bps: u32) -> Option<(Decimal, Decimal)> {
        let mid = Self::mid_price(book)?;
        let bps_dec = Decimal::from(bps) / Decimal::new(10000, 0);

        let min_bid = mid * (Decimal::ONE - bps_dec);
        let max_ask = mid * (Decimal::ONE + bps_dec);

        let bid_depth: Decimal = book
            .bids
            .iter()
            .take_while(|l| l.price >= min_bid)
            .map(|l| l.size)
            .sum();

        let ask_depth: Decimal = book
            .asks
            .iter()
            .take_while(|l| l.price <= max_ask)
            .map(|l| l.size)
            .sum();

        Some((bid_depth, ask_depth))
    }

    /// Estimate the VWAP fill price if you were to execute `qty` units on given `side`
    /// Walks through order book levels consuming liquidity
    pub fn estimated_fill_price(book: &OrderBook, qty: Decimal, side: Side) -> Option<Decimal> {
        if qty.is_sign_negative() || qty.is_zero() {
            return None;
        }

        let mut remaining_qty = qty;
        let mut total_cost = Decimal::ZERO;

        let levels = match side {
            Side::Buy => &book.asks,
            Side::Sell => &book.bids,
        };

        for level in levels {
            if remaining_qty.is_zero() {
                break;
            }

            let fill_qty = remaining_qty.min(level.size);
            total_cost += fill_qty * level.price;
            remaining_qty -= fill_qty;
        }

        if remaining_qty > Decimal::ZERO {
            // Not enough liquidity to fill the entire quantity
            return None;
        }

        Some(total_cost / qty)
    }

    /// Compute the market impact cost as percentage for executing `qty` on `side`
    /// impact = (estimated_fill_price - mid_price) / mid_price * 100
    pub fn market_impact_pct(book: &OrderBook, qty: Decimal, side: Side) -> Option<f64> {
        let mid = Self::mid_price(book)?;
        let fill_price = Self::estimated_fill_price(book, qty, side)?;

        let impact = match side {
            Side::Buy => (fill_price - mid) / mid * Decimal::new(100, 0),
            Side::Sell => (mid - fill_price) / mid * Decimal::new(100, 0),
        };

        impact.to_f64()
    }

    /// Top N price levels summary
    pub fn top_levels(book: &OrderBook, n: usize) -> (Vec<&PriceLevel>, Vec<&PriceLevel>) {
        let bids = book.bids.iter().take(n).collect();
        let asks = book.asks.iter().take(n).collect();
        (bids, asks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bt_core::types::Symbol;
    use chrono::Utc;

    fn create_mock_book() -> OrderBook {
        OrderBook {
            symbol: Symbol::parse("BTC/USD").unwrap(),
            bids: vec![
                PriceLevel { price: Decimal::new(10000, 0), size: Decimal::new(2, 0), order_count: None },
                PriceLevel { price: Decimal::new(9990, 0), size: Decimal::new(5, 0), order_count: None },
                PriceLevel { price: Decimal::new(9980, 0), size: Decimal::new(3, 0), order_count: None },
            ],
            asks: vec![
                PriceLevel { price: Decimal::new(10010, 0), size: Decimal::new(1, 0), order_count: None },
                PriceLevel { price: Decimal::new(10020, 0), size: Decimal::new(4, 0), order_count: None },
                PriceLevel { price: Decimal::new(10030, 0), size: Decimal::new(5, 0), order_count: None },
            ],
            timestamp: Utc::now(),
            venue: bt_core::types::Venue::Binance,
        }
    }

    #[test]
    fn test_imbalance() {
        let book = create_mock_book();
        // bids total 10, asks total 10
        let imb = OrderBookAnalyzer::imbalance(&book);
        assert_eq!(imb, 0.0);
    }

    #[test]
    fn test_spread_and_mid() {
        let book = create_mock_book();
        assert_eq!(OrderBookAnalyzer::spread(&book).unwrap(), Decimal::new(10, 0));
        assert_eq!(OrderBookAnalyzer::mid_price(&book).unwrap(), Decimal::new(10005, 0));
    }

    #[test]
    fn test_depth() {
        let book = create_mock_book();
        // mid is 10005. 20 bps is 0.002 * 10005 = ~20
        // min bid = 10005 - 20 = 9985, max ask = 10005 + 20 = 10025
        let (bid_depth, ask_depth) = OrderBookAnalyzer::depth_within_bps(&book, 20).unwrap();
        assert_eq!(bid_depth, Decimal::new(7, 0)); // 10000 and 9990
        assert_eq!(ask_depth, Decimal::new(5, 0)); // 10010 and 10020
    }

    #[test]
    fn test_fill_price() {
        let book = create_mock_book();
        // Buy 3. Takes 1 at 10010, 2 at 10020.
        // Cost: 10010 + 20040 = 30050. Avg: 30050 / 3 = 10016.666...
        let fill = OrderBookAnalyzer::estimated_fill_price(&book, Decimal::new(3, 0), Side::Buy).unwrap();
        assert_eq!(fill.round_dp(2), Decimal::new(1001667, 2));
    }

    #[test]
    fn test_fill_price_insufficient_liquidity() {
        let book = create_mock_book();
        // Buy 20, but only 10 available
        let fill = OrderBookAnalyzer::estimated_fill_price(&book, Decimal::new(20, 0), Side::Buy);
        assert!(fill.is_none());
    }

    #[test]
    fn test_market_impact() {
        let book = create_mock_book();
        // Buy 3. Mid is 10005. Fill is 10016.666...
        // Impact = (10016.666 - 10005) / 10005 * 100 = 0.1166%
        let impact = OrderBookAnalyzer::market_impact_pct(&book, Decimal::new(3, 0), Side::Buy).unwrap();
        assert!((impact - 0.1166).abs() < 0.0001);
    }
}
