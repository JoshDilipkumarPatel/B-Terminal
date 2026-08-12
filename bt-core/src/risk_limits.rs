use crate::events::{RiskEvent, RiskLimitType};
use crate::types::{Account, Order, Position, Side, Symbol};
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub use crate::correlation_groups::{get_correlation_group, CorrelationGroupRegistry};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RiskLimits {
    pub global: GlobalRiskLimits,
    pub per_strategy: HashMap<String, StrategyRiskLimits>,
    pub per_symbol: HashMap<String, SymbolRiskLimits>,
}

fn default_correlation_recompute_interval() -> usize {
    crate::correlation_groups::DEFAULT_RECOMPUTE_INTERVAL
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GlobalRiskLimits {
    pub max_daily_loss_pct: Decimal,
    pub max_drawdown_pct: Decimal,
    pub max_leverage: Decimal,
    pub max_open_orders: usize,
    pub max_order_size_usd: Decimal,
    pub max_portfolio_heat_pct: Decimal,
    pub max_sector_exposure_pct: Decimal,
    /// Number of `update_returns` calls between dynamic-group re-clusters (0 → default 100).
    #[serde(default = "default_correlation_recompute_interval")]
    pub correlation_recompute_interval: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrategyRiskLimits {
    pub max_allocation_pct: Decimal,
    pub max_drawdown_pct: Decimal,
    pub max_open_positions: usize,
    pub max_daily_trades: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SymbolRiskLimits {
    pub max_position_pct: Decimal,
    pub max_notional_usd: Decimal,
    pub min_liquidity_usd: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskCheckResult {
    Pass,
    Warn(String),
    Reject(String),
}

impl RiskCheckResult {
    pub fn is_pass(&self) -> bool {
        matches!(self, RiskCheckResult::Pass)
    }

    pub fn is_reject(&self) -> bool {
        matches!(self, RiskCheckResult::Reject(_))
    }
}

#[derive(Debug, Clone)]
pub struct RiskManager {
    limits: Arc<RwLock<RiskLimits>>,
    daily_pnl: Arc<RwLock<Decimal>>,
    peak_equity: Arc<RwLock<Decimal>>,
    strategy_pnl: Arc<RwLock<HashMap<String, Decimal>>>,
    strategy_peak: Arc<RwLock<HashMap<String, Decimal>>>,
    open_orders_count: Arc<RwLock<usize>>,
    positions: Arc<RwLock<HashMap<Symbol, Position>>>,
    account: Arc<RwLock<Option<Account>>>,
    event_tx: tokio::sync::broadcast::Sender<RiskEvent>,
    /// Dynamic correlation group registry — computes groups from live return data
    pub correlation_registry: Arc<RwLock<CorrelationGroupRegistry>>,
    pub regime_detector: Arc<RwLock<crate::regime::RegimeDetector>>,
}

impl RiskManager {
    pub fn new(limits: RiskLimits) -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(1024);
        Self::with_event_tx(limits, event_tx)
    }

    pub fn with_event_tx(limits: RiskLimits, event_tx: tokio::sync::broadcast::Sender<RiskEvent>) -> Self {
        // 0 (default) means "use the registry's default interval".
        let recompute_interval = if limits.global.correlation_recompute_interval == 0 {
            crate::correlation_groups::DEFAULT_RECOMPUTE_INTERVAL
        } else {
            limits.global.correlation_recompute_interval
        };
        Self {
            limits: Arc::new(RwLock::new(limits)),
            daily_pnl: Arc::new(RwLock::new(Decimal::ZERO)),
            peak_equity: Arc::new(RwLock::new(Decimal::ZERO)),
            strategy_pnl: Arc::new(RwLock::new(HashMap::new())),
            strategy_peak: Arc::new(RwLock::new(HashMap::new())),
            open_orders_count: Arc::new(RwLock::new(0)),
            positions: Arc::new(RwLock::new(HashMap::new())),
            account: Arc::new(RwLock::new(None)),
            event_tx,
            correlation_registry: Arc::new(RwLock::new(
                CorrelationGroupRegistry::new().with_recompute_interval(recompute_interval),
            )),
            regime_detector: Arc::new(RwLock::new(crate::regime::RegimeDetector::new())),
        }
    }

    pub async fn validate_order(&self, order: &Order) -> RiskCheckResult {
        let limits = self.limits.read().await;
        let account = self.account.read().await;

        // Check max order size (dynamically scaled by Market Regime)
        let detector = self.regime_detector.read().await;
        let multiplier = Decimal::from_f64_retain(detector.position_sizing_multiplier()).unwrap_or(Decimal::ONE);
        let effective_max_order_size = limits.global.max_order_size_usd * multiplier;
        
        let notional = order.quantity * order.limit_price.unwrap_or(Decimal::ZERO);
        if notional > effective_max_order_size {
            return RiskCheckResult::Reject(format!(
                "Order notional ${} exceeds regime-adjusted max ${} (Regime: {:?})",
                notional, effective_max_order_size, detector.current_regime()
            ));
        }

        // Check aggregate correlated sector exposure (prevents multi-order limit circumvention)
        if let Some(account) = account.as_ref() {
            if account.equity > Decimal::ZERO && limits.global.max_sector_exposure_pct > Decimal::ZERO {
                let registry = self.correlation_registry.read().await;
                let target_group = registry.get_group(&order.symbol.ticker);
                let mut current_group_exposure = notional;
                let positions = self.positions.read().await;
                for (sym, pos) in positions.iter() {
                    if registry.get_group(&sym.ticker) == target_group && pos.quantity > Decimal::ZERO {
                        let price = pos.current_price.unwrap_or(pos.avg_entry_price);
                        current_group_exposure += pos.quantity * price;
                    }
                }
                let group_exposure_pct = current_group_exposure / account.equity;
                if group_exposure_pct > limits.global.max_sector_exposure_pct {
                    return RiskCheckResult::Reject(format!(
                        "Aggregate correlated exposure for sector '{}' ({:.2}%) exceeds max allowed ({:.2}%)",
                        target_group,
                        group_exposure_pct * Decimal::new(100, 0),
                        limits.global.max_sector_exposure_pct * Decimal::new(100, 0)
                    ));
                }
            }
        }

        // Check max open orders
        let open_orders = *self.open_orders_count.read().await;
        if open_orders >= limits.global.max_open_orders {
            return RiskCheckResult::Reject(format!(
                "Max open orders ({}) reached",
                limits.global.max_open_orders
            ));
        }

        // Check daily loss limit
        let daily_pnl = *self.daily_pnl.read().await;
        if let Some(account) = account.as_ref() {
            if account.equity > Decimal::ZERO {
                let daily_loss_pct = (daily_pnl / account.equity).abs();
                if daily_loss_pct > limits.global.max_daily_loss_pct {
                    self.emit_risk_event(RiskEvent::DailyLossLimitExceeded(crate::events::DailyLossEvent {
                        current_loss: daily_pnl.abs(),
                        limit: account.equity * limits.global.max_daily_loss_pct,
                        timestamp: Utc::now(),
                    })).await;
                    return RiskCheckResult::Reject(format!(
                        "Daily loss limit breached: {:.2}% > {:.2}%",
                        daily_loss_pct * Decimal::new(100, 0),
                        limits.global.max_daily_loss_pct * Decimal::new(100, 0)
                    ));
                }
            }
        }

        // Check max drawdown
        let peak = *self.peak_equity.read().await;
        if let Some(account) = account.as_ref() {
            if peak > Decimal::ZERO {
                let drawdown = (peak - account.equity) / peak;
                if drawdown > limits.global.max_drawdown_pct {
                    self.emit_risk_event(RiskEvent::LimitBreached(crate::events::RiskLimitBreach {
                        limit_type: RiskLimitType::MaxDrawdown,
                        current_value: drawdown,
                        limit_value: limits.global.max_drawdown_pct,
                        symbol: None,
                        timestamp: Utc::now(),
                    })).await;
                    return RiskCheckResult::Reject(format!(
                        "Max drawdown breached: {:.2}% > {:.2}%",
                        drawdown * Decimal::new(100, 0),
                        limits.global.max_drawdown_pct * Decimal::new(100, 0)
                    ));
                }
            }
        }

        // Check leverage
        if let Some(account) = account.as_ref() {
            let positions = self.positions.read().await;
            let total_exposure: Decimal = positions.values()
                .map(|p| p.market_value.unwrap_or(Decimal::ZERO).abs())
                .sum();
            let leverage = if account.equity > Decimal::ZERO {
                total_exposure / account.equity
            } else {
                Decimal::ZERO
            };
            if leverage > limits.global.max_leverage {
                return RiskCheckResult::Reject(format!(
                    "Leverage {:.2}x exceeds max {:.2}x",
                    leverage, limits.global.max_leverage
                ));
            }
        }

        // Check position limit for symbol
        if let Some(symbol_limits) = limits
            .per_symbol
            .get(&order.symbol.ticker)
            .or_else(|| limits.per_symbol.get(&order.symbol.to_string()))
        {
            let positions = self.positions.read().await;
            if let Some(pos) = positions.get(&order.symbol) {
                let new_qty = match order.side {
                    Side::Buy => pos.quantity + order.quantity,
                    Side::Sell => pos.quantity - order.quantity,
                };
                let new_notional = new_qty.abs() * order.limit_price.unwrap_or(Decimal::ZERO);
                if new_notional > symbol_limits.max_notional_usd {
                    return RiskCheckResult::Reject(format!(
                        "Position notional ${} exceeds symbol limit ${}",
                        new_notional, symbol_limits.max_notional_usd
                    ));
                }
            }
        }

        RiskCheckResult::Pass
    }

    pub async fn update_position(&self, position: Position) {
        let mut positions = self.positions.write().await;
        positions.insert(position.symbol.clone(), position);
    }

    pub async fn remove_position(&self, symbol: &Symbol) {
        let mut positions = self.positions.write().await;
        positions.remove(symbol);
    }

    pub async fn update_account(&self, account: Account) {
        let mut acc = self.account.write().await;
        let mut peak = self.peak_equity.write().await;

        if acc.is_none() || account.equity > *peak {
            *peak = account.equity;
        }
        *acc = Some(account);
    }

    pub async fn update_daily_pnl(&self, pnl: Decimal) {
        let mut daily = self.daily_pnl.write().await;
        *daily = pnl;
    }

    /// Update correlation registry with daily returns for all held positions.
    /// Call this once per trading day (e.g., at market close) with P&L data.
    /// Approximates daily return as position P&L / position market value.
    ///
    /// Dynamic groups are re-clustered internally by `update_returns` on the
    /// configured `correlation_recompute_interval` (default 100 calls).
    pub async fn update_correlation_returns(&self) {
        let positions = self.positions.read().await;
        let mut registry = self.correlation_registry.write().await;
        for (sym, pos) in positions.iter() {
            if pos.quantity != Decimal::ZERO && pos.market_value.unwrap_or(Decimal::ZERO) != Decimal::ZERO {
                let mv = pos.market_value.unwrap_or(Decimal::ZERO);
                let pnl = pos.unrealized_pnl.unwrap_or(Decimal::ZERO);
                let daily_return = (pnl / mv).to_f64().unwrap_or(0.0);
                registry.update_returns(&sym.ticker, daily_return);
            }
        }
    }

    pub async fn increment_open_orders(&self) {
        let mut count = self.open_orders_count.write().await;
        *count += 1;
    }

    pub async fn decrement_open_orders(&self) {
        let mut count = self.open_orders_count.write().await;
        if *count > 0 {
            *count -= 1;
        }
    }

    pub async fn update_strategy_pnl(&self, strategy_id: &str, pnl: Decimal) {
        let mut map = self.strategy_pnl.write().await;
        let mut peak_map = self.strategy_peak.write().await;
        map.insert(strategy_id.to_string(), pnl);
        if pnl > *peak_map.entry(strategy_id.to_string()).or_insert(pnl) {
            peak_map.insert(strategy_id.to_string(), pnl);
        }
    }

    pub async fn check_strategy_limits(&self, strategy_id: &str) -> RiskCheckResult {
        let limits = self.limits.read().await;
        let pnl_map = self.strategy_pnl.read().await;
        let peak_map = self.strategy_peak.read().await;

        if let Some(strategy_limits) = limits.per_strategy.get(strategy_id) {
            if let Some(&pnl) = pnl_map.get(strategy_id) {
                if let Some(&peak) = peak_map.get(strategy_id) {
                    if peak > Decimal::ZERO {
                        let drawdown = (peak - pnl) / peak;
                        if drawdown > strategy_limits.max_drawdown_pct {
                            return RiskCheckResult::Reject(format!(
                                "Strategy {} drawdown {:.2}% exceeds limit {:.2}%",
                                strategy_id,
                                drawdown * Decimal::new(100, 0),
                                strategy_limits.max_drawdown_pct * Decimal::new(100, 0)
                            ));
                        }
                    }
                }
            }
        }
        RiskCheckResult::Pass
    }

    async fn emit_risk_event(&self, event: RiskEvent) {
        let _ = self.event_tx.send(event);
    }

    pub async fn get_positions(&self) -> HashMap<Symbol, Position> {
        self.positions.read().await.clone()
    }

    pub async fn get_account(&self) -> Option<Account> {
        self.account.read().await.clone()
    }

    pub async fn get_daily_pnl(&self) -> Decimal {
        *self.daily_pnl.read().await
    }

    pub async fn get_peak_equity(&self) -> Decimal {
        *self.peak_equity.read().await
    }

    pub async fn get_limits(&self) -> RiskLimits {
        self.limits.read().await.clone()
    }

    pub async fn update_limits(&self, limits: RiskLimits) {
        *self.limits.write().await = limits;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskMetrics {
    pub daily_pnl: Decimal,
    pub daily_pnl_pct: Decimal,
    pub current_drawdown: Decimal,
    pub current_drawdown_pct: Decimal,
    pub leverage: Decimal,
    pub open_orders: usize,
    pub total_exposure: Decimal,
    pub portfolio_heat_pct: Decimal,
    pub strategy_metrics: HashMap<String, StrategyRiskMetrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyRiskMetrics {
    pub current_pnl: Decimal,
    pub peak_pnl: Decimal,
    pub drawdown: Decimal,
    pub drawdown_pct: Decimal,
    pub open_positions: usize,
    pub daily_trades: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Venue, AssetClass, OrderType};

    #[tokio::test]
    async fn test_aggregate_correlated_sector_exposure_limit() {
        let mut limits = RiskLimits::default();
        limits.global.max_order_size_usd = Decimal::new(1000, 0); // $1,000 max single order
        limits.global.max_sector_exposure_pct = Decimal::new(5, 2); // 5% max aggregate sector exposure ($5,000 on $100K capital)

        let risk_mgr = RiskManager::new(limits);
        
        // Set up account with $100,000 equity
        let account = Account {
            id: uuid::Uuid::new_v4(),
            cash: Decimal::new(100_000, 0),
            equity: Decimal::new(100_000, 0),
            initial_margin: Decimal::ZERO,
            maintenance_margin: Decimal::ZERO,
            day_trading_buying_power: Decimal::new(100_000, 0),
            long_market_value: Decimal::ZERO,
            short_market_value: Decimal::ZERO,
            buying_power: Decimal::new(100_000, 0),
            currency: "USD".to_string(),
            updated_at: Utc::now(),
        };
        risk_mgr.account.write().await.replace(account);

        // Simulate preexisting open positions in IN_BANKING sector totaling $4,500
        let mut pos_map = HashMap::new();
        let sym_hdfc = Symbol::new(Venue::Nse, "HDFCBANK", AssetClass::Equity);
        pos_map.insert(sym_hdfc.clone(), Position {
            symbol: sym_hdfc,
            quantity: Decimal::new(100, 0),
            avg_entry_price: Decimal::new(25, 0),
            current_price: Some(Decimal::new(25, 0)), // $2,500 exposure
            market_value: Some(Decimal::new(2500, 0)),
            unrealized_pnl: Some(Decimal::ZERO),
            realized_pnl: Decimal::ZERO,
            opened_at: Utc::now(),
            updated_at: Utc::now(),
        });
        let sym_icici = Symbol::new(Venue::Nse, "ICICIBANK", AssetClass::Equity);
        pos_map.insert(sym_icici.clone(), Position {
            symbol: sym_icici,
            quantity: Decimal::new(100, 0),
            avg_entry_price: Decimal::new(20, 0),
            current_price: Some(Decimal::new(20, 0)), // $2,000 exposure
            market_value: Some(Decimal::new(2000, 0)),
            unrealized_pnl: Some(Decimal::ZERO),
            realized_pnl: Decimal::ZERO,
            opened_at: Utc::now(),
            updated_at: Utc::now(),
        });
        *risk_mgr.positions.write().await = pos_map;

        // Try placing a third banking trade for $750 in SBIN (which passes the single $1,000 order size limit & 0.8x regime multiplier!)
        let order = Order::new(
            Symbol::new(Venue::Nse, "SBIN", AssetClass::Equity),
            Side::Buy,
            OrderType::Limit,
            Decimal::new(100, 0)
        ).with_limit(Decimal::new(75, 1)); // $750 notional

        // Total IN_BANKING exposure would become $2500 + $2000 + $750 = $5,250 (5.25% > 5.00% limit)
        let result = risk_mgr.validate_order(&order).await;
        match result {
            RiskCheckResult::Reject(msg) => {
                assert!(msg.contains("Aggregate correlated exposure for sector 'IN_BANKING'"), "Unexpected error message: {}", msg);
                assert!(msg.contains("exceeds max allowed"));
            }
            _ => panic!("Order should have been rejected due to aggregate sector exposure! Got: {:?}", result),
        }
    }
}
