use crate::broker::{BrokerAdapter, BrokerConfig, BrokerType, BrokerHealth, BrokerAccountInfo, AccountType, AccountStatus};
use async_trait::async_trait;
use bt_core::{ExecutionEvent, Fill, OrderAck, OrderId, Liquidity};
use bt_core::types::{Order, OrderStatus, OrderType, Side, Symbol, Position, Account, Venue};
use chrono::Utc;
use rust_decimal::Decimal;
use rand::{Rng, SeedableRng};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio::time::Duration as TokioDuration;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatorConfig {
    pub initial_cash: Decimal,
    pub commission_per_share: Decimal,
    pub commission_min: Decimal,
    pub slippage_bps: u32,
    pub spread_bps: u32,
    pub fill_probability: f64,
    pub partial_fill_probability: f64,
    pub latency_ms: u64,
    pub market_data_latency_ms: u64,
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self {
            initial_cash: Decimal::new(100000, 2),
            commission_per_share: Decimal::new(1, 4),
            commission_min: Decimal::new(1, 2),
            slippage_bps: 5,
            spread_bps: 2,
            fill_probability: 1.0,
            partial_fill_probability: 0.1,
            latency_ms: 10,
            market_data_latency_ms: 1,
        }
    }
}

struct SimulatorState {
    cash: Decimal,
    positions: HashMap<Symbol, Position>,
    orders: HashMap<OrderId, Order>,
    open_orders: Vec<OrderId>,
    #[allow(dead_code)]
    equity: Decimal,
    last_prices: HashMap<Symbol, Decimal>,
    fills: Vec<Fill>,
    rng: rand::rngs::StdRng,
    current_vpin: f64,
}

pub struct SimulatorAdapter {
    config: SimulatorConfig,
    #[allow(dead_code)]
    broker_config: BrokerConfig,
    state: Arc<RwLock<SimulatorState>>,
    event_tx: broadcast::Sender<ExecutionEvent>,
    market_data_tx: broadcast::Sender<bt_core::events::MarketEvent>,
    running: Arc<RwLock<bool>>,
}

impl SimulatorAdapter {
    pub fn new(broker_config: BrokerConfig) -> Self {
        Self::new_with_config(broker_config, SimulatorConfig::default())
    }

    pub fn new_with_config(broker_config: BrokerConfig, sim_config: SimulatorConfig) -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        let (market_data_tx, _) = broadcast::channel(1000);

        let state = SimulatorState {
            cash: sim_config.initial_cash,
            positions: HashMap::new(),
            orders: HashMap::new(),
            open_orders: Vec::new(),
            equity: sim_config.initial_cash,
            last_prices: HashMap::new(),
            fills: Vec::new(),
            rng: rand::rngs::StdRng::from_entropy(),
            current_vpin: 0.0,
        };

