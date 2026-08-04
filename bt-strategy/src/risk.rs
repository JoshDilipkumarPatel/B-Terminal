use bt_core::risk_limits::{RiskManager, StrategyRiskLimits, SymbolRiskLimits, RiskCheckResult};
use bt_core::types::{Order, OrderType};
use bt_core::events::{SignalEntry, KillReason};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct StrategyRiskManager {
    risk_manager: Arc<RiskManager>,
    strategy_limits: Arc<RwLock<HashMap<String, StrategyRiskLimits>>>,
    strategy_metrics: Arc<RwLock<HashMap<String, StrategyMetrics>>>,
    symbol_limits: Arc<RwLock<HashMap<String, SymbolRiskLimits>>>,
    position_sizers: Arc<RwLock<HashMap<String, Box<dyn PositionSizer>>>>,
}

#[derive(Debug, Clone)]
pub struct StrategyMetrics {
    pub current_pnl: Decimal,
    pub peak_pnl: Decimal,
    pub daily_pnl: Decimal,
    pub daily_trades: u32,
    pub open_positions: usize,
    pub max_drawdown: Decimal,
    pub last_reset: DateTime<Utc>,
}

impl Default for StrategyMetrics {
    fn default() -> Self {
        Self {
            current_pnl: Decimal::ZERO,
            peak_pnl: Decimal::ZERO,
            daily_pnl: Decimal::ZERO,
            daily_trades: 0,
            open_positions: 0,
            max_drawdown: Decimal::ZERO,
            last_reset: Utc::now(),
        }
    }
}

pub trait PositionSizer: Send + Sync {
    fn calculate_size(&self, signal: &SignalEntry, equity: Decimal, risk_params: &RiskParams) -> Decimal;
    fn name(&self) -> &str;
}

#[derive(Debug, Clone)]
pub struct RiskParams {
    pub max_position_pct: Decimal,
    pub max_daily_loss_pct: Decimal,
    pub max_drawdown_pct: Decimal,
    pub max_leverage: Decimal,
    pub volatility_target: Option<Decimal>,
    pub kelly_fraction: Option<Decimal>,
}

impl Default for RiskParams {
    fn default() -> Self {
        Self {
            max_position_pct: Decimal::new(5, 2),    // 5%
            max_daily_loss_pct: Decimal::new(2, 2),  // 2%
            max_drawdown_pct: Decimal::new(10, 2),   // 10%
            max_leverage: Decimal::new(2, 1),        // 2x
            volatility_target: None,
            kelly_fraction: None,
        }
    }
}

pub struct FixedFractionalSizer;
impl PositionSizer for FixedFractionalSizer {
    fn name(&self) -> &str { "fixed_fractional" }
    fn calculate_size(&self, signal: &SignalEntry, equity: Decimal, params: &RiskParams) -> Decimal {
        let risk_per_trade = params.max_position_pct * equity;
        if let Some(entry) = signal.entry_price {
            if let Some(stop) = signal.stop_loss {
                let risk_per_share = (entry - stop).abs();
                if risk_per_share > Decimal::ZERO {
                    return (risk_per_trade / risk_per_share).floor();
                }
            }
        }
        Decimal::ZERO
    }
}

pub struct KellySizer;
impl PositionSizer for KellySizer {
    fn name(&self) -> &str { "kelly" }
    fn calculate_size(&self, signal: &SignalEntry, equity: Decimal, params: &RiskParams) -> Decimal {
        let fraction = params.kelly_fraction.unwrap_or(Decimal::new(25, 2)); // 0.25 default
        let win_rate = Decimal::from_f64_retain(0.55).unwrap_or(Decimal::ZERO); // Would come from strategy stats
        let win_loss_ratio = Decimal::from_f64_retain(1.5).unwrap_or(Decimal::ZERO); // Would come from strategy stats

        if win_loss_ratio > Decimal::ZERO {
            let kelly_pct = (win_rate * win_loss_ratio - (Decimal::ONE - win_rate)) / win_loss_ratio;
            let adjusted = kelly_pct * fraction;
            let max_pct = params.max_position_pct.min(adjusted.max(Decimal::ZERO));
            (equity * max_pct / signal.entry_price.unwrap_or(Decimal::ONE)).floor()
        } else {
            Decimal::ZERO
        }
    }
}

