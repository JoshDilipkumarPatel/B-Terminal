use crate::broker::BrokerAdapter;
use crate::idempotency_store::IdempotencyStore;
use bt_core::{ExecutionEvent, OrderReject, OrderId, RiskEvent};
use bt_core::types::{Order, OrderStatus, OrderType, Side, Symbol, Position, Account, TimeInForce};
use bt_core::risk_limits::RiskManager;
use bt_core::kill_switch::GlobalKillSwitch;
use bt_core::events::OrderBook;
use bt_data::orderbook_aggregator::{MarketCircuitState, OrderBookAnalyzer};
use chrono::{DateTime, Utc, Duration as ChronoDuration};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio::time::{interval, Duration as TokioDuration};
use tracing::{debug, info, warn};
use serde::{Serialize, Deserialize};

/// Deduplication window for idempotency keys (5 minutes).
const IDEMPOTENCY_TTL_SECONDS: i64 = 300;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OMSConfig {
    pub max_orders_per_second: u32,
    pub order_timeout_ms: u64,
    pub max_retry_attempts: u32,
    pub retry_delay_ms: u64,
    pub enable_pre_trade_risk: bool,
    pub enable_post_trade_risk: bool,
    pub reconcile_interval_sec: u64,
    /// Persist idempotency keys to SQLite so the 300s dedup window survives restarts.
    #[serde(default = "default_true")]
    pub enable_idempotency_persistence: bool,
    /// SQLite database path for the persistent idempotency store.
    #[serde(default)]
    pub idempotency_db_path: PathBuf,
}

