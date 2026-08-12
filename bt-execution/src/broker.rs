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
#[derive(Default)]
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

impl BrokerCredentials {
    /// Validates presence of required authentication secrets and rejects empty string values (Finding #2 & Item 7)
    pub fn validate_required(&self, require_key: bool, require_secret: bool, require_passphrase: bool) -> anyhow::Result<()> {
        let check_field = |val: &Option<String>, required: bool, name: &str| -> anyhow::Result<()> {
            match val {
                None if required => anyhow::bail!("Security Alert [P0]: Missing required {} in BrokerCredentials", name),
                Some(s) if s.trim().is_empty() => anyhow::bail!("Security Alert [P0]: Explicitly provided {} in BrokerCredentials cannot be empty string", name),
                _ => Ok(()),
            }
        };
        check_field(&self.api_key, require_key, "api_key")?;
        check_field(&self.api_secret, require_secret, "api_secret")?;
        check_field(&self.passphrase, require_passphrase, "passphrase")?;
        Ok(())
    }
}

/// Enforces safety lockouts on stub broker implementations to prevent production state mismatch (Finding #1)
pub fn enforce_stub_broker_safety(broker_name: &str) -> anyhow::Result<()> {
    if !cfg!(feature = "stub-brokers") {
        anyhow::bail!("SECURITY ALERT [P0]: Stub broker adapter ('{}') is prohibited from live trading execution! Enable paper trading mode or compile with `--features stub-brokers`.", broker_name);
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_broker_credentials_validation() {
        let empty_cred = BrokerCredentials {
            api_key: None,
            api_secret: Some("secret".to_string()),
            passphrase: None,
            account_id: None,
        };
        assert!(empty_cred.validate_required(true, false, false).is_err(), "Must fail validation when required API key is missing");
        assert!(empty_cred.validate_required(false, true, false).is_ok(), "Must pass when only checking provided secret");

        let empty_str_cred = BrokerCredentials {
            api_key: Some("".to_string()),
            api_secret: Some("secret".to_string()),
            passphrase: None,
            account_id: None,
        };
        assert!(empty_str_cred.validate_required(false, true, false).is_err(), "Must reject explicitly provided empty string even if require_key=false");
    }

    #[test]
    fn test_stub_broker_safety_lockout() {
        if !cfg!(feature = "stub-brokers") {
            let res = enforce_stub_broker_safety("Zerodha");
            assert!(res.is_err(), "Must throw security error when engaging stub adapter without explicit feature flag");
            assert!(res.unwrap_err().to_string().contains("SECURITY ALERT [P0]"));
        }
    }
}