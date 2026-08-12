use crate::provider::{BarsRequest, Bar, Quote, Trade, OrderBook, HistoricalDataProvider, MarketDataProvider, NewsProvider, ProviderInfo, ConnectionStatus, RateLimitInfo, HealthStatus, Subscription, Symbol};
use anyhow::Result;
use async_trait::async_trait;
use bt_core::events::{MarketEvent, Bar as EventBar, PriceLevel};
use bt_core::events::Timeframe;
use bt_core::types::{Venue, AssetClass, Side};
use chrono::{DateTime, Utc, Duration};
use rust_decimal::Decimal;
use std::collections::HashMap;
use tokio::sync::broadcast;
use uuid::Uuid;

fn rand_i32_decimal(range: std::ops::Range<i32>, scale: u32) -> Decimal {
    Decimal::new(i64::from(fastrand::i32(range)), scale)
}

fn rand_u64_decimal(range: std::ops::Range<u64>, scale: u32) -> Decimal {
    Decimal::new(fastrand::u64(range) as i64, scale)
}

pub struct MockProvider {
    info: ProviderInfo,
    event_tx: broadcast::Sender<MarketEvent>,
    #[allow(dead_code)]
    symbols: Vec<Symbol>,
    base_prices: HashMap<Symbol, rust_decimal::Decimal>,
    connected: bool,
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockProvider {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        let mut base_prices = HashMap::new();
        base_prices.insert(Symbol::new(Venue::Polygon, "AAPL", AssetClass::Equity), rust_decimal::Decimal::new(17500, 2));
        base_prices.insert(Symbol::new(Venue::Polygon, "MSFT", AssetClass::Equity), rust_decimal::Decimal::new(38000, 2));
        base_prices.insert(Symbol::new(Venue::Polygon, "GOOGL", AssetClass::Equity), rust_decimal::Decimal::new(14000, 2));
        base_prices.insert(Symbol::new(Venue::Polygon, "SPY", AssetClass::Equity), rust_decimal::Decimal::new(45000, 2));
        base_prices.insert(Symbol::new(Venue::Polygon, "QQQ", AssetClass::Equity), rust_decimal::Decimal::new(38000, 2));
        base_prices.insert(Symbol::crypto(Venue::Binance, "BTCUSDT"), rust_decimal::Decimal::new(6500000, 2));
        base_prices.insert(Symbol::crypto(Venue::Binance, "ETHUSDT"), rust_decimal::Decimal::new(320000, 2));
        base_prices.insert(Symbol::crypto(Venue::Binance, "SOLUSDT"), rust_decimal::Decimal::new(15000, 2));
        base_prices.insert(Symbol::new(Venue::Nse, "RELIANCE", AssetClass::Equity), rust_decimal::Decimal::new(290000, 2)); // ₹2,900.00
        base_prices.insert(Symbol::new(Venue::Nse, "TCS", AssetClass::Equity), rust_decimal::Decimal::new(410000, 2)); // ₹4,100.00
        base_prices.insert(Symbol::new(Venue::Bse, "HDFCBANK", AssetClass::Equity), rust_decimal::Decimal::new(165000, 2)); // ₹1,650.00
        base_prices.insert(Symbol::crypto(Venue::CoinDcx, "BTCINR"), rust_decimal::Decimal::new(580000000, 2)); // ₹58,00,000.00

        Self {
            info: ProviderInfo {
                name: "Mock".to_string(),
                venue: Venue::Simulator,
                asset_classes: vec![AssetClass::Equity, AssetClass::Crypto],
                supports_streaming: true,
                supports_historical: true,
                supports_orderbook: true,
                rate_limit: RateLimitInfo {
                    requests_per_second: 1000,
                    burst: 100,
                    websocket_connections: 10,
                },
            },
            event_tx,
            symbols: Vec::new(),
            base_prices,
            connected: false,
        }
    }

    pub fn with_symbols(mut self, symbols: Vec<Symbol>) -> Self {
        for symbol in &symbols {
            if !self.base_prices.contains_key(symbol) {
                self.base_prices.insert(symbol.clone(), rust_decimal::Decimal::new(10000, 2));
            }
        }
        self
    }

    fn generate_quote(&self, symbol: &Symbol) -> Quote {
        let base = *self.base_prices.get(symbol).unwrap_or(&rust_decimal::Decimal::new(10000, 2));
        let spread = base * rust_decimal::Decimal::new(1, 4); // 1bp spread
        let bid = base - spread / Decimal::new(2, 0);
        let ask = base + spread / Decimal::new(2, 0);

        Quote {
            symbol: symbol.clone(),
            bid_price: bid,
            bid_size: rust_decimal::Decimal::new(1000, 0),
            ask_price: ask,
            ask_size: rust_decimal::Decimal::new(1000, 0),
            timestamp: Utc::now(),
            venue: symbol.venue,
        }
    }