impl Default for OMSConfig {
    fn default() -> Self {
        Self {
            max_orders_per_second: 100,
            order_timeout_ms: 5000,
            max_retry_attempts: 3,
            retry_delay_ms: 1000,
            enable_pre_trade_risk: true,
            enable_post_trade_risk: true,
            reconcile_interval_sec: 30,
            enable_idempotency_persistence: true,
            idempotency_db_path: PathBuf::from("data/idempotency.sqlite"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OrderTracking {
    order: Order,
    broker_id: Option<String>,
    broker_name: String,
    #[allow(dead_code)]
    retry_count: u32,
    submitted_at: DateTime<Utc>,
    acknowledged: bool,
}

pub struct OrderManagementSystem {
    config: OMSConfig,
    brokers: Arc<RwLock<HashMap<String, Arc<dyn BrokerAdapter>>>>,
    default_broker: String,
    risk_manager: Option<Arc<RiskManager>>,
    kill_switch: Option<Arc<GlobalKillSwitch>>,
    order_tracking: Arc<RwLock<HashMap<OrderId, OrderTracking>>>,
    idempotency_cache: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
    // Interior-mutable so it can be lazily initialized (constructor is sync, store init is async).
    idempotency_store: Arc<RwLock<Option<Arc<IdempotencyStore>>>>,
    event_tx: broadcast::Sender<ExecutionEvent>,
    risk_event_tx: broadcast::Sender<RiskEvent>,
    circuit_breaker_state: Arc<RwLock<MarketCircuitState>>,
    circuit_breaker_normal_ticks: Arc<RwLock<u32>>,
    running: Arc<RwLock<bool>>,
}

impl OrderManagementSystem {
    pub fn new(exec_config: bt_core::config::ExecutionConfig) -> Self {
        let mut oms_config = OMSConfig {
            enable_idempotency_persistence: exec_config.idempotency.persistence_enabled,
            ..Default::default()
        };
        if let Some(db_path) = &exec_config.idempotency.db_path {
            oms_config.idempotency_db_path = db_path.clone();
        }
        Self::new_with_components(
            oms_config,
            exec_config.default_broker,
            None,
            None,
        )
    }

    pub fn new_with_components(
        config: OMSConfig,
        default_broker: String,
        risk_manager: Option<Arc<RiskManager>>,
        kill_switch: Option<Arc<GlobalKillSwitch>>,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        let (risk_event_tx, _) = broadcast::channel(1000);

        Self {
            config,
            brokers: Arc::new(RwLock::new(HashMap::new())),
            default_broker,
            risk_manager,
            kill_switch,
            order_tracking: Arc::new(RwLock::new(HashMap::new())),
            idempotency_cache: Arc::new(RwLock::new(HashMap::new())),
            idempotency_store: Arc::new(RwLock::new(None)),
            event_tx,
            risk_event_tx,
            circuit_breaker_state: Arc::new(RwLock::new(MarketCircuitState::Normal)),
            circuit_breaker_normal_ticks: Arc::new(RwLock::new(0)),
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Feeds incoming Level-2 OrderBook snapshots into the Circuit Breaker.
    /// Manages Halt and Auto-Resume (requires 3 consecutive normal ticks to unhalt).
    pub async fn update_market_circuit(&self, book: &OrderBook, max_spread_bps: u32) {
        let current_health = OrderBookAnalyzer::verify_market_integrity(book, max_spread_bps);
        let mut state = self.circuit_breaker_state.write().await;
        let mut normal_ticks = self.circuit_breaker_normal_ticks.write().await;

        match current_health {
            MarketCircuitState::HaltedOrMalformed(reason) => {
                if *state == MarketCircuitState::Normal {
                    warn!("CIRCUIT BREAKER TRIPPED: {}", reason);
                    *state = MarketCircuitState::HaltedOrMalformed(reason);
                }
                *normal_ticks = 0; // Reset auto-resume counter
            }
            MarketCircuitState::Normal => {
                if *state != MarketCircuitState::Normal {
                    *normal_ticks += 1;
                    if *normal_ticks >= 3 {
                        info!("CIRCUIT BREAKER AUTO-RESUMED: Market normalized for 3 consecutive ticks.");
                        *state = MarketCircuitState::Normal;
                        *normal_ticks = 0;
                    }
                }
            }
        }
    }

    pub async fn add_broker<B: BrokerAdapter + 'static>(&self, mut broker: B) -> anyhow::Result<()> {
        let name = broker.name().to_string();
        let mut brokers = self.brokers.write().await;
        if brokers.contains_key(&name) {
            return Err(anyhow::anyhow!("Broker already exists: {}", name));
        }
        broker.connect().await?;
        info!("Added broker: {}", name);
        brokers.insert(name, Arc::new(broker));
        Ok(())
    }

    pub async fn remove_broker(&self, name: &str) -> anyhow::Result<()> {
        let mut brokers = self.brokers.write().await;
        if let Some(mut broker) = brokers.remove(name) {
            if let Some(b) = Arc::get_mut(&mut broker) {
                let _ = b.disconnect().await;
            }
            info!("Removed broker: {}", name);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Broker not found: {}", name))
        }
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        *self.running.write().await = true;

        // Warm the in-memory cache from the persistent store so a restarted process
        // still rejects duplicate client_order_ids within the 300s window.
        self.ensure_idempotency_store().await;
        if let Some(store) = self.idempotency_store.read().await.as_ref() {
            if let Ok(active) = store.load_active().await {
                let mut cache = self.idempotency_cache.write().await;
                *cache = active;
                info!("Warmed idempotency cache with {} keys", cache.len());
            }
        }

        self.start_event_listeners().await;
        self.start_reconciliation().await;
        info!("OMS started");
        Ok(())
    }

    pub async fn stop(&self) -> anyhow::Result<()> {
        *self.running.write().await = false;
        info!("OMS stopped");
        Ok(())
    }

    /// Lazily initializes the persistent idempotency store if enabled.
    /// Falls back to in-memory dedup on failure (logs a warning, self-heals on retry).
    async fn ensure_idempotency_store(&self) {
        if !self.config.enable_idempotency_persistence {
            return;
        }
        if self.idempotency_store.read().await.is_some() {
            return;
        }
        let db_path = self
            .config
            .idempotency_db_path
            .to_str()
            .unwrap_or("data/idempotency.sqlite");
        match IdempotencyStore::new(db_path, IDEMPOTENCY_TTL_SECONDS).await {
            Ok(store) => {
                info!("Persistent idempotency store enabled at {}", self.config.idempotency_db_path.display());
                *self.idempotency_store.write().await = Some(Arc::new(store));
            }
            Err(e) => {
                warn!("Persistent idempotency store unavailable ({}); using in-memory dedup only", e);
            }
        }
    }

    pub async fn pre_trade_balance_check(
        &self,
        order: &Order,
        estimated_price: rust_decimal::Decimal,
        broker_name: Option<String>,
    ) -> anyhow::Result<rust_decimal::Decimal> {
        let broker_name = broker_name.unwrap_or_else(|| self.default_broker.clone());
        let broker = {
            let brokers = self.brokers.read().await;
            brokers.get(&broker_name).cloned()
        };

        let broker = broker.ok_or_else(|| anyhow::anyhow!("Broker not found: {}", broker_name))?;
        crate::algo_orders::PreTradeCheck::verify_balance(broker.as_ref(), order, estimated_price).await
    }

    pub async fn submit_order(&self, order: Order, broker_name: Option<String>) -> anyhow::Result<OrderId> {
        // Check kill switch
        if let Some(ks) = &self.kill_switch {
            if ks.is_activated().await {
                return Err(anyhow::anyhow!("Kill switch activated"));
            }
        }

        // Circuit Breaker Gate
        {
            let cb_state = self.circuit_breaker_state.read().await;
            if let MarketCircuitState::HaltedOrMalformed(ref reason) = *cb_state {
                return Err(anyhow::anyhow!("Market Circuit Breaker Active - Order Rejected: {}", reason));
            }
        }

        // Idempotency Gate: Duplicate order protection against WebSocket replays and overlapping autopilot cycles.
        // The persistent SQLite store is checked FIRST (atomic INSERT OR IGNORE) so the window survives restarts;
        // the in-memory cache is the warm fast path layered on top.
        if !order.client_order_id.trim().is_empty() {
            self.ensure_idempotency_store().await;
            if let Some(store) = self.idempotency_store.read().await.as_ref() {
                if store.check_and_insert(&order.client_order_id, Utc::now()).await? {
                    let reason = format!("Idempotent Rejection: Duplicate order (persisted) with client_order_id '{}'", order.client_order_id);
                    warn!("{}", reason);
                    return Err(anyhow::anyhow!(reason));
                }
            }

            let mut idump = self.idempotency_cache.write().await;
            // Purge expired signatures (older than IDEMPOTENCY_TTL_SECONDS)
            let now = Utc::now();
            idump.retain(|_, timestamp| (now - *timestamp).num_seconds() < IDEMPOTENCY_TTL_SECONDS);

            if idump.contains_key(&order.client_order_id) {
                let reason = format!("Idempotent Rejection: Duplicate order intercepted with client_order_id '{}'", order.client_order_id);
                warn!("{}", reason);
                return Err(anyhow::anyhow!(reason));
            }
            idump.insert(order.client_order_id.clone(), now);
        }

        // Pre-trade risk check
        if self.config.enable_pre_trade_risk {
            if let Some(rm) = &self.risk_manager {
                let risk_result = rm.validate_order(&order).await;
                if risk_result.is_reject() {
                    let reason = format!("Risk check failed: {:?}", risk_result);
                    let _ = self.event_tx.send(ExecutionEvent::OrderRejected(OrderReject {
                        order_id: order.id,
                        client_order_id: order.client_order_id.clone(),
                        reason: reason.clone(),
                        timestamp: Utc::now(),
                    }));
                    return Err(anyhow::anyhow!(reason));
                }
            }
        }

        // Determine broker
        let broker_name = broker_name.unwrap_or_else(|| self.default_broker.clone());
        let broker = {
            let brokers = self.brokers.read().await;
            brokers.get(&broker_name).cloned()
        };

        let broker = broker.ok_or_else(|| anyhow::anyhow!("Broker not found: {}", broker_name))?;

        // Submit order
        let order_id = broker.place_order(order.clone()).await?;

        // Track order
        let tracking = OrderTracking {
            order,
            broker_id: None,
            broker_name: broker_name.clone(),
            retry_count: 0,
            submitted_at: Utc::now(),
            acknowledged: false,
        };
        self.order_tracking.write().await.insert(order_id, tracking);

        Ok(order_id)
    }

    pub async fn cancel_order(&self, order_id: OrderId) -> anyhow::Result<()> {
        let tracking = {
            let mut tracking = self.order_tracking.write().await;
            tracking.remove(&order_id)
        };

        if let Some(tracking) = tracking {
            let brokers = self.brokers.read().await;
            if let Some(broker) = brokers.get(&tracking.broker_name) {
                broker.cancel_order(order_id).await?;
            }
        } else {
            // Try all brokers
            let brokers = self.brokers.read().await;
            for broker in brokers.values() {
                if broker.cancel_order(order_id).await.is_ok() {
                    return Ok(());
                }
            }
            return Err(anyhow::anyhow!("Order not found: {}", order_id));
        }

        Ok(())
    }

    pub async fn cancel_all_orders(&self) -> anyhow::Result<()> {
        let brokers = self.brokers.read().await;
        for broker in brokers.values() {
            broker.cancel_all_orders().await?;
        }
        Ok(())
    }

    pub async fn flatten_all_positions(&self) -> anyhow::Result<()> {
        self.cancel_all_orders().await?;
        let positions = self.get_positions().await?;
        for pos in positions {
            if pos.quantity.is_zero() {
                continue;
            }
            let side = if pos.quantity > Decimal::ZERO { Side::Sell } else { Side::Buy };
            let order = Order::new(
                pos.symbol.clone(),
                side,
                OrderType::Market,
                pos.quantity.abs(),
            );
            let _ = self.submit_order(order, None).await;
        }
        Ok(())
    }

    pub async fn get_order(&self, order_id: OrderId) -> anyhow::Result<Option<Order>> {
        // Check local tracking first
        let tracking = self.order_tracking.read().await;
        if let Some(t) = tracking.get(&order_id) {
            return Ok(Some(t.order.clone()));
        }

        // Check brokers
        let brokers = self.brokers.read().await;
        for broker in brokers.values() {
            if let Ok(Some(order)) = broker.get_order(order_id).await {
                return Ok(Some(order));
            }
        }

        Ok(None)
    }

    pub async fn get_positions(&self) -> anyhow::Result<Vec<Position>> {
        let brokers = self.brokers.read().await;
        let mut all_positions = Vec::new();
        for broker in brokers.values() {
            if let Ok(positions) = broker.get_positions().await {
                all_positions.extend(positions);
            }
        }
        Ok(all_positions)
    }

    pub async fn get_account(&self, broker_name: Option<String>) -> anyhow::Result<Account> {
        let name = broker_name.unwrap_or_else(|| self.default_broker.clone());
        let brokers = self.brokers.read().await;
        let broker = brokers.get(&name).ok_or_else(|| anyhow::anyhow!("Broker not found: {}", name))?;
        broker.get_account().await
    }

    pub fn events(&self) -> broadcast::Receiver<ExecutionEvent> {
        self.event_tx.subscribe()
    }

    pub fn risk_events(&self) -> broadcast::Receiver<RiskEvent> {
        self.risk_event_tx.subscribe()
    }

    async fn start_event_listeners(&self) {
        let brokers = self.brokers.clone();
        let order_tracking = self.order_tracking.clone();
        let event_tx = self.event_tx.clone();
        let risk_manager = self.risk_manager.clone();
        let _kill_switch = self.kill_switch.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            // Note: Keep track of broker receivers to support dynamically added brokers
            let mut broker_receivers: std::collections::HashMap<String, broadcast::Receiver<ExecutionEvent>> = std::collections::HashMap::new();

            loop {
                if !(*running.read().await) {
                    break;
                }

                // Add any new broker receivers
                {
                    let brokers_read = brokers.read().await;
                    for (name, broker) in brokers_read.iter() {
                        if !broker_receivers.contains_key(name) {
                            broker_receivers.insert(name.clone(), broker.events());
                        }
                    }
                }

                for rx in broker_receivers.values_mut() {
                    while let Ok(event) = rx.try_recv() {
                        // Forward event
                        let _ = event_tx.send(event.clone());

                        // Update tracking
                        match &event {
                            ExecutionEvent::OrderAcknowledged(ack) => {
                                let mut tracking = order_tracking.write().await;
                                if let Some(t) = tracking.get_mut(&ack.order_id) {
                                    t.broker_id = Some(ack.broker_order_id.clone());
                                    t.acknowledged = true;
                                }
                            }
                            ExecutionEvent::OrderFilled(_fill) => {
                                // Update risk manager
                                if let Some(_rm) = &risk_manager {
                                    // Update P&L tracking
                                }
                            }
                            ExecutionEvent::OrderRejected(reject) => {
                                let mut tracking = order_tracking.write().await;
                                if let Some(t) = tracking.get_mut(&reject.order_id) {
                                    t.order.status = OrderStatus::Rejected;
                                }
                            }
                            _ => {}
                        }
                    }
                }

                tokio::time::sleep(TokioDuration::from_millis(10)).await;
            }
        });
    }

    async fn start_reconciliation(&self) {
        let brokers = self.brokers.clone();
        let order_tracking = self.order_tracking.clone();
        let running = self.running.clone();
        let interval_sec = self.config.reconcile_interval_sec;
        let idempotency_store = self.idempotency_store.clone();

        tokio::spawn(async move {
            let mut interval = interval(TokioDuration::from_secs(interval_sec));

            loop {
                interval.tick().await;

                if !(*running.read().await) {
                    break;
                }

                // Reconcile positions
                let brokers_read = brokers.read().await;
                for (name, broker) in brokers_read.iter() {
                    if let Ok(positions) = broker.get_positions().await {
                        debug!("Reconciled {} positions from {}", positions.len(), name);
                        // Compare with local tracking
                    }
                }

                // Check for stale orders
                let mut tracking = order_tracking.write().await;
                let now = Utc::now();
                let timeout = ChronoDuration::milliseconds(interval_sec as i64 * 1000);
                tracking.retain(|_, t| {
                    if !t.acknowledged && now - t.submitted_at > timeout {
                        warn!("Order {} timed out without acknowledgment", t.order.id);
                        false
                    } else { !matches!(t.order.status, OrderStatus::Filled | OrderStatus::Cancelled | OrderStatus::Rejected | OrderStatus::Expired) }
                });

                // Purge expired idempotency keys from SQLite (TTL window enforcement)
                if let Some(store) = idempotency_store.read().await.as_ref() {
                    let _ = store.purge_expired().await;
                }
            }
        });
    }
}

pub struct OrderBuilder {
    order: Order,
}

impl OrderBuilder {
    pub fn new(symbol: Symbol, side: Side, order_type: OrderType, quantity: Decimal) -> Self {
        Self {
            order: Order::new(symbol, side, order_type, quantity),
        }
    }

    pub fn limit_price(mut self, price: Decimal) -> Self {
        self.order.limit_price = Some(price);
        self
    }

    pub fn stop_price(mut self, price: Decimal) -> Self {
        self.order.stop_price = Some(price);
        self
    }

    pub fn trail_amount(mut self, amount: Decimal) -> Self {
        self.order.trail_amount = Some(amount);
        self
    }

    pub fn trail_percent(mut self, percent: Decimal) -> Self {
        self.order.trail_percent = Some(percent);
        self
    }

    pub fn time_in_force(mut self, tif: TimeInForce) -> Self {
        self.order.time_in_force = tif;
        self
    }

    pub fn gtd(mut self, date: DateTime<Utc>) -> Self {
        self.order.time_in_force = TimeInForce::Gtd;
        self.order.gtd_date = Some(date);
        self
    }

    pub fn client_order_id(mut self, id: String) -> Self {
        self.order.client_order_id = id;
        self
    }

    pub fn tag(mut self, key: String, value: String) -> Self {
        self.order.tags.insert(key, value);
        self
    }

    pub fn build(self) -> Order {
        self.order
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulator::SimulatorAdapter;
    use bt_core::types::{Symbol, Side, OrderType};

    #[tokio::test]
    async fn test_oms_idempotent_order_deduplication() {
        // Keep this a pure in-memory dedup test (no SQLite side effects).
        let config = OMSConfig {
            enable_idempotency_persistence: false,
            ..OMSConfig::default()
        };
        let oms = OrderManagementSystem::new_with_components(config, "Simulator".to_string(), None, None);
        let sim = SimulatorAdapter::new(crate::broker::BrokerConfig::default());
        let sym = Symbol::new(bt_core::types::Venue::Nse, "RELIANCE", bt_core::types::AssetClass::Equity);
        sim.update_price(sym.clone(), Decimal::new(2500, 0)).await;
        let _ = oms.add_broker(sim).await;

        let mut order = Order::new(
            sym,
            Side::Buy,
            OrderType::Market,
            Decimal::new(10, 0)
        );
        order.client_order_id = "IDEMPOTENCY-SIGN-001".to_string();

        // First submission should pass cleanly
        let res1 = oms.submit_order(order.clone(), Some("Simulator".to_string())).await;
        assert!(res1.is_ok(), "First execution failed: {:?}", res1);

        // Immediate duplicate submission with identical client_order_id must be cleanly blocked!
        let res2 = oms.submit_order(order, Some("Simulator".to_string())).await;
        assert!(res2.is_err(), "Duplicate order should have been rejected!");
        assert!(res2.unwrap_err().to_string().contains("Idempotent Rejection: Duplicate order intercepted"));
    }

    #[tokio::test]
    async fn test_oms_idempotency_persists_across_restart() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join("test_oms_idempotency_restart.sqlite");
        let _ = std::fs::remove_file(&db_path);

        let sym = Symbol::new(bt_core::types::Venue::Nse, "RELIANCE", bt_core::types::AssetClass::Equity);
        let client_order_id = "RESTART-SIGN-001".to_string();

        // First "process" — submit an order so its key is persisted to SQLite.
        {
            let config = OMSConfig {
                enable_idempotency_persistence: true,
                idempotency_db_path: db_path.clone(),
                ..OMSConfig::default()
            };
            let oms = OrderManagementSystem::new_with_components(config, "Simulator".to_string(), None, None);
            let sim = SimulatorAdapter::new(crate::broker::BrokerConfig::default());
            sim.update_price(sym.clone(), Decimal::new(2500, 0)).await;
            oms.add_broker(sim).await.unwrap();

            let mut order = Order::new(sym.clone(), Side::Buy, OrderType::Market, Decimal::new(10, 0));
            order.client_order_id = client_order_id.clone();
            let res = oms.submit_order(order, Some("Simulator".to_string())).await;
            assert!(res.is_ok(), "First submission failed: {:?}", res);
        } // drop oms — simulates process exit

        // Second "process" — new OMS instance, same SQLite db. The duplicate
        // client_order_id must be rejected even though the in-memory cache is gone.
        {
            let config = OMSConfig {
                enable_idempotency_persistence: true,
                idempotency_db_path: db_path.clone(),
                ..OMSConfig::default()
            };
            let oms = OrderManagementSystem::new_with_components(config, "Simulator".to_string(), None, None);
            oms.start().await.unwrap(); // exercises the warm-cache-on-startup path
            let sim = SimulatorAdapter::new(crate::broker::BrokerConfig::default());
            sim.update_price(sym.clone(), Decimal::new(2500, 0)).await;
            oms.add_broker(sim).await.unwrap();

            let mut order = Order::new(sym, Side::Buy, OrderType::Market, Decimal::new(10, 0));
            order.client_order_id = client_order_id;
            let res = oms.submit_order(order, Some("Simulator".to_string())).await;
            assert!(res.is_err(), "Duplicate order must be rejected after restart");
            let err = res.unwrap_err().to_string();
            assert!(
                err.contains("Duplicate order (persisted)"),
                "Expected persistent-store rejection, got: {:?}",
                err
            );
        }

        let _ = std::fs::remove_file(&db_path);
    }
}