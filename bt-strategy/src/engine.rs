use crate::dsl::ast::*;
use crate::dsl::compiler::{CompiledStrategy, StrategyCompiler};
use crate::indicators::{Indicator, IndicatorInput, IndicatorOutput, create_indicator};
use bt_core::events::{Bar, SignalEntry, SignalEvent, SignalExit, ExitReason};
use bt_core::types::Side;
use chrono::{DateTime, Utc, Duration};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalSide {
    Buy,
    Sell,
    CloseLong,
    CloseShort,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub symbol: String,
    pub side: SignalSide,
    pub price: Decimal,
    pub timestamp: DateTime<Utc>,
}

pub struct SignalEngine {
    strategies: Arc<RwLock<HashMap<String, StrategyState>>>,
    compiler: StrategyCompiler,
    event_tx: broadcast::Sender<SignalEvent>,
    config: EngineConfig,
    dedup_cache: Arc<RwLock<HashMap<String, (DateTime<Utc>, SignalEntry)>>>,
}

#[derive(Debug)]
pub struct StrategyState {
    compiled: CompiledStrategy,
    indicators: HashMap<String, Box<dyn Indicator>>,
    last_signal: Option<(DateTime<Utc>, SignalEntry)>,
    position: Option<Side>,
    entry_price: Option<Decimal>,
    bars_since_entry: u64,
    daily_trades: u32,
    #[allow(dead_code)]
    last_trade_date: Option<DateTime<Utc>>,
    previous_values: HashMap<String, IndicatorOutput>,
}

#[derive(Debug, Clone)]
pub struct StrategySnapshot {
    pub name: String,
    pub position: Option<Side>,
    pub entry_price: Option<Decimal>,
    pub bars_since_entry: u64,
    pub daily_trades: u32,
    pub last_signal: Option<(DateTime<Utc>, SignalEntry)>,
}

impl From<&StrategyState> for StrategySnapshot {
    fn from(state: &StrategyState) -> Self {
        Self {
            name: state.compiled.name().to_string(),
            position: state.position,
            entry_price: state.entry_price,
            bars_since_entry: state.bars_since_entry,
            daily_trades: state.daily_trades,
            last_signal: state.last_signal.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub evaluation_interval_ms: u64,
    pub max_concurrent_strategies: usize,
    pub signal_dedup_window_ms: u64,
    pub min_signal_confidence: f64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            evaluation_interval_ms: 100,
            max_concurrent_strategies: 10,
            signal_dedup_window_ms: 1000,
            min_signal_confidence: 0.5,
        }
    }
}

