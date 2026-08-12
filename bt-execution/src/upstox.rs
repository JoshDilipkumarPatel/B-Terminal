use async_trait::async_trait;
use crate::broker::*;
use bt_core::{ExecutionEvent, OrderId};
use bt_core::types::*;
use tokio::sync::broadcast;
use serde::{Deserialize, Serialize};

/// Upstox API v2 adapter.
/// This is currently a stub implementation. The actual Upstox API v2
/// uses OAuth 2.0 access tokens and standard REST endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstoxConfig {
    pub api_key: String,
    pub access_token: String,
    pub base_url: String,
}

pub struct UpstoxAdapter {
    #[allow(dead_code)]
    config: UpstoxConfig,
    #[allow(dead_code)]
    client: reqwest::Client,
    event_tx: broadcast::Sender<ExecutionEvent>,
    connected: std::sync::atomic::AtomicBool,
}

impl UpstoxAdapter {
    pub fn new(config: UpstoxConfig) -> Self {
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
}

#[async_trait]
impl BrokerAdapter for UpstoxAdapter {
    fn broker_type(&self) -> BrokerType {
        BrokerType::Upstox
    }

    fn name(&self) -> &str {
        "Upstox"
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
        // TODO: Implement Upstox order placement
        Ok(order.id)
    }

    async fn cancel_order(&self, _order_id: OrderId) -> anyhow::Result<()> {
        crate::broker::enforce_stub_broker_safety(self.name())?;
        // TODO: Implement Upstox order cancellation
        Ok(())
    }

    async fn cancel_all_orders(&self) -> anyhow::Result<()> {
        crate::broker::enforce_stub_broker_safety(self.name())?;
        Ok(())
    }

    async fn get_order(&self, _order_id: OrderId) -> anyhow::Result<Option<Order>> {
        Ok(None)
    }

    async fn get_open_orders(&self) -> anyhow::Result<Vec<Order>> {
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
    async fn test_upstox_stub_safety_and_paper_introspection() {
        let mut adapter = UpstoxAdapter::new(UpstoxConfig {
            api_key: "key".to_string(),
            access_token: "token".to_string(),
            base_url: "http://localhost".to_string(),
        });
        assert!(adapter.is_paper(), "Stub adapters must return true for is_paper()");
        assert!(adapter.get_account().await.is_ok(), "Read-only introspection must succeed");
        assert!(adapter.get_positions().await.is_ok(), "Read-only introspection must succeed");
        assert!(adapter.health_check().await.is_ok(), "Health check must succeed");

        if !cfg!(feature = "stub-brokers") {
            assert!(adapter.connect().await.is_err(), "Must reject live connect attempt without explicit feature flag");
            assert!(adapter.cancel_all_orders().await.is_err(), "Must reject order modification attempts");
        }
    }
}
