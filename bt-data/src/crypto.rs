use crate::provider::{BarsRequest, Bar, Quote, Trade, OrderBook, HistoricalDataProvider, MarketDataProvider, ProviderInfo, ConnectionStatus, RateLimitInfo, HealthStatus, Subscription, Symbol, NewsProvider};
use anyhow::{Result, Context};
use async_trait::async_trait;
use bt_core::events::{MarketEvent, Quote as EventQuote, Trade as EventTrade, Bar as EventBar, PriceLevel, NewsItem};
use bt_core::events::Timeframe;
use bt_core::types::{Venue, AssetClass, Side};
use chrono::{DateTime, Utc, Duration};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::broadcast;
use tokio::time::Duration as TokioDuration;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures::StreamExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinanceConfig {
    pub ws_url: String,
    pub rest_url: String,
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub testnet: bool,
}

impl Default for BinanceConfig {
    fn default() -> Self {
        Self {
            ws_url: "wss://stream.binance.com:9443/ws".to_string(),
            rest_url: "https://api.binance.com".to_string(),
            api_key: None,
            api_secret: None,
            testnet: false,
        }
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct BinanceStreamMessage {
    stream: Option<String>,
    data: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code, non_snake_case)]
struct BinanceBookTicker {
    u: u64,
    s: String,
    b: String,
    B: String,
    a: String,
    A: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code, non_snake_case)]
struct BinanceTrade {
    e: String,
    E: i64,
    s: String,
    t: u64,
    p: String,
    q: String,
    b: u64,
    a: u64,
    T: i64,
    m: bool,
    M: bool,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code, non_snake_case)]
struct BinanceKline {
    e: String,
    E: i64,
    s: String,
    k: BinanceKlineData,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code, non_snake_case)]
struct BinanceKlineData {
    t: i64,
    T: i64,
    s: String,
    i: String,
    f: u64,
    L: u64,
    o: String,
    c: String,
    h: String,
    l: String,
    v: String,
    n: u64,
    x: bool,
    q: String,
    V: String,
    Q: String,
    B: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code, non_snake_case)]
struct BinanceDepth {
    lastUpdateId: u64,
    bids: Vec<Vec<String>>,
    asks: Vec<Vec<String>>,
}

pub struct BinanceProvider {
    config: BinanceConfig,
    client: reqwest::Client,
    event_tx: broadcast::Sender<MarketEvent>,
    subscribed_symbols: Vec<Symbol>,
    ws_handles: Vec<tokio::task::JoinHandle<()>>,
    connected: bool,
}