pub struct VolatilityTargetSizer;
impl PositionSizer for VolatilityTargetSizer {
    fn name(&self) -> &str { "volatility_target" }
    fn calculate_size(&self, signal: &SignalEntry, equity: Decimal, params: &RiskParams) -> Decimal {
        let target_vol = params.volatility_target.unwrap_or(Decimal::new(15, 2)); // 15% annual
        let asset_vol = Decimal::from_f64_retain(0.25).unwrap_or(Decimal::ZERO); // Would come from market data
        if asset_vol > Decimal::ZERO {
            let leverage = target_vol / asset_vol;
            let max_pct = (params.max_position_pct * leverage).min(params.max_position_pct);
            (equity * max_pct / signal.entry_price.unwrap_or(Decimal::ONE)).floor()
        } else {
            Decimal::ZERO
        }
    }
}

pub struct FixedNotionalSizer;
impl PositionSizer for FixedNotionalSizer {
    fn name(&self) -> &str { "fixed_notional" }
    fn calculate_size(&self, signal: &SignalEntry, equity: Decimal, params: &RiskParams) -> Decimal {
        let notional = params.max_position_pct * equity;
        (notional / signal.entry_price.unwrap_or(Decimal::ONE)).floor()
    }
}

impl StrategyRiskManager {
    pub fn new(risk_manager: Arc<RiskManager>) -> Self {
        let mut sizers: HashMap<String, Box<dyn PositionSizer>> = HashMap::new();
        sizers.insert("fixed_fractional".to_string(), Box::new(FixedFractionalSizer));
        sizers.insert("kelly".to_string(), Box::new(KellySizer));
        sizers.insert("kelly_fraction".to_string(), Box::new(KellySizer));
        sizers.insert("volatility_target".to_string(), Box::new(VolatilityTargetSizer));
        sizers.insert("fixed_notional".to_string(), Box::new(FixedNotionalSizer));

        Self {
            risk_manager,
            strategy_limits: Arc::new(RwLock::new(HashMap::new())),
            strategy_metrics: Arc::new(RwLock::new(HashMap::new())),
            symbol_limits: Arc::new(RwLock::new(HashMap::new())),
            position_sizers: Arc::new(RwLock::new(sizers)),
        }
    }

    pub async fn validate_signal(&self, signal: &SignalEntry) -> RiskCheckResult {
        // Check strategy-level limits
        if let Some(limits) = self.strategy_limits.read().await.get(&signal.strategy_id) {
            let metrics = self.strategy_metrics.read().await.get(&signal.strategy_id).cloned().unwrap_or_default();

            // Max open positions
            if metrics.open_positions >= limits.max_open_positions {
                return RiskCheckResult::Reject(
                    format!("Strategy {} max open positions ({}) reached",
                        signal.strategy_id, limits.max_open_positions)
                );
            }

            // Max daily trades
            if metrics.daily_trades as usize >= limits.max_daily_trades {
                return RiskCheckResult::Reject(
                    format!("Strategy {} max daily trades ({}) reached",
                        signal.strategy_id, limits.max_daily_trades)
                );
            }

            // Strategy drawdown
            if metrics.peak_pnl > Decimal::ZERO {
                let drawdown = (metrics.peak_pnl - metrics.current_pnl) / metrics.peak_pnl;
                if drawdown > limits.max_drawdown_pct {
                    return RiskCheckResult::Reject(
                        format!("Strategy {} drawdown {:.2}% exceeds limit {:.2}%",
                            signal.strategy_id, drawdown * Decimal::new(100, 0),
                            limits.max_drawdown_pct * Decimal::new(100, 0))
                    );
                }
            }
        }

        // Check symbol-level limits
        if let Some(_symbol_limits) = self.symbol_limits.read().await.get(&signal.symbol.normalized()) {
            // Would check current position size for this symbol
            // Implementation depends on position tracking
        }

        // Delegate to global risk manager for account-level checks
        let order = self.signal_to_order(signal);
        self.risk_manager.validate_order(&order).await
    }

