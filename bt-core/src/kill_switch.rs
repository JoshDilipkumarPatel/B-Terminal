use crate::events::{KillReason, KillSwitchEvent, RiskEvent};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, RwLock};
use tokio::time::Duration;
use uuid::Uuid;

#[derive(Clone)]
pub struct GlobalKillSwitch {
    activated: Arc<RwLock<bool>>,
    activated_atomic: Arc<AtomicBool>,
    reason: Arc<RwLock<Option<KillReason>>>,
    activated_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    flatten_in_progress: Arc<RwLock<bool>>,
    positions_to_flatten: Arc<RwLock<HashMap<Uuid, FlattenOrder>>>,
    callbacks: Arc<RwLock<Vec<Arc<dyn KillSwitchCallback + Send + Sync>>>>,
    event_tx: broadcast::Sender<RiskEvent>,
    max_flatten_time: Duration,
}

#[derive(Debug, Clone)]
struct FlattenOrder {
    symbol: crate::types::Symbol,
    quantity: rust_decimal::Decimal,
    side: crate::types::Side,
    order_id: Option<Uuid>,
    submitted_at: Instant,
    retries: u32,
}

#[async_trait]
pub trait KillSwitchCallback: Send + Sync {
    async fn on_activate(&self, reason: KillReason) -> Result<()>;
    async fn on_flatten_complete(&self, success: bool) -> Result<()>;
}

impl GlobalKillSwitch {
    pub fn new(
        event_tx: broadcast::Sender<RiskEvent>,
        max_flatten_time_ms: u64,
    ) -> Self {
        Self {
            activated: Arc::new(RwLock::new(false)),
            activated_atomic: Arc::new(AtomicBool::new(false)),
            reason: Arc::new(RwLock::new(None)),
            activated_at: Arc::new(RwLock::new(None)),
            flatten_in_progress: Arc::new(RwLock::new(false)),
            positions_to_flatten: Arc::new(RwLock::new(HashMap::new())),
            callbacks: Arc::new(RwLock::new(Vec::new())),
            event_tx,
            max_flatten_time: Duration::from_millis(max_flatten_time_ms),
        }
    }

    pub async fn activate(&self, reason: KillReason) -> Result<()> {
        let mut activated = self.activated.write().await;
        if *activated {
            return Ok(()); // Already activated
        }
        *activated = true;
        self.activated_atomic.store(true, Ordering::SeqCst);
        *self.reason.write().await = Some(reason);
        *self.activated_at.write().await = Some(Utc::now());

        // Notify callbacks
        let callbacks = self.callbacks.read().await.clone();
        for callback in callbacks {
            if let Err(e) = callback.on_activate(reason).await {
                tracing::error!("Kill switch callback failed: {}", e);
            }
        }

        // Emit event
        let event = RiskEvent::KillSwitchActivated(KillSwitchEvent {
            reason,
            timestamp: Utc::now(),
            positions_flattened: false,
        });
        let _ = self.event_tx.send(event);

        tracing::warn!("KILL SWITCH ACTIVATED: {:?}", reason);
        Ok(())
    }

    pub async fn is_activated(&self) -> bool {
        *self.activated.read().await
    }

    pub fn get_max_flatten_time(&self) -> Duration {
        self.max_flatten_time
    }

    pub fn is_active(&self) -> bool {
        self.activated_atomic.load(Ordering::SeqCst)
    }

    pub async fn get_reason(&self) -> Option<KillReason> {
        *self.reason.read().await
    }

    pub async fn register_callback(&self, callback: Box<dyn KillSwitchCallback + Send + Sync>) {
        let callback: Arc<dyn KillSwitchCallback + Send + Sync> = callback.into();
        self.callbacks.write().await.push(callback);
    }

    pub async fn prepare_flatten(&self, positions: HashMap<crate::types::Symbol, crate::types::Position>) {
        let mut to_flatten = self.positions_to_flatten.write().await;
        to_flatten.clear();

        for (symbol, position) in positions {
            if position.quantity != rust_decimal::Decimal::ZERO {
                let side = if position.quantity > rust_decimal::Decimal::ZERO {
                    crate::types::Side::Sell
                } else {
                    crate::types::Side::Buy
                };
                to_flatten.insert(
                    Uuid::new_v4(),
                    FlattenOrder {
                        symbol,
                        quantity: position.quantity.abs(),
                        side,
                        order_id: None,
                        submitted_at: Instant::now(),
                        retries: 0,
                    },
                );
            }
        }
    }