    fn generate_trade(&self, symbol: &Symbol) -> Trade {
        let base = *self.base_prices.get(symbol).unwrap_or(&rust_decimal::Decimal::new(10000, 2));
        let price = base * (rust_decimal::Decimal::new(10000, 4) +
            rand_i32_decimal(-10..10, 4)) / rust_decimal::Decimal::new(10000, 4);

        Trade {
            symbol: symbol.clone(),
            price,
            size: rand_u64_decimal(100..10000, 0),
            side: if fastrand::bool() { Some(Side::Buy) } else { Some(Side::Sell) },
            timestamp: Utc::now(),
            venue: symbol.venue,
            trade_id: Uuid::new_v4().to_string(),
            conditions: vec![],
        }
    }

    #[allow(dead_code)]
    fn generate_bar(&self, symbol: &Symbol, timeframe: Timeframe) -> EventBar {
        let base = *self.base_prices.get(symbol).unwrap_or(&rust_decimal::Decimal::new(10000, 2));
        let _vol = base * rand_i32_decimal(-50..50, 4) / rust_decimal::Decimal::new(10000, 4);
        let open = base;
        let high = base + base * rand_u64_decimal(0..100, 4) / rust_decimal::Decimal::new(10000, 4);
        let low = base - base * rand_u64_decimal(0..100, 4) / rust_decimal::Decimal::new(10000, 4);
        let close = base + base * rand_i32_decimal(-50..50, 4) / rust_decimal::Decimal::new(10000, 4);

        EventBar {
            symbol: symbol.clone(),
            timeframe,
            open,
            high,
            low,
            close,
            volume: rand_u64_decimal(10000..1000000, 0),
            vwap: Some((high + low + close) / Decimal::new(3, 0)),
            trade_count: Some(fastrand::u64(100..10000)),
            timestamp: Utc::now(),
            venue: symbol.venue,
        }
    }

    fn generate_orderbook(&self, symbol: &Symbol) -> OrderBook {
        let base = *self.base_prices.get(symbol).unwrap_or(&rust_decimal::Decimal::new(10000, 2));
        let spread = base * rust_decimal::Decimal::new(1, 4);
        let mut bids = Vec::new();
        let mut asks = Vec::new();

        for i in 0..10 {
            bids.push(PriceLevel {
                price: base - spread / Decimal::new(2, 0) - base * rust_decimal::Decimal::new(i as i64 * 2, 4) / rust_decimal::Decimal::new(10000, 4),
                size: rand_u64_decimal(100..10000, 0),
                order_count: Some(fastrand::u32(1..50)),
            });
            asks.push(PriceLevel {
                price: base + spread / Decimal::new(2, 0) + base * rust_decimal::Decimal::new(i as i64 * 2, 4) / rust_decimal::Decimal::new(10000, 4),
                size: rand_u64_decimal(100..10000, 0),
                order_count: Some(fastrand::u32(1..50)),
            });
        }

        OrderBook {
            symbol: symbol.clone(),
            bids,
            asks,
            timestamp: Utc::now(),
            venue: symbol.venue,
        }
    }
}

#[async_trait]
impl MarketDataProvider for MockProvider {
    fn info(&self) -> ProviderInfo {
        self.info.clone()
    }

    async fn connect(&mut self) -> Result<()> {
        self.connected = true;
        let _ = self.event_tx.send(MarketEvent::Status(ConnectionStatus::Connected));
        tracing::info!("Mock provider connected");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.connected = false;
        let _ = self.event_tx.send(MarketEvent::Status(ConnectionStatus::Disconnected));
        Ok(())
    }

    async fn subscribe(&self, symbols: &[Symbol]) -> Result<Subscription> {
        let event_rx = self.event_tx.subscribe();
        Ok(Subscription {
            symbols: symbols.to_vec(),
            event_rx,
        })
    }

    async fn unsubscribe(&self, _symbols: &[Symbol]) -> Result<()> {
        Ok(())
    }

    fn events(&self) -> broadcast::Receiver<MarketEvent> {
        self.event_tx.subscribe()
    }

    fn connection_status(&self) -> ConnectionStatus {
        if self.connected {
            ConnectionStatus::Connected
        } else {
            ConnectionStatus::Disconnected
        }
    }

    async fn health_check(&self) -> Result<HealthStatus> {
        Ok(HealthStatus {
            healthy: self.connected,
            latency_ms: 1,
            last_message: Some(Utc::now()),
            messages_per_second: 100.0,
            errors_last_minute: 0,
            reconnect_count: 0,
        })
    }
}

