use crate::types::{Account, Fill, OrderId, Position, Side, Symbol};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketEvent {
    Quote(Quote),
    Trade(Trade),
    Bar(Bar),
    OrderBook(OrderBook),
    News(NewsItem),
    Status(ConnectionStatus),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quote {
    pub symbol: Symbol,
    pub bid_price: Decimal,
    pub bid_size: Decimal,
    pub ask_price: Decimal,
    pub ask_size: Decimal,
    pub timestamp: DateTime<Utc>,
    pub venue: crate::types::Venue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub symbol: Symbol,
    pub price: Decimal,
    pub size: Decimal,
    pub side: Option<Side>,
    pub timestamp: DateTime<Utc>,
    pub venue: crate::types::Venue,
    pub trade_id: String,
    pub conditions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bar {
    pub symbol: Symbol,
    pub timeframe: Timeframe,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub vwap: Option<Decimal>,
    pub trade_count: Option<u64>,
    pub timestamp: DateTime<Utc>,
    pub venue: crate::types::Venue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Timeframe {
    Tick,
    Second,
    Minute,
    Minute5,
    Minute15,
    Minute30,
    Hour,
    Hour4,
    Day,
    Week,
    Month,
}

impl Timeframe {
    pub fn as_seconds(&self) -> u64 {
        match self {
            Timeframe::Tick => 1,
            Timeframe::Second => 1,
            Timeframe::Minute => 60,
            Timeframe::Minute5 => 300,
            Timeframe::Minute15 => 900,
            Timeframe::Minute30 => 1800,
            Timeframe::Hour => 3600,
            Timeframe::Hour4 => 14400,
            Timeframe::Day => 86400,
            Timeframe::Week => 604800,
            Timeframe::Month => 2592000,
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "tick" | "t" => Some(Timeframe::Tick),
            "1s" | "second" => Some(Timeframe::Second),
            "1m" | "minute" => Some(Timeframe::Minute),
            "5m" | "5min" => Some(Timeframe::Minute5),
            "15m" | "15min" => Some(Timeframe::Minute15),
            "30m" | "30min" => Some(Timeframe::Minute30),
            "1h" | "hour" => Some(Timeframe::Hour),
            "4h" | "4hour" => Some(Timeframe::Hour4),
            "1d" | "day" => Some(Timeframe::Day),
            "1w" | "week" => Some(Timeframe::Week),
            "1mo" | "month" => Some(Timeframe::Month),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    pub symbol: Symbol,
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
    pub timestamp: DateTime<Utc>,
    pub venue: crate::types::Venue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: Decimal,
    pub size: Decimal,
    pub order_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsItem {
    pub id: String,
    pub headline: String,
    pub summary: Option<String>,
    pub url: Option<String>,
    pub source: String,
    pub symbols: Vec<Symbol>,
    pub timestamp: DateTime<Utc>,
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Reconnecting,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignalEvent {
    Entry(SignalEntry),
    Exit(SignalExit),
    Adjust(SignalAdjust),
    Cancel(String), // strategy_id
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalEntry {
    pub strategy_id: String,
    pub signal_id: Uuid,
    pub symbol: Symbol,
    pub side: Side,
    pub quantity: Decimal,
    pub confidence: f64, // 0.0 - 1.0
    pub entry_price: Option<Decimal>,
    pub stop_loss: Option<Decimal>,
    pub take_profit: Option<Decimal>,
    pub timestamp: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalExit {
    pub strategy_id: String,
    pub signal_id: Uuid,
    pub symbol: Symbol,
    pub reason: ExitReason,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExitReason {
    StopLoss,
    TakeProfit,
    SignalReversal,
    TimeExit,
    RiskLimit,
    Manual,
    StrategyDisabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalAdjust {
    pub strategy_id: String,
    pub signal_id: Uuid,
    pub symbol: Symbol,
    pub new_quantity: Decimal,
    pub reason: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionEvent {
    OrderAcknowledged(OrderAck),
    OrderFilled(OrderFill),
    OrderPartialFill(OrderPartialFill),
    OrderCancelled(OrderId),
    OrderRejected(OrderReject),
    OrderExpired(OrderId),
    PositionUpdate(Position),
    AccountUpdate(Account),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderAck {
    pub order_id: Uuid,
    pub client_order_id: String,
    pub broker_order_id: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderFill {
    pub order_id: Uuid,
    pub fill: Fill,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderPartialFill {
    pub order_id: Uuid,
    pub fill: Fill,
    pub remaining: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderReject {
    pub order_id: Uuid,
    pub client_order_id: String,
    pub reason: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskEvent {
    LimitBreached(RiskLimitBreach),
    KillSwitchActivated(KillSwitchEvent),
    PositionLimitExceeded(PositionLimitEvent),
    DailyLossLimitExceeded(DailyLossEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskLimitBreach {
    pub limit_type: RiskLimitType,
    pub current_value: Decimal,
    pub limit_value: Decimal,
    pub symbol: Option<Symbol>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLimitType {
    DailyLoss,
    MaxDrawdown,
    PositionSize,
    SectorConcentration,
    Correlation,
    Leverage,
    OpenOrders,
    OrderSize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KillSwitchEvent {
    pub reason: KillReason,
    pub timestamp: DateTime<Utc>,
    pub positions_flattened: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KillReason {
    DailyLossLimit,
    MaxDrawdown,
    Manual,
    SystemError,
    ConnectionLost,
    RiskLimitBreach,
    FatFinger5Sigma,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionLimitEvent {
    pub symbol: Symbol,
    pub current_size: Decimal,
    pub limit: Decimal,
    pub action_taken: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyLossEvent {
    pub current_loss: Decimal,
    pub limit: Decimal,
    pub timestamp: DateTime<Utc>,
}

pub type EventSender<T> = broadcast::Sender<T>;
pub type EventReceiver<T> = broadcast::Receiver<T>;

pub fn create_event_bus<T: Clone + Send + Sync + 'static>(capacity: usize) -> (EventSender<T>, EventReceiver<T>) {
    broadcast::channel(capacity)
}