    pub async fn execute_flatten<F>(&self, mut place_order: F) -> Result<bool>
    where
        F: FnMut(crate::types::Order) -> Result<Uuid> + Send,
    {
        let mut in_progress = self.flatten_in_progress.write().await;
        if *in_progress {
            return Ok(false); // Already flattening
        }
        *in_progress = true;
        drop(in_progress);

        let mut to_flatten = self.positions_to_flatten.write().await;
        let pending_orders: Vec<_> = to_flatten.drain().collect();
        drop(to_flatten);

        let mut retry_orders = Vec::new();
        let mut all_success = true;
        let deadline = Instant::now() + self.max_flatten_time;
        let pending_count = pending_orders.len();

        for (index, (id, mut order)) in pending_orders.into_iter().enumerate() {
            if Instant::now() >= deadline {
                tracing::error!(
                    "Kill switch flatten timeout, {} orders remaining",
                    pending_count.saturating_sub(index)
                );
                all_success = false;
                break;
            }

            let bt_order = crate::types::Order::new(
                order.symbol.clone(),
                order.side,
                crate::types::OrderType::Market,
                order.quantity,
            ).with_tif(crate::types::TimeInForce::Ioc);

            match place_order(bt_order) {
                Ok(order_id) => {
                    order.order_id = Some(order_id);
                    order.submitted_at = Instant::now();
                    tracing::info!("Flatten order placed: {:?} {} @ MKT", order.side, order.symbol);
                }
                Err(e) => {
                    tracing::error!("Failed to place flatten order: {}", e);
                    order.retries += 1;
                    if order.retries < 3 {
                        retry_orders.push((id, order));
                    } else {
                        all_success = false;
                    }
                }
            }
        }

        if !retry_orders.is_empty() {
            let mut to_flatten = self.positions_to_flatten.write().await;
            to_flatten.extend(retry_orders);
        }

        // Wait for fills (simplified - in production would track via execution events)
        tokio::time::sleep(Duration::from_millis(500)).await;

        *self.flatten_in_progress.write().await = false;

        // Notify callbacks
        let callbacks = self.callbacks.read().await.clone();
        for callback in callbacks {
            if let Err(e) = callback.on_flatten_complete(all_success).await {
                tracing::error!("Kill switch flatten callback failed: {}", e);
            }
        }

        // Emit completion event
        let event = RiskEvent::KillSwitchActivated(KillSwitchEvent {
            reason: self.reason.read().await.unwrap_or(KillReason::Manual),
            timestamp: Utc::now(),
            positions_flattened: all_success,
        });
        let _ = self.event_tx.send(event);

        Ok(all_success)
    }

    pub async fn reset(&self) -> Result<()> {
        *self.activated.write().await = false;
        self.activated_atomic.store(false, Ordering::SeqCst);
        *self.reason.write().await = None;
        *self.activated_at.write().await = None;
        self.positions_to_flatten.write().await.clear();
        tracing::info!("Kill switch reset");
        Ok(())
    }

    pub async fn status(&self) -> KillSwitchStatus {
        KillSwitchStatus {
            activated: *self.activated.read().await,
            reason: *self.reason.read().await,
            activated_at: *self.activated_at.read().await,
            flatten_in_progress: *self.flatten_in_progress.read().await,
            positions_pending: self.positions_to_flatten.read().await.len(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct KillSwitchStatus {
    pub activated: bool,
    pub reason: Option<KillReason>,
    pub activated_at: Option<DateTime<Utc>>,
    pub flatten_in_progress: bool,
    pub positions_pending: usize,
}

pub struct AutoKillSwitchMonitor {
    kill_switch: Arc<GlobalKillSwitch>,
    risk_manager: Arc<crate::risk_limits::RiskManager>,
    check_interval: Duration,
}

impl AutoKillSwitchMonitor {
    pub fn new(
        kill_switch: Arc<GlobalKillSwitch>,
        risk_manager: Arc<crate::risk_limits::RiskManager>,
        check_interval_ms: u64,
    ) -> Self {
        Self {
            kill_switch,
            risk_manager,
            check_interval: Duration::from_millis(check_interval_ms),
        }
    }

    pub async fn run(&self) {
        let mut interval = tokio::time::interval(self.check_interval);
        loop {
            interval.tick().await;

            if self.kill_switch.is_activated().await {
                continue;
            }

            // Check daily loss
            if let Some(account) = self.risk_manager.get_account().await {
                if account.equity.is_zero() {
                    continue;
                }
                
                let daily_pnl = self.risk_manager.get_daily_pnl().await;
                let limits = self.risk_manager.get_limits().await;
                let daily_loss_pct = (daily_pnl / account.equity).abs();

                if daily_loss_pct > limits.global.max_daily_loss_pct {
                    let _ = self.kill_switch.activate(KillReason::DailyLossLimit).await;
                    continue;
                }

                // Check max drawdown
                let peak = self.risk_manager.get_peak_equity().await;
                if peak > rust_decimal::Decimal::ZERO {
                    let drawdown = (peak - account.equity) / peak;
                    if drawdown > limits.global.max_drawdown_pct {
                        let _ = self.kill_switch.activate(KillReason::MaxDrawdown).await;
                        continue;
                    }
                }
            }
        }
    }
}

// Helper for testing
impl GlobalKillSwitch {
    pub async fn activate_for_test(&self, reason: KillReason) {
        let _ = self.activate(reason).await;
    }
}

pub struct DynamicCircuitBreaker {
    kill_switch: Arc<GlobalKillSwitch>,
    rolling_mean: f64,
    rolling_std_dev: f64,
}

impl DynamicCircuitBreaker {
    pub fn new(kill_switch: Arc<GlobalKillSwitch>) -> Self {
        Self {
            kill_switch,
            rolling_mean: 0.0,
            rolling_std_dev: 0.0,
        }
    }

    /// Simulates inspecting a live tick for a 5-sigma anomaly (e.g. Flash Crash or Fat Finger).
    pub async fn inspect_tick(&mut self, symbol: &str, price: f64) {
        // Mocking a rolling mean/std-dev update
        if self.rolling_mean == 0.0 {
            self.rolling_mean = price;
            self.rolling_std_dev = price * 0.001; // 0.1% initial std dev
            return;
        }
        
        let z_score = (price - self.rolling_mean).abs() / self.rolling_std_dev;
        
        if z_score > 5.0 {
            tracing::error!("5-SIGMA FAT FINGER DETECTED on {}! Z-Score: {}. Triggering hard kill switch.", symbol, z_score);
            let _ = self.kill_switch.activate(KillReason::FatFinger5Sigma).await;
        } else {
            // Update rolling stats slowly
            self.rolling_mean = self.rolling_mean * 0.99 + price * 0.01;
        }
    }
}
