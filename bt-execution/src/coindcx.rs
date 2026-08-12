use async_trait::async_trait;
use crate::broker::*;
use bt_core::{ExecutionEvent, OrderId};
use bt_core::types::*;
use tokio::sync::broadcast;
use serde::{Deserialize, Serialize};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use uuid::Uuid;
use std::fmt;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Serialize, Deserialize)]
pub struct CoinDcxConfig {
    pub api_key: String,
    pub api_secret: String,
    pub base_url: String,
}

impl fmt::Debug for CoinDcxConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CoinDcxConfig")
            .field("api_key", &"[REDACTED]")
            .field("api_secret", &"[REDACTED]")
            .field("base_url", &self.base_url)
            .finish()
    }
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
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("Failed to build HTTP client"),
            event_tx,
            connected: std::sync::atomic::AtomicBool::new(false),
        }
    }

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
        true
    }

    async fn connect(&mut self) -> anyhow::Result<()> {
        crate::broker::enforce_stub_broker_safety(self.name())?;
        self.connected.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    async fn disconnect(&mut self) -> anyhow::Result<()> {
        self.connected.store(false, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    async fn place_order(&self, order: Order) -> anyhow::Result<OrderId> {
        crate::broker::enforce_stub_broker_safety(self.name())?;
        let url = format!("{}/exchange/v1/orders/create", self.config.base_url);
        let payload = serde_json::to_string(&order).unwrap_or_else(|_| "{}".to_string());
        let signature = self.generate_signature(&payload);

        let response = self.client
            .post(&url)
            .header("X-AUTH-APIKEY", &self.config.api_key)
            .header("X-AUTH-SIGNATURE", &signature)
            .header("Content-Type", "application/json")
            .body(payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("CoinDCX order placement failed: {}", error_text);
        }

        // Parse order ID from response
        let json: serde_json::Value = response.json().await?;
        let order_id = json.get("id").and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok()).unwrap_or(order.id);
        Ok(order_id)
    }

    async fn cancel_order(&self, order_id: OrderId) -> anyhow::Result<()> {
        crate::broker::enforce_stub_broker_safety(self.name())?;
        let url = format!("{}/exchange/v1/orders/cancel", self.config.base_url);
        let payload = serde_json::json!({ "id": order_id }).to_string();
        let signature = self.generate_signature(&payload);

        let response = self.client
            .post(&url)
            .header("X-AUTH-APIKEY", &self.config.api_key)
            .header("X-AUTH-SIGNATURE", &signature)
            .header("Content-Type", "application/json")
            .body(payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("CoinDCX order cancellation failed: {}", error_text);
        }
        Ok(())
    }

    async fn cancel_all_orders(&self) -> anyhow::Result<()> {
        crate::broker::enforce_stub_broker_safety(self.name())?;
        let url = format!("{}/exchange/v1/orders/cancel_all", self.config.base_url);
        let payload = "{}".to_string();
        let signature = self.generate_signature(&payload);

        let response = self.client
            .post(&url)
            .header("X-AUTH-APIKEY", &self.config.api_key)
            .header("X-AUTH-SIGNATURE", &signature)
            .header("Content-Type", "application/json")
            .body(payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("CoinDCX cancel all orders failed: {}", error_text);
        }
        Ok(())
    }

    async fn get_order(&self, _order_id: OrderId) -> anyhow::Result<Option<Order>> {
        Ok(None)
    }

    async fn get_open_orders(&self) -> anyhow::Result<Vec<Order>> {
        // Return mock data for monitoring - no auth needed for read-only introspection
        Ok(vec![])
    }

    async fn get_order_history(&self, _limit: usize) -> anyhow::Result<Vec<Order>> {
        Ok(vec![])
    }

    async fn get_positions(&self) -> anyhow::Result<Vec<Position>> {
        // Return mock data for monitoring - no auth needed for read-only introspection
        Ok(vec![])
    }

    async fn get_position(&self, _symbol: &Symbol) -> anyhow::Result<Option<Position>> {
        Ok(None)
    }

    async fn get_account(&self) -> anyhow::Result<Account> {
        // Return mock data for monitoring - no auth needed for read-only introspection
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_coindcx_stub_safety_and_paper_introspection() {
        let mut adapter = CoinDcxAdapter::new(CoinDcxConfig {
            api_key: "key".to_string(),
            api_secret: "secret".to_string(),
            base_url: "http://localhost".to_string(),
        });
        assert!(adapter.is_paper(), "Stub adapters must return true for is_paper() to avoid being mistaken for live brokers");
        assert!(adapter.get_account().await.is_ok(), "Read-only introspection must succeed without throwing safety lockouts");
        assert!(adapter.get_positions().await.is_ok(), "Read-only introspection must succeed for monitoring");
        assert!(adapter.health_check().await.is_ok(), "Health check must succeed for dashboard monitoring");

        let sig = adapter.generate_signature("test_payload");
        assert!(!sig.is_empty(), "HMAC signature generation must work properly");

        if !cfg!(feature = "stub-brokers") {
            assert!(adapter.connect().await.is_err(), "Must reject live connect attempt without explicit feature flag");
            assert!(adapter.cancel_all_orders().await.is_err(), "Must reject order modification attempts");
        }
    }
}

