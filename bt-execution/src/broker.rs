use async_trait::async_trait;
use bt_core::{ExecutionEvent, OrderId};
use bt_core::types::{Order, Symbol, Position, Account};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerConfig {
    pub name: String,
    pub broker_type: BrokerType,
    pub paper_trading: bool,
    pub credentials: BrokerCredentials,
    pub endpoints: BrokerEndpoints,
    pub rate_limits: HashMap<String, crate::RateLimitConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrokerType {
    Alpaca,
    InteractiveBrokers,
    Binance,
    Coinbase,
    Bybit,
    Simulator,
    Groww,
    CoinDcx,
    AngelOne,
    Zerodha,
    Upstox,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BrokerCredentials {
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub passphrase: Option<String>,
    pub account_id: Option<String>,
}

impl std::fmt::Debug for BrokerCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrokerCredentials")
            .field("api_key", &self.api_key.as_ref().map(|_| "***"))
            .field("api_secret", &self.api_secret.as_ref().map(|_| "***"))
            .field("passphrase", &self.passphrase.as_ref().map(|_| "***"))
            .field("account_id", &self.account_id)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerEndpoints {
    pub rest: String,
    pub websocket: Option<String>,
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            broker_type: BrokerType::Simulator,
            paper_trading: true,
            credentials: BrokerCredentials::default(),
            endpoints: BrokerEndpoints::default(),
            rate_limits: HashMap::new(),
        }
    }
}

impl Default for BrokerCredentials {
    fn default() -> Self {
        Self {
            api_key: None,
            api_secret: None,
            passphrase: None,
            account_id: None,
        }
    }
}

impl Default for BrokerEndpoints {
    fn default() -> Self {
        Self {
            rest: "https://api.alpaca.markets".to_string(),
            websocket: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub requests_per_second: u32,
    pub burst: u32,
}

#[derive(Debug, Clone)]
pub struct BrokerAccountInfo {
    pub id: String,
    pub name: String,
    pub account_type: AccountType,
    pub status: AccountStatus,
    pub currency: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountType {
    Cash,
    Margin,
    PortfolioMargin,
    Crypto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountStatus {
    Active,
    Inactive,
    Restricted,
    Closed,
}

#[async_trait]
pub trait BrokerAdapter: Send + Sync {
    fn broker_type(&self) -> BrokerType;
    fn name(&self) -> &str;
    fn is_paper(&self) -> bool;

    async fn connect(&mut self) -> anyhow::Result<()>;
    async fn disconnect(&mut self) -> anyhow::Result<()>;

    async fn place_order(&self, order: Order) -> anyhow::Result<OrderId>;
    async fn cancel_order(&self, order_id: OrderId) -> anyhow::Result<()>;
    async fn cancel_all_orders(&self) -> anyhow::Result<()>;

    async fn get_order(&self, order_id: OrderId) -> anyhow::Result<Option<Order>>;
    async fn get_open_orders(&self) -> anyhow::Result<Vec<Order>>;
    async fn get_order_history(&self, limit: usize) -> anyhow::Result<Vec<Order>>;

    async fn get_positions(&self) -> anyhow::Result<Vec<Position>>;
    async fn get_position(&self, symbol: &Symbol) -> anyhow::Result<Option<Position>>;

    async fn get_account(&self) -> anyhow::Result<Account>;
    async fn get_accounts(&self) -> anyhow::Result<Vec<BrokerAccountInfo>>;

    fn events(&self) -> broadcast::Receiver<ExecutionEvent>;

    async fn health_check(&self) -> anyhow::Result<BrokerHealth>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerHealth {
    pub healthy: bool,
    pub latency_ms: u64,
    pub last_order_ms: Option<u64>,
    pub error_rate: f64,
    pub connection_status: String,
}