    pub async fn size_position(&self, signal: &SignalEntry, equity: Decimal) -> Decimal {
        let sizers = self.position_sizers.read().await;
        let sizing_method = "fixed_fractional"; // Would come from strategy config

        if let Some(sizer) = sizers.get(sizing_method) {
            let params = RiskParams::default(); // Would come from strategy/risk config
            sizer.calculate_size(signal, equity, &params)
        } else {
            Decimal::ZERO
        }
    }

    pub async fn update_strategy_pnl(&self, strategy_id: &str, pnl: Decimal) {
        let mut metrics = self.strategy_metrics.write().await;
        let entry = metrics.entry(strategy_id.to_string()).or_default();
        entry.current_pnl = pnl;
        if pnl > entry.peak_pnl {
            entry.peak_pnl = pnl;
        }
    }

    pub async fn record_trade(&self, strategy_id: &str) {
        let mut metrics = self.strategy_metrics.write().await;
        let entry = metrics.entry(strategy_id.to_string()).or_default();
        entry.daily_trades += 1;
    }

    pub async fn increment_positions(&self, strategy_id: &str) {
        let mut metrics = self.strategy_metrics.write().await;
        let entry = metrics.entry(strategy_id.to_string()).or_default();
        entry.open_positions += 1;
    }

    pub async fn decrement_positions(&self, strategy_id: &str) {
        let mut metrics = self.strategy_metrics.write().await;
        if let Some(entry) = metrics.get_mut(strategy_id) {
            if entry.open_positions > 0 {
                entry.open_positions -= 1;
            }
        }
    }

    pub async fn reset_daily_metrics(&self) {
        let mut metrics = self.strategy_metrics.write().await;
        let now = Utc::now();
        for entry in metrics.values_mut() {
            if (now - entry.last_reset).num_days() >= 1 {
                entry.daily_pnl = Decimal::ZERO;
                entry.daily_trades = 0;
                entry.last_reset = now;
            }
        }
    }

    pub async fn set_strategy_limits(&self, strategy_id: &str, limits: StrategyRiskLimits) {
        self.strategy_limits.write().await.insert(strategy_id.to_string(), limits);
    }

    pub async fn set_symbol_limits(&self, symbol: &str, limits: SymbolRiskLimits) {
        self.symbol_limits.write().await.insert(symbol.to_string(), limits);
    }

    pub async fn register_sizer(&self, name: &str, sizer: Box<dyn PositionSizer>) {
        self.position_sizers.write().await.insert(name.to_string(), sizer);
    }

    fn signal_to_order(&self, signal: &SignalEntry) -> Order {
        let mut order = Order::new(
            signal.symbol.clone(),
            signal.side,
            OrderType::Market,
            signal.quantity,
        );
        order = order.with_client_id(signal.signal_id.to_string());
        order
    }

    pub async fn check_kill_conditions(&self) -> Option<KillReason> {
        // Check global daily loss
        let daily_pnl = self.risk_manager.get_daily_pnl().await;
        if daily_pnl < Decimal::ZERO {
            let account = self.risk_manager.get_account().await;
            if let Some(acc) = account {
                let loss_pct = daily_pnl.abs() / acc.equity;
                let limits = self.risk_manager.get_limits().await;
                if loss_pct > limits.global.max_daily_loss_pct {
                    return Some(KillReason::DailyLossLimit);
                }
            }
        }

        // Check max drawdown
        let peak = self.risk_manager.get_peak_equity().await;
        let account = self.risk_manager.get_account().await;
        if let Some(acc) = account {
            if peak > Decimal::ZERO {
                let drawdown = (peak - acc.equity) / peak;
                let limits = self.risk_manager.get_limits().await;
                if drawdown > limits.global.max_drawdown_pct {
                    return Some(KillReason::MaxDrawdown);
                }
            }
        }

        None
    }
}
