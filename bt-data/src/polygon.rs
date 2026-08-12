use crate::provider::{BarsRequest, Bar, Quote, Trade, OrderBook, HistoricalDataProvider, MarketDataProvider, ProviderInfo, ConnectionStatus, RateLimitInfo, HealthStatus, Subscription, Symbol, NewsProvider};
use anyhow::{Result, Context, bail};
use async_trait::async_trait;
use bt_core::events::{MarketEvent, Quote as EventQuote, Trade as EventTrade, NewsItem};
use bt_core::events::Timeframe;
use bt_core::types::{Venue, AssetClass};
use chrono::{DateTime, Utc, Duration};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use tokio::sync::broadcast;
use tokio::time::Duration as TokioDuration;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures::{SinkExt, StreamExt};

const POLYGON_WS_URL: &str = "wss://socket.polygon.io/stocks";
const POLYGON_REST_URL: &str = "https://api.polygon.io";

#[derive(Clone, Serialize, Deserialize)]
pub struct PolygonConfig {
    pub api_key: String,
    pub ws_url: String,
    pub rest_url: String,
    pub feed: String, // "sip" or "iex"
}

impl fmt::Debug for PolygonConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PolygonConfig")
            .field("api_key", &"[REDACTED]")
            .field("ws_url", &self.ws_url)
            .field("rest_url", &self.rest_url)
            .field("feed", &self.feed)
            .finish()
    }
}