impl SignalEngine {
    pub fn new(config: EngineConfig, event_tx: broadcast::Sender<SignalEvent>) -> Self {
        Self {
            strategies: Arc::new(RwLock::new(HashMap::new())),
            compiler: StrategyCompiler::new(),
            event_tx,
            config,
            dedup_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn load_strategy(&self, source: &str) -> anyhow::Result<String> {
        let compiled = self.compiler.compile(source)?;
        let name = compiled.name().to_string();

        // Create indicators
        let mut indicators = HashMap::new();
        for (ind_name, ind_def) in compiled.indicators() {
            let indicator = create_indicator(ind_def.kind.clone(), &ind_def.params);
            indicators.insert(ind_name.clone(), indicator);
        }

        let state = StrategyState {
            compiled,
            indicators,
            last_signal: None,
            position: None,
            entry_price: None,
            bars_since_entry: 0,
            daily_trades: 0,
            last_trade_date: None,
            previous_values: HashMap::new(),
        };

        self.strategies.write().await.insert(name.clone(), state);
        info!("Loaded strategy: {}", name);
        Ok(name)
    }

    pub async fn unload_strategy(&self, name: &str) -> bool {
        self.strategies.write().await.remove(name).is_some()
    }

    pub async fn on_bar(&self, bar: &Bar) {
        let mut strategies = self.strategies.write().await;

        for (name, state) in strategies.iter_mut() {
            if !state.compiled.ast.metadata.enabled {
                continue;
            }

            // Update indicators
            let input = IndicatorInput {
                open: bar.open,
                high: bar.high,
                low: bar.low,
                close: bar.close,
                volume: bar.volume,
                vwap: bar.vwap,
                timestamp: bar.timestamp,
            };

            let mut indicator_values = HashMap::new();
            for (ind_name, indicator) in &mut state.indicators {
                let output = indicator.update(&input);
                let ind_type = indicator.name();
                if let IndicatorOutput::Tuple(ref values) = output {
                    if ind_type == "BB" && values.len() == 3 {
                        indicator_values.insert(format!("{}_upper", ind_name), IndicatorOutput::Scalar(values[0]));
                        indicator_values.insert(format!("{}_middle", ind_name), IndicatorOutput::Scalar(values[1]));
                        indicator_values.insert(format!("{}_lower", ind_name), IndicatorOutput::Scalar(values[2]));
                    } else if ind_type == "MACD" && values.len() == 3 {
                        indicator_values.insert(format!("{}_line", ind_name), IndicatorOutput::Scalar(values[0]));
                        indicator_values.insert(format!("{}_signal", ind_name), IndicatorOutput::Scalar(values[1]));
                        indicator_values.insert(format!("{}_hist", ind_name), IndicatorOutput::Scalar(values[2]));
                    }
                }
                indicator_values.insert(ind_name.clone(), output);
            }

            // Add price/volume variables
            indicator_values.insert("open".to_string(), IndicatorOutput::Scalar(bar.open));
            indicator_values.insert("high".to_string(), IndicatorOutput::Scalar(bar.high));
            indicator_values.insert("low".to_string(), IndicatorOutput::Scalar(bar.low));
            indicator_values.insert("close".to_string(), IndicatorOutput::Scalar(bar.close));
            indicator_values.insert("volume".to_string(), IndicatorOutput::Scalar(bar.volume));
            if let Some(vwap) = bar.vwap {
                indicator_values.insert("vwap".to_string(), IndicatorOutput::Scalar(vwap));
            }

            // Evaluate entry/exit rules
            if let Some(signal) = self.evaluate_entry(name, state, &indicator_values, bar).await {
                self.emit_signal(signal).await;
            }

            if let Some(signal) = self.evaluate_exit(name, state, &indicator_values, bar).await {
                self.emit_signal(signal).await;
            }

            state.previous_values = indicator_values;
        }
    }

    async fn evaluate_entry(
        &self,
        name: &str,
        state: &mut StrategyState,
        values: &HashMap<String, IndicatorOutput>,
        bar: &Bar,
    ) -> Option<SignalEvent> {
        // Skip if already in position
        if state.position.is_some() {
            return None;
        }

        let compiled = &state.compiled;

        // Check long entry
        if let Some(expr) = compiled.entry_long() {
            if self.eval_expression(expr, values, &state.previous_values)? {
                let signal = SignalEntry {
                    strategy_id: name.to_string(),
                    signal_id: Uuid::new_v4(),
                    symbol: bar.symbol.clone(),
                    side: Side::Buy,
                    quantity: Decimal::ZERO, // Will be sized by risk manager
                    confidence: 0.8,
                    entry_price: Some(bar.close),
                    stop_loss: compiled.exit_stop_loss().map(|p| bar.close * (Decimal::ONE - Decimal::from_f64_retain(p).unwrap_or(Decimal::ZERO) / Decimal::new(100, 0))),
                    take_profit: compiled.exit_take_profit().map(|p| bar.close * (Decimal::ONE + Decimal::from_f64_retain(p).unwrap_or(Decimal::ZERO) / Decimal::new(100, 0))),
                    timestamp: bar.timestamp,
                    metadata: HashMap::new(),
                };

                if self.should_emit_signal(name, &signal).await {
                    state.last_signal = Some((bar.timestamp, signal.clone()));
                    state.position = Some(Side::Buy);
                    state.entry_price = Some(bar.close);
                    state.bars_since_entry = 0;
                    return Some(SignalEvent::Entry(signal));
                }
            }
        }

        // Check short entry
        if let Some(expr) = compiled.entry_short() {
            if self.eval_expression(expr, values, &state.previous_values)? {
                let signal = SignalEntry {
                    strategy_id: name.to_string(),
                    signal_id: Uuid::new_v4(),
                    symbol: bar.symbol.clone(),
                    side: Side::Sell,
                    quantity: Decimal::ZERO,
                    confidence: 0.8,
                    entry_price: Some(bar.close),
                    stop_loss: compiled.exit_stop_loss().map(|p| bar.close * (Decimal::ONE + Decimal::from_f64_retain(p).unwrap_or(Decimal::ZERO) / Decimal::new(100, 0))),
                    take_profit: compiled.exit_take_profit().map(|p| bar.close * (Decimal::ONE - Decimal::from_f64_retain(p).unwrap_or(Decimal::ZERO) / Decimal::new(100, 0))),
                    timestamp: bar.timestamp,
                    metadata: HashMap::new(),
                };

                if self.should_emit_signal(name, &signal).await {
                    state.last_signal = Some((bar.timestamp, signal.clone()));
                    state.position = Some(Side::Sell);
                    state.entry_price = Some(bar.close);
                    state.bars_since_entry = 0;
                    return Some(SignalEvent::Entry(signal));
                }
            }
        }

        None
    }

    async fn evaluate_exit(
        &self,
        name: &str,
        state: &mut StrategyState,
        values: &HashMap<String, IndicatorOutput>,
        bar: &Bar,
    ) -> Option<SignalEvent> {
        let position = state.position?;
        let entry_price = state.entry_price?;
        let compiled = &state.compiled;

        state.bars_since_entry += 1;

        // Check stop loss
        if let Some(sl_pct) = compiled.exit_stop_loss() {
            let sl_price = if position == Side::Buy {
                entry_price * (Decimal::ONE - Decimal::from_f64_retain(sl_pct).unwrap_or(Decimal::ZERO) / Decimal::new(100, 0))
            } else {
                entry_price * (Decimal::ONE + Decimal::from_f64_retain(sl_pct).unwrap_or(Decimal::ZERO) / Decimal::new(100, 0))
            };

            let hit = match position {
                Side::Buy => bar.low <= sl_price,
                Side::Sell => bar.high >= sl_price,
            };

            if hit {
                state.position = None;
                return Some(SignalEvent::Exit(SignalExit {
                    strategy_id: name.to_string(),
                    signal_id: Uuid::new_v4(),
                    symbol: bar.symbol.clone(),
                    reason: ExitReason::StopLoss,
                    timestamp: bar.timestamp,
                }));
            }
        }

        // Check take profit
        if let Some(tp_pct) = compiled.exit_take_profit() {
            let tp_price = if position == Side::Buy {
                entry_price * (Decimal::ONE + Decimal::from_f64_retain(tp_pct).unwrap_or(Decimal::ZERO) / Decimal::new(100, 0))
            } else {
                entry_price * (Decimal::ONE - Decimal::from_f64_retain(tp_pct).unwrap_or(Decimal::ZERO) / Decimal::new(100, 0))
            };

            let hit = match position {
                Side::Buy => bar.high >= tp_price,
                Side::Sell => bar.low <= tp_price,
            };

            if hit {
                state.position = None;
                return Some(SignalEvent::Exit(SignalExit {
                    strategy_id: name.to_string(),
                    signal_id: Uuid::new_v4(),
                    symbol: bar.symbol.clone(),
                    reason: ExitReason::TakeProfit,
                    timestamp: bar.timestamp,
                }));
            }
        }

        // Check time exit
        if let Some(time_exit) = compiled.ast.exit_rules.time_exit_minutes {
            let timeframe_minutes = match compiled.timeframe() {
                Some("1m") => 1,
                Some("5m") => 5,
                Some("15m") => 15,
                Some("30m") => 30,
                Some("1h") => 60,
                Some("4h") => 240,
                _ => 5,
            };
            if state.bars_since_entry * timeframe_minutes >= time_exit {
                state.position = None;
                return Some(SignalEvent::Exit(SignalExit {
                    strategy_id: name.to_string(),
                    signal_id: Uuid::new_v4(),
                    symbol: bar.symbol.clone(),
                    reason: ExitReason::TimeExit,
                    timestamp: bar.timestamp,
                }));
            }
        }

        // Check signal reversal (long_exit / short_exit)
        let exit_expr = match position {
            Side::Buy => compiled.ast.exit_rules.long_exit.as_ref(),
            Side::Sell => compiled.ast.exit_rules.short_exit.as_ref(),
        };

        if let Some(expr) = exit_expr {
            if self.eval_expression(expr, values, &state.previous_values)? {
                state.position = None;
                return Some(SignalEvent::Exit(SignalExit {
                    strategy_id: name.to_string(),
                    signal_id: Uuid::new_v4(),
                    symbol: bar.symbol.clone(),
                    reason: ExitReason::SignalReversal,
                    timestamp: bar.timestamp,
                }));
            }
        }

        None
    }

    fn eval_expression(&self, expr: &Expression, values: &HashMap<String, IndicatorOutput>, previous_values: &HashMap<String, IndicatorOutput>) -> Option<bool> {
        match expr {
            Expression::Literal(v) => Some(*v != 0.0),
            Expression::Variable(name) => {
                values.get(name).and_then(|v| match v {
                    IndicatorOutput::Scalar(d) => Some(*d != Decimal::ZERO),
                    IndicatorOutput::Bool(b) => Some(*b),
                    _ => None,
                })
            }
            Expression::BinaryOp { left, op, right } => {
                let l = self.eval_numeric(left, values, previous_values)?;
                let r = self.eval_numeric(right, values, previous_values)?;

                match op {
                    BinaryOperator::Gt => Some(l > r),
                    BinaryOperator::Lt => Some(l < r),
                    BinaryOperator::Gte => Some(l >= r),
                    BinaryOperator::Lte => Some(l <= r),
                    BinaryOperator::Eq => Some((l - r).abs() < Decimal::new(1, 8)),
                    BinaryOperator::Neq => Some((l - r).abs() >= Decimal::new(1, 8)),
                    BinaryOperator::And => Some(l != Decimal::ZERO && r != Decimal::ZERO),
                    BinaryOperator::Or => Some(l != Decimal::ZERO || r != Decimal::ZERO),
                    _ => None,
                }
            }
            Expression::UnaryOp { op, expr } => {
                let v = self.eval_numeric(expr, values, previous_values)?;
                match op {
                    UnaryOperator::Not => Some(v == Decimal::ZERO),
                    UnaryOperator::Neg => Some(v != Decimal::ZERO), // Non-zero after negation
                }
            }
            Expression::FunctionCall { name, args } => {
                // Handle built-in functions
                match name.as_str() {
                    "cross_over" => {
                        if args.len() == 2 {
                            let a = self.eval_numeric(&args[0], values, previous_values)?;
                            let b = self.eval_numeric(&args[1], values, previous_values)?;
                            let prev_a = self.eval_numeric(&args[0], previous_values, previous_values)?;
                            let prev_b = self.eval_numeric(&args[1], previous_values, previous_values)?;
                            Some(prev_a <= prev_b && a > b)
                        } else { None }
                    }
                    "cross_under" => {
                        if args.len() == 2 {
                            let a = self.eval_numeric(&args[0], values, previous_values)?;
                            let b = self.eval_numeric(&args[1], values, previous_values)?;
                            let prev_a = self.eval_numeric(&args[0], previous_values, previous_values)?;
                            let prev_b = self.eval_numeric(&args[1], previous_values, previous_values)?;
                            Some(prev_a >= prev_b && a < b)
                        } else { None }
                    }
                    _ => {
                        // For other functions (e.g. math functions returning boolean) we don't have them yet
                        None
                    }
                }
            }
        }
    }

    fn eval_numeric(&self, expr: &Expression, values: &HashMap<String, IndicatorOutput>, previous_values: &HashMap<String, IndicatorOutput>) -> Option<Decimal> {
        match expr {
            Expression::Literal(v) => Some(Decimal::from_f64_retain(*v).unwrap_or(Decimal::ZERO)),
            Expression::Variable(name) => {
                values.get(name).and_then(|v| match v {
                    IndicatorOutput::Scalar(d) => Some(*d),
                    _ => None,
                })
            }
            Expression::BinaryOp { left, op, right } => {
                let l = self.eval_numeric(left, values, previous_values)?;
                let r = self.eval_numeric(right, values, previous_values)?;

                match op {
                    BinaryOperator::Add => Some(l + r),
                    BinaryOperator::Sub => Some(l - r),
                    BinaryOperator::Mul => Some(l * r),
                    BinaryOperator::Div => if r != Decimal::ZERO { Some(l / r) } else { None },
                    _ => None,
                }
            }
            Expression::UnaryOp { op, expr } => {
                let v = self.eval_numeric(expr, values, previous_values)?;
                match op {
                    UnaryOperator::Neg => Some(-v),
                    UnaryOperator::Not => Some(if v == Decimal::ZERO { Decimal::ONE } else { Decimal::ZERO }),
                }
            }
            Expression::FunctionCall { name: _, args } => {
                // For non-boolean functions, just return the first arg's value as a fallback or implement them
                if !args.is_empty() {
                    self.eval_numeric(&args[0], values, previous_values)
                } else {
                    None
                }
            }
        }
    }

    async fn should_emit_signal(&self, strategy_id: &str, signal: &SignalEntry) -> bool {
        let mut cache = self.dedup_cache.write().await;
        let key = format!("{}:{}:{}", strategy_id, signal.symbol, signal.side as u8);

        if let Some((last_time, _)) = cache.get(&key) {
            let elapsed = signal.timestamp - *last_time;
            if elapsed < Duration::milliseconds(self.config.signal_dedup_window_ms as i64) {
                return false;
            }
        }

        cache.insert(key, (signal.timestamp, signal.clone()));
        
        let now = signal.timestamp;
        cache.retain(|_, (time, _)| {
            (now - *time) < Duration::milliseconds(self.config.signal_dedup_window_ms as i64)
        });
        
        true
    }

    async fn emit_signal(&self, signal: SignalEvent) {
        if let Err(e) = self.event_tx.send(signal) {
            warn!("Failed to emit signal: {}", e);
        }
    }

    pub async fn get_strategies(&self) -> Vec<String> {
        self.strategies.read().await.keys().cloned().collect()
    }

    pub async fn get_strategy_state(&self, name: &str) -> Option<StrategySnapshot> {
        self.strategies.read().await.get(name).map(Into::into)
    }
}