        Self {
            config: sim_config,
            broker_config,
            state: Arc::new(RwLock::new(state)),
            event_tx,
            market_data_tx,
            running: Arc::new(RwLock::new(false)),
        }
    }

    pub fn with_market_data(&self) -> broadcast::Sender<bt_core::events::MarketEvent> {
        self.market_data_tx.clone()
    }

    pub async fn update_vpin(&self, vpin: f64) {
        let mut state = self.state.write().await;
        state.current_vpin = vpin;
    }

    fn apply_slippage(&self, price: Decimal, side: Side, vpin: f64) -> Decimal {
        let toxicity_multiplier = Decimal::from_f64_retain(1.0 + (vpin * 2.0)).unwrap_or(Decimal::ONE);
        let slippage = (price * Decimal::from(self.config.slippage_bps) / Decimal::new(10000, 0)) * toxicity_multiplier;
        let spread = price * Decimal::from(self.config.spread_bps) / Decimal::new(10000, 0);
        match side {
            Side::Buy => price + slippage + spread / Decimal::from(2),
            Side::Sell => price - slippage - spread / Decimal::from(2),
        }
    }

    fn calculate_commission(&self, quantity: Decimal, _price: Decimal) -> Decimal {
        let per_share = self.config.commission_per_share * quantity;
        per_share.max(self.config.commission_min)
    }

    async fn try_fill_order(&self, order_id: OrderId, current_price: Decimal) {
        let mut state = self.state.write().await;

        let Some(mut order) = state.orders.get(&order_id).cloned() else { return; };

        if !order.is_active() {
            return;
        }

        let vpin = state.current_vpin;
        let fill_price = self.apply_slippage(current_price, order.side, vpin);
        let should_fill = state.rng.gen_bool(self.config.fill_probability);

        if !should_fill {
            return;
        }

        let remaining = order.remaining_quantity();
        let mut fill_qty = remaining;

        // Partial fill
        if state.rng.gen_bool(self.config.partial_fill_probability) && remaining > Decimal::ONE {
            let pct = Decimal::from_f64_retain(state.rng.gen_range(0.1..0.9)).unwrap_or(Decimal::new(5, 1));
            fill_qty = (remaining * pct).floor().max(Decimal::ONE);
        }

        let commission = self.calculate_commission(fill_qty, fill_price);
        let notional = fill_qty * fill_price;

        // Check buying power for buys
        if order.side == Side::Buy {
            let required = notional + commission;
            if state.cash < required {
                // Reject order
                order.status = OrderStatus::Rejected;
                state.orders.insert(order_id, order.clone());
                let _ = self.event_tx.send(ExecutionEvent::OrderRejected(bt_core::events::OrderReject {
                    order_id,
                    client_order_id: order.client_order_id.clone(),
                    reason: "Insufficient buying power".to_string(),
                    timestamp: Utc::now(),
                }));
                state.open_orders.retain(|id| *id != order_id);
                return;
            }
            state.cash -= required;
        } else {
            // For sells, check position
            if let Some(pos) = state.positions.get(&order.symbol) {
                if pos.quantity < fill_qty {
                    fill_qty = pos.quantity;
                }
            }
            state.cash += notional - commission;
        }

        // Update order
        order.update_fill(fill_qty, fill_price);

        // Create fill
        let fill = Fill {
            id: Uuid::new_v4(),
            order_id,
            symbol: order.symbol.clone(),
            side: order.side,
            quantity: fill_qty,
            price: fill_price,
            venue: Venue::Simulator,
            timestamp: Utc::now(),
            commission,
            liquidity: Liquidity::Removed,
        };
        state.fills.push(fill.clone());

        // Update position inside a block so borrow ends
        let is_zero = {
            let pos = state.positions.entry(order.symbol.clone()).or_insert_with(|| Position {
                symbol: order.symbol.clone(),
                quantity: Decimal::ZERO,
                avg_entry_price: Decimal::ZERO,
                current_price: Some(current_price),
                unrealized_pnl: None,
                realized_pnl: Decimal::ZERO,
                market_value: None,
                opened_at: Utc::now(),
                updated_at: Utc::now(),
            });

            let (new_qty, new_avg) = match order.side {
                Side::Buy => {
                    if pos.quantity < Decimal::ZERO {
                        let new_qty = pos.quantity + fill_qty;
                        let realized = (pos.avg_entry_price - fill_price) * fill_qty;
                        pos.realized_pnl += realized;
                        (new_qty, if new_qty == Decimal::ZERO { Decimal::ZERO } else { pos.avg_entry_price })
                    } else {
                        let total_cost = pos.avg_entry_price * pos.quantity + fill_price * fill_qty;
                        let new_qty = pos.quantity + fill_qty;
                        let new_avg = if new_qty != Decimal::ZERO { total_cost / new_qty } else { Decimal::ZERO };
                        (new_qty, new_avg)
                    }
                }
                Side::Sell => {
                    if pos.quantity > Decimal::ZERO {
                        let new_qty = pos.quantity - fill_qty;
                        let realized = (fill_price - pos.avg_entry_price) * fill_qty;
                        pos.realized_pnl += realized;
                        (new_qty, if new_qty == Decimal::ZERO { Decimal::ZERO } else { pos.avg_entry_price })
                    } else {
                        let total_cost = pos.avg_entry_price * pos.quantity.abs() + fill_price * fill_qty;
                        let new_qty = pos.quantity - fill_qty;
                        let new_avg = if new_qty != Decimal::ZERO { total_cost / new_qty.abs() } else { Decimal::ZERO };
                        (new_qty, new_avg)
                    }
                }
            };

            pos.quantity = new_qty;
            pos.avg_entry_price = new_avg;
            pos.current_price = Some(current_price);
            pos.market_value = Some(new_qty.abs() * current_price);
            pos.unrealized_pnl = Some((current_price - pos.avg_entry_price) * new_qty);
            pos.updated_at = Utc::now();
            pos.quantity == Decimal::ZERO
        };

        if is_zero {
            state.positions.remove(&order.symbol);
        }

        state.orders.insert(order_id, order.clone());

        // Emit events
        if order.status == OrderStatus::Filled {
            let _ = self.event_tx.send(ExecutionEvent::OrderFilled(bt_core::events::OrderFill {
                order_id,
                fill: fill.clone(),
            }));
            state.open_orders.retain(|id| *id != order_id);
        } else {
            let _ = self.event_tx.send(ExecutionEvent::OrderPartialFill(bt_core::events::OrderPartialFill {
                order_id,
                fill: fill.clone(),
                remaining: order.remaining_quantity(),
            }));
        }
    }
}