impl Default for PolygonConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            ws_url: POLYGON_WS_URL.to_string(),
            rest_url: POLYGON_REST_URL.to_string(),
            feed: "sip".to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct PolygonWsMessage {
    ev: String,
    #[serde(flatten)]
    data: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct PolygonQuote {
    sym: String,
    bp: Decimal,
    bs: u64,
    ap: Decimal,
    as_: u64,
    t: i64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct PolygonTrade {
    sym: String,
    p: Decimal,
    s: u64,
    t: i64,
    x: u32,
    i: String,
    c: Vec<u32>,
}

#[derive(Debug, Deserialize)]
struct PolygonAgg {
    o: Decimal,
    h: Decimal,
    l: Decimal,
    c: Decimal,
    v: u64,
    vw: Decimal,
    n: u64,
    t: i64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code, non_snake_case)]
struct PolygonAggResponse {
    results: Option<Vec<PolygonAgg>>,
    status: String,
    ticker: String,
    queryCount: u32,
    resultsCount: u32,
    request_id: String,
}

pub struct PolygonProvider {
    config: PolygonConfig,
    client: reqwest::Client,
    event_tx: broadcast::Sender<MarketEvent>,
    subscribed_symbols: Vec<Symbol>,
    ws_handle: Option<tokio::task::JoinHandle<()>>,
    connected: bool,
    reconnect_attempts: u32,
}

impl PolygonProvider {
    pub fn new(config: PolygonConfig) -> Self {
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
            ws_handle: None,
            connected: false,
            reconnect_attempts: 0,
        }
    }

    fn normalize_symbol(&self, symbol: &Symbol) -> String {
        match symbol.venue {
            Venue::Polygon => symbol.ticker.clone(),
            _ => format!("{}:{}", symbol.venue as u8, symbol.ticker),
        }
    }

    #[allow(dead_code)]
    fn polygon_to_symbol(&self, ticker: &str) -> Symbol {
        Symbol::new(Venue::Polygon, ticker, AssetClass::Equity)
    }

    async fn start_websocket(&mut self) -> Result<()> {
        tracing::debug!("Connecting to Polygon WebSocket (API key redacted)");
        let url = format!("{}?apikey={}", self.config.ws_url, self.config.api_key);
        let (ws_stream, _) = connect_async(&url).await
            .context("Failed to connect to Polygon WebSocket")?;

        let (mut write, mut read) = ws_stream.split();

        // Subscribe to symbols
        let symbols: Vec<String> = self.subscribed_symbols.iter()
            .map(|s| self.normalize_symbol(s))
            .collect();

        if !symbols.is_empty() {
            let msg = serde_json::json!({
                "action": "subscribe",
                "params": symbols.join(",")
            });
            write.send(Message::Text(msg.to_string())).await
                .context("Failed to send subscribe message")?;
        }

        let event_tx = self.event_tx.clone();
        let handle = tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(msgs) = serde_json::from_str::<Vec<PolygonWsMessage>>(&text) {
                            for msg in msgs {
                                if let Err(e) = Self::handle_ws_message(&event_tx, msg).await {
                                    tracing::error!("Error handling WS message: {}", e);
                                }
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        tracing::warn!("Polygon WS closed");
                        break;
                    }
                    Err(e) => {
                        tracing::error!("Polygon WS error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
        });

        self.ws_handle = Some(handle);
        self.connected = true;
        self.reconnect_attempts = 0;
        let _ = self.event_tx.send(MarketEvent::Status(ConnectionStatus::Connected));
        Ok(())
    }

    async fn handle_ws_message(
        event_tx: &broadcast::Sender<MarketEvent>,
        msg: PolygonWsMessage,
    ) -> Result<()> {
        match msg.ev.as_str() {
            "Q" | "q" => { // Quote
                let quote: PolygonQuote = serde_json::from_value(msg.data)?;
                let symbol = Symbol::new(Venue::Polygon, quote.sym, AssetClass::Equity);
                let event = EventQuote {
                    symbol: symbol.clone(),
                    bid_price: quote.bp,
                    bid_size: Decimal::from(quote.bs),
                    ask_price: quote.ap,
                    ask_size: Decimal::from(quote.as_),
                    timestamp: DateTime::from_timestamp_millis(quote.t).unwrap_or_else(Utc::now),
                    venue: Venue::Polygon,
                };
                let _ = event_tx.send(MarketEvent::Quote(event));
            }
            "T" | "t" => { // Trade
                let trade: PolygonTrade = serde_json::from_value(msg.data)?;
                let symbol = Symbol::new(Venue::Polygon, trade.sym, AssetClass::Equity);
                let event = EventTrade {
                    symbol: symbol.clone(),
                    price: trade.p,
                    size: Decimal::from(trade.s),
                    side: None,
                    timestamp: DateTime::from_timestamp_millis(trade.t).unwrap_or_else(Utc::now),
                    venue: Venue::Polygon,
                    trade_id: trade.i,
                    conditions: trade.c.iter().map(|c| c.to_string()).collect(),
                };
                let _ = event_tx.send(MarketEvent::Trade(event));
            }
            "A" | "a" | "AM" => { // Aggregate/Bar
                let _agg: PolygonAgg = serde_json::from_value(msg.data)?;
                // We need the symbol - in practice this comes from subscription context
                // For now we'll skip as we need symbol mapping
            }
            "status" => {
                tracing::info!("Polygon status: {:?}", msg.data);
            }
            _ => {
                tracing::debug!("Unhandled Polygon event: {}", msg.ev);
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
    async fn reconnect(&mut self) -> Result<()> {
        if self.reconnect_attempts >= 10 {
            bail!("Max reconnect attempts reached");
        }
        self.reconnect_attempts += 1;
        tracing::info!("Reconnecting to Polygon (attempt {})...", self.reconnect_attempts);
        tokio::time::sleep(TokioDuration::from_secs(2_u64.pow(self.reconnect_attempts.min(5)))).await;

        if let Some(handle) = self.ws_handle.take() {
            handle.abort();
        }
        self.start_websocket().await
    }
}

#[async_trait]
impl MarketDataProvider for PolygonProvider {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            name: "Polygon".to_string(),
            venue: Venue::Polygon,
            asset_classes: vec![AssetClass::Equity],
            supports_streaming: true,
            supports_historical: true,
            supports_orderbook: true,
            rate_limit: RateLimitInfo {
                requests_per_second: 5,
                burst: 10,
                websocket_connections: 1,
            },
        }
    }

    async fn connect(&mut self) -> Result<()> {
        if !self.connected {
            self.start_websocket().await?;
        }
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        if let Some(handle) = self.ws_handle.take() {
            handle.abort();
        }
        self.connected = false;
        let _ = self.event_tx.send(MarketEvent::Status(ConnectionStatus::Disconnected));
        Ok(())
    }

    async fn subscribe(&self, symbols: &[Symbol]) -> Result<Subscription> {
        let event_rx = self.event_tx.subscribe();
        // In practice, we'd send subscribe message via WS
        // For now, just return the receiver
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
        let _ = self.client
            .get(format!("{}/v1/marketstatus/now", self.config.rest_url))
            .query(&[("apikey", &self.config.api_key)])
            .send()
            .await?;
        let latency = start.elapsed().as_millis() as u64;

        Ok(HealthStatus {
            healthy: self.connected,
            latency_ms: latency,
            last_message: Some(Utc::now()),
            messages_per_second: 0.0,
            errors_last_minute: 0,
            reconnect_count: self.reconnect_attempts,
        })
    }
}

#[async_trait]
impl HistoricalDataProvider for PolygonProvider {
    async fn get_bars(&self, request: BarsRequest) -> Result<Vec<Bar>> {
        let symbol = self.normalize_symbol(&request.symbol);
        let multiplier = 1;
        let timespan = match request.timeframe {
            Timeframe::Minute => "minute",
            Timeframe::Minute5 => "minute",
            Timeframe::Minute15 => "minute",
            Timeframe::Minute30 => "minute",
            Timeframe::Hour => "hour",
            Timeframe::Hour4 => "hour",
            Timeframe::Day => "day",
            Timeframe::Week => "week",
            Timeframe::Month => "month",
            _ => "day",
        };

        let from = request.start.format("%Y-%m-%d").to_string();
        let to = request.end.format("%Y-%m-%d").to_string();

        let url = format!(
            "{}/v2/aggs/ticker/{}/range/{}/{}/{}/{}",
            self.config.rest_url, symbol, multiplier, timespan, from, to
        );

        let response = self.client
            .get(&url)
            .query(&[("apikey", self.config.api_key.as_str()), ("adjusted", "true"), ("sort", "asc")])
            .send()
            .await?;

        let data: PolygonAggResponse = response.json().await?;
        let mut bars = Vec::new();

        if let Some(results) = data.results {
            for agg in results {
                let timestamp = DateTime::from_timestamp_millis(agg.t)
                    .unwrap_or_else(Utc::now);

                bars.push(Bar {
                    symbol: request.symbol.clone(),
                    timeframe: request.timeframe,
                    open: agg.o,
                    high: agg.h,
                    low: agg.l,
                    close: agg.c,
                    volume: Decimal::from(agg.v),
                    vwap: Some(agg.vw),
                    trade_count: Some(agg.n),
                    timestamp,
                    venue: request.symbol.venue,
                });
            }
        }

        if let Some(limit) = request.limit {
            bars.truncate(limit);
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
        let quotes = HashMap::new();
        // Use last trade endpoint for latest quote
        // In production, use snapshot endpoint
        for symbol in symbols {
            // Placeholder - would call snapshot API
            let _ = symbol;
        }
        Ok(quotes)
    }

    async fn get_trades(&self, symbol: &Symbol, start: DateTime<Utc>, end: DateTime<Utc>, limit: usize) -> Result<Vec<Trade>> {
        let normalized = self.normalize_symbol(symbol);
        let from = start.format("%Y-%m-%d").to_string();
        let to = end.format("%Y-%m-%d").to_string();
        let limit_param = limit.to_string();

        let url = format!(
            "{}/v3/trades/{}",
            self.config.rest_url, normalized
        );

        let response = self.client
            .get(&url)
            .query(&[
                ("apikey", self.config.api_key.as_str()),
                ("timestamp.gte", from.as_str()),
                ("timestamp.lte", to.as_str()),
                ("limit", limit_param.as_str()),
                ("sort", "asc"),
            ])
            .send()
            .await?;

        #[derive(Deserialize)]
        struct TradesResponse {
            results: Vec<PolygonTrade>,
        }

        let data: TradesResponse = response.json().await?;
        let mut trades = Vec::new();

        for trade in data.results.into_iter().take(limit) {
            trades.push(Trade {
                symbol: symbol.clone(),
                price: trade.p,
                size: Decimal::from(trade.s),
                side: None,
                timestamp: DateTime::from_timestamp_millis(trade.t).unwrap_or_else(Utc::now),
                venue: Venue::Polygon,
                trade_id: trade.i,
                conditions: trade.c.iter().map(|c| c.to_string()).collect(),
            });
        }

        Ok(trades)
    }

    async fn get_order_book(&self, symbol: &Symbol, depth: usize) -> Result<Option<OrderBook>> {
        // Would use level 2 snapshot endpoint
        // Placeholder implementation
        let _ = depth;
        let _ = symbol;
        Ok(None)
    }

    async fn search_symbols(&self, query: &str) -> Result<Vec<Symbol>> {
        let url = format!("{}/v3/reference/tickers", self.config.rest_url);
        let response = self.client
            .get(&url)
            .query(&[("apikey", self.config.api_key.as_str()), ("search", query), ("active", "true"), ("limit", "50")])
            .send()
            .await?;

        #[derive(Deserialize)]
        struct SearchResponse {
            results: Vec<SearchResult>,
        }

        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct SearchResult {
            ticker: String,
            name: String,
            market: String,
            #[serde(rename = "type")]
            type_: String,
        }

        let data: SearchResponse = response.json().await?;
        let mut results = Vec::new();

        for r in data.results {
            results.push(Symbol::new(Venue::Polygon, r.ticker, AssetClass::Equity));
        }

        Ok(results)
    }
}

#[async_trait]
impl NewsProvider for PolygonProvider {
    async fn get_news(&self, symbols: Option<&[Symbol]>, limit: usize) -> Result<Vec<NewsItem>> {
        let url = format!("{}/v2/reference/news", self.config.rest_url);
        let limit = limit.to_string();
        let tickers;
        let mut query = vec![("apikey", self.config.api_key.as_str()), ("limit", limit.as_str())];

        if let Some(syms) = symbols {
            tickers = syms.iter().map(|s| self.normalize_symbol(s)).collect::<Vec<_>>().join(",");
            query.push(("ticker", tickers.as_str()));
        }

        let response = self.client.get(&url).query(&query).send().await?;

        #[derive(Deserialize)]
        struct NewsResponse {
            results: Vec<NewsResult>,
        }

        #[derive(Deserialize)]
        struct NewsResult {
            id: String,
            title: String,
            article_url: String,
            publisher: Publisher,
            published_utc: String,
            tickers: Vec<String>,
        }

        #[derive(Deserialize)]
        struct Publisher {
            name: String,
        }

        let data: NewsResponse = response.json().await?;
        let mut news = Vec::new();

        for item in data.results {
            news.push(NewsItem {
                id: item.id,
                headline: item.title,
                summary: None,
                url: Some(item.article_url),
                source: item.publisher.name,
                symbols: item.tickers.iter()
                    .map(|t| Symbol::new(Venue::Polygon, t.clone(), AssetClass::Equity))
                    .collect(),
                timestamp: DateTime::parse_from_rfc3339(&item.published_utc)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                categories: vec!["news".to_string()],
            });
        }

        Ok(news)
    }

    async fn subscribe_news(&self, _symbols: &[Symbol]) -> Result<broadcast::Receiver<NewsItem>> {
        let (_tx, rx) = broadcast::channel(100);
        Ok(rx)
    }
}
