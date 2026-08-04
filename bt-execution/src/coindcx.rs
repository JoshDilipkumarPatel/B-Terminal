use async_trait::async_trait;
use crate::broker::*;
use bt_core::{ExecutionEvent, OrderId};
use bt_core::types::*;
use tokio::sync::broadcast;
use serde::{Deserialize, Serialize};
use hmac::{Hmac, Mac};
use sha2::Sha256;

#[allow(dead_code)]
type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinDcxConfig {
    pub api_key: String,
    pub api_secret: String,
    pub base_url: String,
}

pub struct CoinDcxAdapter {
    config: CoinDcxConfig,
    #[allow(dead_code)]
    client: reqwest::Client,
    event_tx: broadcast::Sender<ExecutionEvent>,
    connected: std::sync::atomic::AtomicBool,
}

impl CoinDcxAdapter {
    pub fn new(config: CoinDcxConfig) -> Self {
        let (event_tx, _) = broadcast::channel(100);
        Self {
            config,
            client: reqwest::Client::new(),
            event_tx,
            connected: std::sync::atomic::AtomicBool::new(false),
        }
    }

    #[allow(dead_code)]
    fn generate_signature(&self, body: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(self.config.api_secret.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(body.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }
}

#[async_trait]
impl BrokerAdapter for CoinDcxAdapter {
    fn broker_type(&self) -> BrokerType {
        BrokerType::CoinDcx
    }

    fn name(&self) -> &str {
        "CoinDcx"
    }

    fn is_paper(&self) -> bool {
        false
    }

    async fn connect(&mut self) -> anyhow::Result<()> {
        self.connected.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    async fn disconnect(&mut self) -> anyhow::Result<()> {
        self.connected.store(false, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    async fn place_order(&self, order: Order) -> anyhow::Result<OrderId> {
        let _url = format!("{}/exchange/v1/orders/create", self.config.base_url);
        // Symbol mapping logic
        // B-Terminal COINDCX:BTCINR -> B-BTC_INR
        // Perform order placement with HMAC signature headers X-AUTH-APIKEY and X-AUTH-SIGNATURE
        Ok(order.id)
    }

    async fn cancel_order(&self, _order_id: OrderId) -> anyhow::Result<()> {
        let _url = format!("{}/exchange/v1/orders/cancel", self.config.base_url);
        Ok(())
    }

    async fn cancel_all_orders(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn get_order(&self, _order_id: OrderId) -> anyhow::Result<Option<Order>> {
        Ok(None)
    }

    async fn get_open_orders(&self) -> anyhow::Result<Vec<Order>> {
        let _url = format!("{}/exchange/v1/orders/active_orders", self.config.base_url);
        Ok(vec![])
    }

    async fn get_order_history(&self, _limit: usize) -> anyhow::Result<Vec<Order>> {
        Ok(vec![])
    }

    async fn get_positions(&self) -> anyhow::Result<Vec<Position>> {
        Ok(vec![])
    }

    async fn get_position(&self, _symbol: &Symbol) -> anyhow::Result<Option<Position>> {
        Ok(None)
    }

    async fn get_account(&self) -> anyhow::Result<Account> {
        let _url = format!("{}/exchange/v1/users/balances", self.config.base_url);
        Ok(Account {
            id: uuid::Uuid::new_v4(),
            equity: Decimal::ZERO,
            cash: Decimal::ZERO,
            buying_power: Decimal::ZERO,
            initial_margin: Decimal::ZERO,
            maintenance_margin: Decimal::ZERO,
            day_trading_buying_power: Decimal::ZERO,
            long_market_value: Decimal::ZERO,
            short_market_value: Decimal::ZERO,
            currency: "INR".to_string(),
            updated_at: chrono::Utc::now(),
        })
    }

    async fn get_accounts(&self) -> anyhow::Result<Vec<BrokerAccountInfo>> {
        Ok(vec![])
    }

    fn events(&self) -> broadcast::Receiver<ExecutionEvent> {
        self.event_tx.subscribe()
    }

    async fn health_check(&self) -> anyhow::Result<BrokerHealth> {
        Ok(BrokerHealth {
            healthy: self.connected.load(std::sync::atomic::Ordering::SeqCst),
            latency_ms: 0,
            last_order_ms: None,
            error_rate: 0.0,
            connection_status: "connected".to_string(),
        })
    }
}