impl BinanceProvider {
    pub fn new(config: BinanceConfig) -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        let client = reqwest::Client::builder()
            .timeout(TokioDuration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            client,
            event_tx,
            subscribed_symbols: Vec::new(),
            ws_handles: Vec::new(),
            connected: false,
        }
    }

    fn binance_symbol(&self, symbol: &Symbol) -> String {
        symbol.ticker.to_lowercase()
    }

    #[allow(clippy::wrong_self_convention)]
    fn from_binance_symbol(&self, sym: &str) -> Symbol {
        Symbol::crypto(Venue::Binance, sym.to_uppercase())
    }

    async fn start_streams(&mut self) -> Result<()> {
        if self.subscribed_symbols.is_empty() {
            return Ok(());
        }

        let streams: Vec<String> = self.subscribed_symbols.iter()
            .flat_map(|s| {
                let sym = self.binance_symbol(s);
                vec![
                    format!("{}@bookTicker", sym),
                    format!("{}@trade", sym),
                    format!("{}@kline_1m", sym),
                    format!("{}@depth20@100ms", sym),
                ]
            })
            .collect();

        let url = format!("{}/stream?streams={}", self.config.ws_url, streams.join("/"));
        let (ws_stream, _) = connect_async(&url).await
            .context("Failed to connect to Binance WebSocket")?;

        let (_write, mut read) = ws_stream.split();

        let event_tx = self.event_tx.clone();
        let handle = tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(msg) = serde_json::from_str::<BinanceStreamMessage>(&text) {
                            if let Some(data) = msg.data.as_object() {
                                if let Some(e) = data.get("e").and_then(|v| v.as_str()) {
                                    match e {
                                        "bookTicker" | "bt" => {
                                            if let Ok(ticker) = serde_json::from_value::<BinanceBookTicker>(msg.data) {
                                                Self::handle_book_ticker(&event_tx, ticker).await;
                                            }
                                        }
                                        "trade" => {
                                            if let Ok(trade) = serde_json::from_value::<BinanceTrade>(msg.data) {
                                                Self::handle_trade(&event_tx, trade).await;
                                            }
                                        }
                                        "kline" => {
                                            if let Ok(kline) = serde_json::from_value::<BinanceKline>(msg.data) {
                                                Self::handle_kline(&event_tx, kline).await;
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        tracing::warn!("Binance WS closed");
                        break;
                    }
                    Err(e) => {
                        tracing::error!("Binance WS error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
        });

        self.ws_handles.push(handle);
        self.connected = true;
        let _ = self.event_tx.send(MarketEvent::Status(ConnectionStatus::Connected));
        Ok(())
    }

    async fn handle_book_ticker(event_tx: &broadcast::Sender<MarketEvent>, ticker: BinanceBookTicker) {
        let symbol = Symbol::crypto(Venue::Binance, ticker.s.to_uppercase());
        let bid_price = ticker.b.parse().unwrap_or(Decimal::ZERO);
        let bid_size = ticker.B.parse().unwrap_or(Decimal::ZERO);
        let ask_price = ticker.a.parse().unwrap_or(Decimal::ZERO);
        let ask_size = ticker.A.parse().unwrap_or(Decimal::ZERO);

        let event = EventQuote {
            symbol: symbol.clone(),
            bid_price,
            bid_size,
            ask_price,
            ask_size,
            timestamp: Utc::now(),
            venue: Venue::Binance,
        };
        let _ = event_tx.send(MarketEvent::Quote(event));
    }

    async fn handle_trade(event_tx: &broadcast::Sender<MarketEvent>, trade: BinanceTrade) {
        let symbol = Symbol::crypto(Venue::Binance, trade.s.to_uppercase());
        let price = trade.p.parse().unwrap_or(Decimal::ZERO);
        let size = trade.q.parse().unwrap_or(Decimal::ZERO);
        let side = if trade.m { Some(Side::Sell) } else { Some(Side::Buy) };

        let event = EventTrade {
            symbol: symbol.clone(),
            price,
            size,
            side,
            timestamp: DateTime::from_timestamp_millis(trade.T).unwrap_or_else(Utc::now),
            venue: Venue::Binance,
            trade_id: trade.t.to_string(),
            conditions: vec![],
        };
        let _ = event_tx.send(MarketEvent::Trade(event));
    }

    async fn handle_kline(event_tx: &broadcast::Sender<MarketEvent>, kline: BinanceKline) {
        if !kline.k.x { // Only closed klines
            return;
        }
        let symbol = Symbol::crypto(Venue::Binance, kline.k.s.to_uppercase());
        let timeframe = match kline.k.i.as_str() {
            "1m" => Timeframe::Minute,
            "5m" => Timeframe::Minute5,
            "15m" => Timeframe::Minute15,
            "30m" => Timeframe::Minute30,
            "1h" => Timeframe::Hour,
            "4h" => Timeframe::Hour4,
            "1d" => Timeframe::Day,
            "1w" => Timeframe::Week,
            "1M" => Timeframe::Month,
            _ => Timeframe::Minute,
        };

        let volume = kline.k.v.parse().unwrap_or(Decimal::ZERO);
        let quote_volume = kline.k.q.parse().unwrap_or(Decimal::ZERO);
        let vwap = if volume.is_zero() { Decimal::ZERO } else { quote_volume / volume };

        let event = EventBar {
            symbol: symbol.clone(),
            timeframe,
            open: kline.k.o.parse().unwrap_or(Decimal::ZERO),
            high: kline.k.h.parse().unwrap_or(Decimal::ZERO),
            low: kline.k.l.parse().unwrap_or(Decimal::ZERO),
            close: kline.k.c.parse().unwrap_or(Decimal::ZERO),
            volume,
            vwap: Some(vwap),
            trade_count: Some(kline.k.n),
            timestamp: DateTime::from_timestamp_millis(kline.k.t).unwrap_or_else(Utc::now),
            venue: Venue::Binance,
        };
        let _ = event_tx.send(MarketEvent::Bar(event));
    }
}

#[async_trait]
impl MarketDataProvider for BinanceProvider {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            name: "Binance".to_string(),
            venue: Venue::Binance,
            asset_classes: vec![AssetClass::Crypto],
            supports_streaming: true,
            supports_historical: true,
            supports_orderbook: true,
            rate_limit: RateLimitInfo {
                requests_per_second: 20,
                burst: 50,
                websocket_connections: 5,
            },
        }
    }

    async fn connect(&mut self) -> Result<()> {
        if !self.connected {
            self.start_streams().await?;
        }
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        for handle in self.ws_handles.drain(..) {
            handle.abort();
        }
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
        let start = std::time::Instant::now();
        let _ = self.client.get(format!("{}/api/v3/ping", self.config.rest_url)).send().await?;
        let latency = start.elapsed().as_millis() as u64;

        Ok(HealthStatus {
            healthy: self.connected,
            latency_ms: latency,
            last_message: Some(Utc::now()),
            messages_per_second: 0.0,
            errors_last_minute: 0,
            reconnect_count: 0,
        })
    }
}

#[async_trait]
impl HistoricalDataProvider for BinanceProvider {
    async fn get_bars(&self, request: BarsRequest) -> Result<Vec<Bar>> {
        let symbol = self.binance_symbol(&request.symbol);
        let interval = match request.timeframe {
            Timeframe::Minute => "1m",
            Timeframe::Minute5 => "5m",
            Timeframe::Minute15 => "15m",
            Timeframe::Minute30 => "30m",
            Timeframe::Hour => "1h",
            Timeframe::Hour4 => "4h",
            Timeframe::Day => "1d",
            Timeframe::Week => "1w",
            Timeframe::Month => "1M",
            _ => "1m",
        };

        let start_ts = request.start.timestamp_millis();
        let end_ts = request.end.timestamp_millis();
        let start_ts = start_ts.to_string();
        let end_ts = end_ts.to_string();
        let limit = request.limit.unwrap_or(1000).to_string();

        let url = format!("{}/api/v3/klines", self.config.rest_url);
        let response = self.client
            .get(&url)
            .query(&[
                ("symbol", symbol.as_str()),
                ("interval", interval),
                ("startTime", start_ts.as_str()),
                ("endTime", end_ts.as_str()),
                ("limit", limit.as_str()),
            ])
            .send()
            .await?;

        #[derive(Deserialize)]
        struct KlineResponse(
            Vec<(
                i64, // open time
                String, // open
                String, // high
                String, // low
                String, // close
                String, // volume
                i64, // close time
                String, // quote asset volume
                u64, // number of trades
                String, // taker buy base asset volume
                String, // taker buy quote asset volume
                String, // ignore
            )>
        );

        let data: KlineResponse = response.json().await?;
        let mut bars = Vec::new();

        for k in data.0 {
            let volume = k.5.parse().unwrap_or(Decimal::ZERO);
            let quote_volume = k.7.parse().unwrap_or(Decimal::ZERO);
            let vwap = if volume.is_zero() { Decimal::ZERO } else { quote_volume / volume };

            bars.push(Bar {
                symbol: request.symbol.clone(),
                timeframe: request.timeframe,
                open: k.1.parse().unwrap_or(Decimal::ZERO),
                high: k.2.parse().unwrap_or(Decimal::ZERO),
                low: k.3.parse().unwrap_or(Decimal::ZERO),
                close: k.4.parse().unwrap_or(Decimal::ZERO),
                volume,
                vwap: Some(vwap),
                trade_count: Some(k.8),
                timestamp: DateTime::from_timestamp_millis(k.0).unwrap_or_else(Utc::now),
                venue: request.symbol.venue,
            });
        }

        Ok(bars)
    }

    async fn get_latest_bar(&self, symbol: &Symbol, timeframe: Timeframe) -> Result<Option<Bar>> {
        let end = Utc::now();
        let start = end - Duration::days(2);
        let bars = self.get_bars(BarsRequest {
            symbol: symbol.clone(),
            timeframe,
            start,
            end,
            limit: Some(1),
        }).await?;
        Ok(bars.into_iter().last())
    }

    async fn get_quotes(&self, symbols: &[Symbol]) -> Result<HashMap<Symbol, Quote>> {
        let mut quotes = HashMap::new();
        let syms: Vec<String> = symbols.iter().map(|s| self.binance_symbol(s)).collect();

        let url = format!("{}/api/v3/ticker/bookTicker", self.config.rest_url);
        let response = self.client.get(&url).send().await?;

        #[derive(Deserialize)]
        #[allow(non_snake_case)]
        struct BookTicker {
            symbol: String,
            bidPrice: String,
            bidQty: String,
            askPrice: String,
            askQty: String,
        }

        let data: Vec<BookTicker> = response.json().await?;

        for ticker in data {
            if syms.contains(&ticker.symbol.to_lowercase()) {
                let symbol = self.from_binance_symbol(&ticker.symbol);
                quotes.insert(symbol.clone(), Quote {
                    symbol: symbol.clone(),
                    bid_price: ticker.bidPrice.parse().unwrap_or(Decimal::ZERO),
                    bid_size: ticker.bidQty.parse().unwrap_or(Decimal::ZERO),
                    ask_price: ticker.askPrice.parse().unwrap_or(Decimal::ZERO),
                    ask_size: ticker.askQty.parse().unwrap_or(Decimal::ZERO),
                    timestamp: Utc::now(),
                    venue: Venue::Binance,
                });
            }
        }

        Ok(quotes)
    }

    async fn get_trades(&self, symbol: &Symbol, start: DateTime<Utc>, end: DateTime<Utc>, limit: usize) -> Result<Vec<Trade>> {
        let binance_sym = self.binance_symbol(symbol);
        let start_ts = start.timestamp() * 1000;
        let end_ts = end.timestamp() * 1000;

        let url = format!("{}/api/v3/aggTrades", self.config.rest_url);
        let response = self.client
            .get(&url)
            .query(&[
                ("symbol", &binance_sym),
                ("startTime", &start_ts.to_string()),
                ("endTime", &end_ts.to_string()),
                ("limit", &limit.to_string()),
            ])
            .send()
            .await?;

        #[derive(Deserialize)]
        #[allow(dead_code, non_snake_case)]
        struct AggTrade {
            a: u64,
            p: String,
            q: String,
            f: u64,
            l: u64,
            T: i64,
            m: bool,
        }

        let data: Vec<AggTrade> = response.json().await?;
        let mut trades = Vec::new();

        for t in data {
            trades.push(Trade {
                symbol: symbol.clone(),
                price: t.p.parse().unwrap_or(Decimal::ZERO),
                size: t.q.parse().unwrap_or(Decimal::ZERO),
                side: if t.m { Some(Side::Sell) } else { Some(Side::Buy) },
                timestamp: DateTime::from_timestamp_millis(t.T).unwrap_or_else(Utc::now),
                venue: Venue::Binance,
                trade_id: t.a.to_string(),
                conditions: vec![],
            });
        }

        Ok(trades)
    }

    async fn get_order_book(&self, symbol: &Symbol, depth: usize) -> Result<Option<OrderBook>> {
        let binance_sym = self.binance_symbol(symbol);
        let url = format!("{}/api/v3/depth", self.config.rest_url);
        let response = self.client
            .get(&url)
            .query(&[("symbol", &binance_sym), ("limit", &depth.to_string())])
            .send()
            .await?;

        let data: BinanceDepth = response.json().await?;
        let mut bids = Vec::new();
        let mut asks = Vec::new();

        for level in data.bids.into_iter().take(depth) {
            bids.push(PriceLevel {
                price: level[0].parse().unwrap_or(Decimal::ZERO),
                size: level[1].parse().unwrap_or(Decimal::ZERO),
                order_count: None,
            });
        }

        for level in data.asks.into_iter().take(depth) {
            asks.push(PriceLevel {
                price: level[0].parse().unwrap_or(Decimal::ZERO),
                size: level[1].parse().unwrap_or(Decimal::ZERO),
                order_count: None,
            });
        }

        Ok(Some(OrderBook {
            symbol: symbol.clone(),
            bids,
            asks,
            timestamp: Utc::now(),
            venue: Venue::Binance,
        }))
    }

    async fn search_symbols(&self, query: &str) -> Result<Vec<Symbol>> {
        let url = format!("{}/api/v3/exchangeInfo", self.config.rest_url);
        let response = self.client.get(&url).send().await?;

        #[derive(Deserialize)]
        struct ExchangeInfo {
            symbols: Vec<SymbolInfo>,
        }

        #[derive(Deserialize)]
        #[allow(non_snake_case, dead_code)]
        struct SymbolInfo {
            symbol: String,
            status: String,
            baseAsset: String,
            quoteAsset: String,
        }

        let data: ExchangeInfo = response.json().await?;
        let mut results = Vec::new();

        for s in data.symbols {
            if s.status == "TRADING" &&
               s.symbol.to_lowercase().contains(&query.to_lowercase()) {
                results.push(Symbol::crypto(Venue::Binance, s.symbol));
            }
        }

        Ok(results)
    }
}

#[async_trait]
impl NewsProvider for BinanceProvider {
    async fn get_news(&self, _symbols: Option<&[Symbol]>, _limit: usize) -> Result<Vec<NewsItem>> {
        Ok(Vec::new())
    }

    async fn subscribe_news(&self, _symbols: &[Symbol]) -> Result<broadcast::Receiver<NewsItem>> {
        let (_tx, rx) = broadcast::channel(100);
        Ok(rx)
    }
}

pub struct CoinbaseProvider {
    #[allow(dead_code)]
    config: CoinbaseConfig,
    #[allow(dead_code)]
    client: reqwest::Client,
    event_tx: broadcast::Sender<MarketEvent>,
    connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinbaseConfig {
    pub ws_url: String,
    pub rest_url: String,
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub passphrase: Option<String>,
    pub sandbox: bool,
}

impl Default for CoinbaseConfig {
    fn default() -> Self {
        Self {
            ws_url: "wss://ws-feed.exchange.coinbase.com".to_string(),
            rest_url: "https://api.exchange.coinbase.com".to_string(),
            api_key: None,
            api_secret: None,
            passphrase: None,
            sandbox: false,
        }
    }
}

impl CoinbaseProvider {
    pub fn new(config: CoinbaseConfig) -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        let client = reqwest::Client::builder()
            .timeout(TokioDuration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            client,
            event_tx,
            connected: false,
        }
    }

    #[allow(dead_code)]
    fn coinbase_symbol(&self, symbol: &Symbol) -> String {
        if symbol.ticker.len() < 4 {
            symbol.ticker.clone()
        } else {
            format!("{}-{}", &symbol.ticker[..symbol.ticker.len()-4], &symbol.ticker[symbol.ticker.len()-4..])
        }
    }
}

#[async_trait]
impl MarketDataProvider for CoinbaseProvider {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            name: "Coinbase".to_string(),
            venue: Venue::Coinbase,
            asset_classes: vec![AssetClass::Crypto],
            supports_streaming: true,
            supports_historical: true,
            supports_orderbook: true,
            rate_limit: RateLimitInfo {
                requests_per_second: 10,
                burst: 30,
                websocket_connections: 1,
            },
        }
    }

    async fn connect(&mut self) -> Result<()> {
        // Implementation similar to Binance
        self.connected = true;
        let _ = self.event_tx.send(MarketEvent::Status(ConnectionStatus::Connected));
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.connected = false;
        let _ = self.event_tx.send(MarketEvent::Status(ConnectionStatus::Disconnected));
        Ok(())
    }

    async fn subscribe(&self, symbols: &[Symbol]) -> Result<Subscription> {
        Ok(Subscription {
            symbols: symbols.to_vec(),
            event_rx: self.event_tx.subscribe(),
        })
    }

    async fn unsubscribe(&self, _symbols: &[Symbol]) -> Result<()> {
        Ok(())
    }

    fn events(&self) -> broadcast::Receiver<MarketEvent> {
        self.event_tx.subscribe()
    }

    fn connection_status(&self) -> ConnectionStatus {
        if self.connected { ConnectionStatus::Connected } else { ConnectionStatus::Disconnected }
    }

    async fn health_check(&self) -> Result<HealthStatus> {
        Ok(HealthStatus { healthy: self.connected, latency_ms: 0, last_message: Some(Utc::now()), messages_per_second: 0.0, errors_last_minute: 0, reconnect_count: 0 })
    }
}