#[async_trait]
impl BrokerAdapter for SimulatorAdapter {
    fn broker_type(&self) -> BrokerType {
        BrokerType::Simulator
    }

    fn name(&self) -> &str {
        "Simulator"
    }

    fn is_paper(&self) -> bool {
        true
    }

    async fn connect(&mut self) -> anyhow::Result<()> {
        *self.running.write().await = true;
        info!("Simulator connected with ${} initial cash", self.config.initial_cash);
        Ok(())
    }

    async fn disconnect(&mut self) -> anyhow::Result<()> {
        *self.running.write().await = false;
        info!("Simulator disconnected");
        Ok(())
    }

    async fn place_order(&self, order: Order) -> anyhow::Result<OrderId> {
        let mut state = self.state.write().await;

        if !order.is_active() {
            return Err(anyhow::anyhow!("Invalid order"));
        }

        let order_id = order.id;
        state.orders.insert(order_id, order.clone());
        state.open_orders.push(order_id);

        // Emit acknowledgment
        let ack = OrderAck {
            order_id,
            client_order_id: order.client_order_id.clone(),
            broker_order_id: order_id.to_string(),
            timestamp: Utc::now(),
        };
        let _ = self.event_tx.send(ExecutionEvent::OrderAcknowledged(ack));

        // Simulate latency
        tokio::time::sleep(TokioDuration::from_millis(self.config.latency_ms)).await;

        // Try immediate fill for market orders
        if order.order_type == OrderType::Market {
            let price_opt = state.last_prices.get(&order.symbol).copied();
            if let Some(price) = price_opt {
                drop(state); // Release lock before async call
                self.try_fill_order(order_id, price).await;
            }
        }

        Ok(order_id)
    }

    async fn cancel_order(&self, order_id: OrderId) -> anyhow::Result<()> {
        let mut state = self.state.write().await;

        if let Some(order) = state.orders.get_mut(&order_id) {
            if order.is_active() {
                order.status = OrderStatus::Cancelled;
                state.open_orders.retain(|id| *id != order_id);

                let _ = self.event_tx.send(ExecutionEvent::OrderCancelled(order_id));
            }
        }

        Ok(())
    }

    async fn cancel_all_orders(&self) -> anyhow::Result<()> {
        let mut state = self.state.write().await;

        let open_orders = std::mem::take(&mut state.open_orders);
        for order_id in open_orders {
            if let Some(order) = state.orders.get_mut(&order_id) {
                if order.is_active() {
                    order.status = OrderStatus::Cancelled;
                    let _ = self.event_tx.send(ExecutionEvent::OrderCancelled(order_id));
                }
            }
        }

        Ok(())
    }

    async fn get_order(&self, order_id: OrderId) -> anyhow::Result<Option<Order>> {
        let state = self.state.read().await;
        Ok(state.orders.get(&order_id).cloned())
    }

