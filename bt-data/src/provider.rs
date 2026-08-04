//! Data feed provider traits and common types

use async_trait::async_trait;
pub use bt_core::events::{Bar, ConnectionStatus, MarketEvent, NewsItem, OrderBook, PriceLevel, Quote, Timeframe, Trade};
pub use bt_core::types::Symbol;
use bt_core::types::Venue;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use std::collections::HashMap;

#[derive(Debug)]
pub struct Subscription {
    pub symbols: Vec<Symbol>,
    pub event_rx: broadcast::Receiver<MarketEvent>,
}

#[derive(Debug, Clone)]
pub struct BarsRequest {
    pub symbol: Symbol,
    pub timeframe: Timeframe,
    pub start: chrono::DateTime<chrono::Utc>,
    pub end: chrono::DateTime<chrono::Utc>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub name: String,
    pub venue: Venue,
    pub asset_classes: Vec<bt_core::types::AssetClass>,
    pub supports_streaming: bool,
    pub supports_historical: bool,
    pub supports_orderbook: bool,
    pub rate_limit: RateLimitInfo,
}

#[derive(Debug, Clone)]
pub struct RateLimitInfo {
    pub requests_per_second: u32,
    pub burst: u32,
    pub websocket_connections: u32,
}

#[async_trait]
pub trait MarketDataProvider: Send + Sync {
    fn info(&self) -> ProviderInfo;

    async fn connect(&mut self) -> anyhow::Result<()>;

    async fn disconnect(&mut self) -> anyhow::Result<()>;

    async fn subscribe(&self, symbols: &[Symbol]) -> anyhow::Result<Subscription>;

    async fn unsubscribe(&self, symbols: &[Symbol]) -> anyhow::Result<()>;

    fn events(&self) -> broadcast::Receiver<MarketEvent>;

    fn connection_status(&self) -> ConnectionStatus;

    async fn health_check(&self) -> anyhow::Result<HealthStatus>;
}

#[async_trait]
pub trait HistoricalDataProvider: Send + Sync {
    async fn get_bars(&self, request: BarsRequest) -> anyhow::Result<Vec<Bar>>;

    async fn get_latest_bar(&self, symbol: &Symbol, timeframe: Timeframe) -> anyhow::Result<Option<Bar>>;

    async fn get_quotes(&self, symbols: &[Symbol]) -> anyhow::Result<HashMap<Symbol, Quote>>;

    async fn get_trades(&self, symbol: &Symbol, start: chrono::DateTime<chrono::Utc>, end: chrono::DateTime<chrono::Utc>, limit: usize) -> anyhow::Result<Vec<Trade>>;

    async fn get_order_book(&self, symbol: &Symbol, depth: usize) -> anyhow::Result<Option<OrderBook>>;

    async fn search_symbols(&self, query: &str) -> anyhow::Result<Vec<Symbol>>;
}

#[async_trait]
pub trait NewsProvider: Send + Sync {
    async fn get_news(&self, symbols: Option<&[Symbol]>, limit: usize) -> anyhow::Result<Vec<NewsItem>>;

    async fn subscribe_news(&self, symbols: &[Symbol]) -> anyhow::Result<broadcast::Receiver<NewsItem>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub healthy: bool,
    pub latency_ms: u64,
    pub last_message: Option<chrono::DateTime<chrono::Utc>>,
    pub messages_per_second: f64,
    pub errors_last_minute: u32,
    pub reconnect_count: u32,
}