#[async_trait]
impl HistoricalDataProvider for CoinbaseProvider {
    async fn get_bars(&self, request: BarsRequest) -> Result<Vec<Bar>> {
        // Coinbase candles endpoint
        let _ = request;
        Ok(Vec::new())
    }

    async fn get_latest_bar(&self, _symbol: &Symbol, _timeframe: Timeframe) -> Result<Option<Bar>> {
        Ok(None)
    }

    async fn get_quotes(&self, _symbols: &[Symbol]) -> Result<HashMap<Symbol, Quote>> {
        Ok(HashMap::new())
    }

    async fn get_trades(&self, _symbol: &Symbol, _start: DateTime<Utc>, _end: DateTime<Utc>, _limit: usize) -> Result<Vec<Trade>> {
        Ok(Vec::new())
    }

    async fn get_order_book(&self, _symbol: &Symbol, _depth: usize) -> Result<Option<OrderBook>> {
        Ok(None)
    }

    async fn search_symbols(&self, _query: &str) -> Result<Vec<Symbol>> {
        Ok(Vec::new())
    }
}

#[async_trait]
impl NewsProvider for CoinbaseProvider {
    async fn get_news(&self, _symbols: Option<&[Symbol]>, _limit: usize) -> Result<Vec<NewsItem>> {
        Ok(Vec::new())
    }

    async fn subscribe_news(&self, _symbols: &[Symbol]) -> Result<broadcast::Receiver<NewsItem>> {
        let (_tx, rx) = broadcast::channel(100);
        Ok(rx)
    }
}