    async fn get_open_orders(&self) -> anyhow::Result<Vec<Order>> {
        let state = self.state.read().await;
        let orders: Vec<Order> = state.open_orders.iter()
            .filter_map(|id| state.orders.get(id).cloned())
            .collect();
        Ok(orders)
    }

    async fn get_order_history(&self, limit: usize) -> anyhow::Result<Vec<Order>> {
        let state = self.state.read().await;
        let mut orders: Vec<Order> = state.orders.values().cloned().collect();
        orders.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        orders.truncate(limit);
        Ok(orders)
    }

    async fn get_positions(&self) -> anyhow::Result<Vec<Position>> {
        let state = self.state.read().await;
        Ok(state.positions.values().cloned().collect())
    }

    async fn get_position(&self, symbol: &Symbol) -> anyhow::Result<Option<Position>> {
        let state = self.state.read().await;
        Ok(state.positions.get(symbol).cloned())
    }

    async fn get_account(&self) -> anyhow::Result<Account> {
        let state = self.state.read().await;

        let long_market_value: Decimal = state.positions.values()
            .filter(|p| p.quantity > Decimal::ZERO)
            .map(|p| p.market_value.unwrap_or(Decimal::ZERO))
            .sum();

        let short_market_value: Decimal = state.positions.values()
            .filter(|p| p.quantity < Decimal::ZERO)
            .map(|p| p.market_value.unwrap_or(Decimal::ZERO).abs())
            .sum();

        let equity = state.cash + long_market_value - short_market_value;

        Ok(Account {
            id: Uuid::new_v4(),
            equity,
            cash: state.cash,
            buying_power: state.cash * Decimal::new(2, 0), // 2x margin
            initial_margin: (long_market_value + short_market_value) * Decimal::new(5, 2), // 50%
            maintenance_margin: (long_market_value + short_market_value) * Decimal::new(25, 2), // 25%
            day_trading_buying_power: state.cash * Decimal::new(4, 0), // 4x for PDT
            long_market_value,
            short_market_value,
            currency: "USD".to_string(),
            updated_at: Utc::now(),
        })
    }

    async fn get_accounts(&self) -> anyhow::Result<Vec<BrokerAccountInfo>> {
        Ok(vec![BrokerAccountInfo {
            id: "sim".to_string(),
            name: "Simulator".to_string(),
            account_type: AccountType::Margin,
            status: AccountStatus::Active,
            currency: "USD".to_string(),
        }])
    }

    fn events(&self) -> broadcast::Receiver<ExecutionEvent> {
        self.event_tx.subscribe()
    }

    async fn health_check(&self) -> anyhow::Result<BrokerHealth> {
        let running = *self.running.read().await;
        Ok(BrokerHealth {
            healthy: running,
            latency_ms: self.config.latency_ms,
            last_order_ms: Some(self.config.latency_ms),
            error_rate: 0.0,
            connection_status: if running { "connected" } else { "disconnected" }.to_string(),
        })
    }
}

// Extension: Update last prices from market data
impl SimulatorAdapter {
    pub async fn update_price(&self, symbol: Symbol, price: Decimal) {
        let mut state = self.state.write().await;
        state.last_prices.insert(symbol.clone(), price);

        // Update position current prices
        if let Some(pos) = state.positions.get_mut(&symbol) {
            pos.current_price = Some(price);
            pos.market_value = Some(pos.quantity.abs() * price);
            pos.unrealized_pnl = Some((price - pos.avg_entry_price) * pos.quantity);
            pos.updated_at = Utc::now();
        }

        // Try fill pending orders
        for order_id in state.open_orders.clone() {
            let should_fill = state.orders.get(&order_id).is_some_and(|o| o.order_type == OrderType::Market && o.is_active());
            if should_fill {
                drop(state);
                self.try_fill_order(order_id, price).await;
                return;
            }
        }
    }

    pub async fn update_market_data(&self, event: bt_core::events::MarketEvent) {
        let _ = self.market_data_tx.send(event);
    }
}