#[async_trait]
impl HistoricalDataProvider for MockProvider {
    async fn get_bars(&self, request: BarsRequest) -> Result<Vec<Bar>> {
        let mut bars = Vec::new();
        let mut current = request.start;
        let mut secs = request.timeframe.as_seconds() as i64;
        if secs == 0 {
            secs = 1;
        }
        let interval = Duration::seconds(secs);

        while current < request.end && bars.len() < request.limit.unwrap_or(1000) {
            let base = *self.base_prices.get(&request.symbol).unwrap_or(&rust_decimal::Decimal::new(10000, 2));
            let _vol = base * rand_i32_decimal(-50..50, 4) / rust_decimal::Decimal::new(10000, 4);
            let open = base;
            let high = base + base * rand_u64_decimal(0..100, 4) / rust_decimal::Decimal::new(10000, 4);
            let low = base - base * rand_u64_decimal(0..100, 4) / rust_decimal::Decimal::new(10000, 4);
            let close = base + base * rand_i32_decimal(-50..50, 4) / rust_decimal::Decimal::new(10000, 4);

            bars.push(Bar {
                symbol: request.symbol.clone(),
                timeframe: request.timeframe,
                open,
                high,
                low,
                close,
                volume: rand_u64_decimal(10000..1000000, 0),
                vwap: Some((high + low + close) / Decimal::new(3, 0)),
                trade_count: Some(fastrand::u64(100..10000)),
                timestamp: current,
                venue: request.symbol.venue,
            });

            current += interval;
        }

        Ok(bars)
    }

    async fn get_latest_bar(&self, symbol: &Symbol, timeframe: Timeframe) -> Result<Option<Bar>> {
        let base = *self.base_prices.get(symbol).unwrap_or(&rust_decimal::Decimal::new(10000, 2));
        let _vol = base * rand_i32_decimal(-50..50, 4) / rust_decimal::Decimal::new(10000, 4);
        let open = base;
        let high = base + base * rand_u64_decimal(0..100, 4) / rust_decimal::Decimal::new(10000, 4);
        let low = base - base * rand_u64_decimal(0..100, 4) / rust_decimal::Decimal::new(10000, 4);
        let close = base + base * rand_i32_decimal(-50..50, 4) / rust_decimal::Decimal::new(10000, 4);

        Ok(Some(Bar {
            symbol: symbol.clone(),
            timeframe,
            open,
            high,
            low,
            close,
            volume: rand_u64_decimal(10000..1000000, 0),
            vwap: Some((high + low + close) / Decimal::new(3, 0)),
            trade_count: Some(fastrand::u64(100..10000)),
            timestamp: Utc::now(),
            venue: symbol.venue,
        }))
    }

    async fn get_quotes(&self, symbols: &[Symbol]) -> Result<HashMap<Symbol, Quote>> {
        let mut quotes = HashMap::new();
        for symbol in symbols {
            quotes.insert(symbol.clone(), self.generate_quote(symbol));
        }
        Ok(quotes)
    }

    async fn get_trades(&self, symbol: &Symbol, _start: DateTime<Utc>, _end: DateTime<Utc>, limit: usize) -> Result<Vec<Trade>> {
        let mut trades = Vec::new();
        for _ in 0..limit {
            trades.push(self.generate_trade(symbol));
        }
        Ok(trades)
    }

    async fn get_order_book(&self, symbol: &Symbol, _depth: usize) -> Result<Option<OrderBook>> {
        Ok(Some(self.generate_orderbook(symbol)))
    }

    async fn search_symbols(&self, query: &str) -> Result<Vec<Symbol>> {
        let mut results = Vec::new();
        for symbol in self.base_prices.keys() {
            if symbol.ticker.to_lowercase().contains(&query.to_lowercase()) {
                results.push(symbol.clone());
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl NewsProvider for MockProvider {
    async fn get_news(&self, _symbols: Option<&[Symbol]>, limit: usize) -> Result<Vec<bt_core::events::NewsItem>> {
        let mut news = Vec::new();
        let headlines = [
            "Market rallies on Fed pivot hopes",
            "Tech earnings beat expectations",
            "Oil prices surge on supply concerns",
            "Crypto regulation bill advances in Senate",
            "Yield curve steepens on growth data",
        ];
        #[allow(clippy::needless_range_loop)]
        for i in 0..limit.min(headlines.len()) {
            news.push(bt_core::events::NewsItem {
                id: Uuid::new_v4().to_string(),
                headline: headlines[i].to_string(),
                summary: Some(format!("Summary for {}", headlines[i])),
                url: Some("https://example.com".to_string()),
                source: "Mock News".to_string(),
                symbols: vec![],
                timestamp: Utc::now() - chrono::Duration::hours(i as i64),
                categories: vec!["market".to_string()],
            });
        }
        Ok(news)
    }

    async fn subscribe_news(&self, _symbols: &[Symbol]) -> Result<broadcast::Receiver<bt_core::events::NewsItem>> {
        let (_tx, rx) = broadcast::channel(100);
        Ok(rx)
    }
}
