use crate::dsl::compiler::CompiledStrategy;
use crate::indicators::{Indicator, IndicatorInput, IndicatorOutput, create_indicator};
use bt_core::events::{Bar, ExitReason, Timeframe};
use bt_core::types::{Side, Symbol};
use bt_data::provider::BarsRequest;
use chrono::{DateTime, Utc, Duration};
use rust_decimal::{Decimal, MathematicalOps};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestConfig {
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub initial_capital: Decimal,
    pub commission_per_share: Decimal,
    pub commission_min: Decimal,
    pub slippage_bps: u32,
    pub spread_bps: u32,
    pub latency_ms: u64,
    pub max_positions: usize,
    pub position_sizing: PositionSizingMethod,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            start_date: None,
            end_date: None,
            initial_capital: Decimal::new(100000, 0),
            commission_per_share: Decimal::new(5, 3),
            commission_min: Decimal::new(1, 0),
            slippage_bps: 5,
            spread_bps: 2,
            latency_ms: 1,
            max_positions: 10,
            position_sizing: PositionSizingMethod::FixedFractional,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionSizingMethod {
    FixedFractional,
    Kelly,
    VolatilityTarget,
    FixedNotional,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestResult {
    pub strategy_name: String,
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub initial_capital: Decimal,
    pub final_capital: Decimal,
    pub total_return: Decimal,
    pub total_return_pct: Decimal,
    pub annualized_return: Decimal,
    pub sharpe_ratio: Decimal,
    pub sortino_ratio: Decimal,
    pub calmar_ratio: Decimal,
    pub max_drawdown: Decimal,
    pub max_drawdown_pct: Decimal,
    pub win_rate: Decimal,
    pub profit_factor: Decimal,
    pub expectancy: Decimal,
    pub total_trades: usize,
    pub winning_trades: usize,
    pub losing_trades: usize,
    pub avg_win: Decimal,
    pub avg_loss: Decimal,
    pub largest_win: Decimal,
    pub largest_loss: Decimal,
    pub avg_holding_period: Decimal,
    pub trades: Vec<TradeRecord>,
    pub equity_curve: Vec<EquityPoint>,
    pub monthly_returns: Vec<MonthlyReturn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub entry_time: DateTime<Utc>,
    pub exit_time: DateTime<Utc>,
    pub symbol: Symbol,
    pub side: Side,
    pub entry_price: Decimal,
    pub exit_price: Decimal,
    pub quantity: Decimal,
    pub pnl: Decimal,
    pub pnl_pct: Decimal,
    pub commission: Decimal,
    pub slippage: Decimal,
    pub exit_reason: ExitReason,
    pub bars_held: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquityPoint {
    pub timestamp: DateTime<Utc>,
    pub equity: Decimal,
    pub drawdown: Decimal,
    pub drawdown_pct: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlyReturn {
    pub year: i32,
    pub month: u32,
    pub return_pct: Decimal,
}

pub struct BacktestEngine {
    config: BacktestConfig,
    data_provider: Option<Arc<dyn bt_data::provider::HistoricalDataProvider>>,
}

impl BacktestEngine {
    pub fn new(config: BacktestConfig) -> Self {
        Self { config, data_provider: None }
    }

    pub fn with_provider(config: BacktestConfig, data_provider: Arc<dyn bt_data::provider::HistoricalDataProvider>) -> Self {
        Self { config, data_provider: Some(data_provider) }
    }

    pub async fn run(&self, strategy_source: &str) -> anyhow::Result<BacktestResult> {
        let compiler = crate::dsl::compiler::StrategyCompiler::new();
        let compiled = compiler.compile(strategy_source)?;
        let strategy_name = compiled.name().to_string();

        // Load historical data
        let timeframe = self.parse_timeframe(compiled.timeframe().unwrap_or("5m"));
        let start = self.config.start_date.unwrap_or_else(|| Utc::now() - Duration::days(365));
        let end = self.config.end_date.unwrap_or_else(Utc::now);

        // We need a symbol to test - in practice this comes from strategy metadata
        let symbol = self.get_test_symbol(&compiled)?;

        let bars = if let Some(provider) = &self.data_provider {
            provider.get_bars(BarsRequest {
                symbol: symbol.clone(),
                timeframe,
                start,
                end,
                limit: None,
            }).await?
        } else {
            vec![]
        };

        if bars.is_empty() {
            anyhow::bail!("No historical data available for backtest");
        }

        info!("Running backtest for {} on {} bars", strategy_name, bars.len());

        // Initialize strategy state
        let mut indicators = HashMap::new();
        for (ind_name, ind_def) in compiled.indicators() {
            let indicator = create_indicator(ind_def.kind.clone(), &ind_def.params);
            indicators.insert(ind_name.clone(), indicator);
        }

        // Run simulation
        let mut simulator = BacktestSimulator::new(
            self.config.clone(),
            compiled.clone(),
            indicators,
            bars,
            symbol,
        );

        simulator.run()?;

        Ok(simulator.build_result(strategy_name, start, end))
    }

    fn parse_timeframe(&self, tf: &str) -> Timeframe {
        match tf {
            "1m" | "minute" => Timeframe::Minute,
            "5m" => Timeframe::Minute5,
            "15m" => Timeframe::Minute15,
            "30m" => Timeframe::Minute30,
            "1h" | "hour" => Timeframe::Hour,
            "4h" => Timeframe::Hour4,
            "1d" | "day" => Timeframe::Day,
            _ => Timeframe::Minute5,
        }
    }

    fn get_test_symbol(&self, compiled: &CompiledStrategy) -> anyhow::Result<Symbol> {
        if let Some(universe) = &compiled.ast.metadata.universe {
            Ok(universe.parse()?)
        } else {
            Ok("SPY".parse()?)
        }
    }
}

struct BacktestSimulator {
    config: BacktestConfig,
    compiled: CompiledStrategy,
    indicators: HashMap<String, Box<dyn Indicator>>,
    bars: Vec<Bar>,
    symbol: Symbol,
    equity: Decimal,
    peak_equity: Decimal,
    position: Option<PositionState>,
    trades: Vec<TradeRecord>,
    equity_curve: Vec<EquityPoint>,
    current_bar_idx: usize,
    previous_values: HashMap<String, IndicatorOutput>,
}

#[derive(Debug, Clone)]
struct PositionState {
    side: Side,
    entry_price: Decimal,
    quantity: Decimal,
    entry_time: DateTime<Utc>,
    entry_bar_idx: usize,
    stop_loss: Option<Decimal>,
    take_profit: Option<Decimal>,
}

impl BacktestSimulator {
    fn new(
        config: BacktestConfig,
        compiled: CompiledStrategy,
        indicators: HashMap<String, Box<dyn Indicator>>,
        bars: Vec<Bar>,
        symbol: Symbol,
    ) -> Self {
        let equity = config.initial_capital;
        Self {
            config,
            compiled,
            indicators,
            bars,
            symbol,
            equity,
            peak_equity: equity,
            position: None,
            trades: Vec::new(),
            equity_curve: Vec::new(),
            current_bar_idx: 0,
            previous_values: HashMap::new(),
        }
    }

    fn run(&mut self) -> anyhow::Result<()> {
        for idx in 0..self.bars.len() {
            let bar = self.bars[idx].clone();
            self.current_bar_idx = idx;

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

            let mut values = HashMap::new();
            for (name, indicator) in &mut self.indicators {
                let output = indicator.update(&input);
                let ind_type = indicator.name();
                if let IndicatorOutput::Tuple(ref tup_vals) = output {
                    if ind_type == "BB" && tup_vals.len() == 3 {
                        values.insert(format!("{}_upper", name), IndicatorOutput::Scalar(tup_vals[0]));
                        values.insert(format!("{}_middle", name), IndicatorOutput::Scalar(tup_vals[1]));
                        values.insert(format!("{}_lower", name), IndicatorOutput::Scalar(tup_vals[2]));
                    } else if ind_type == "MACD" && tup_vals.len() == 3 {
                        values.insert(format!("{}_line", name), IndicatorOutput::Scalar(tup_vals[0]));
                        values.insert(format!("{}_signal", name), IndicatorOutput::Scalar(tup_vals[1]));
                        values.insert(format!("{}_hist", name), IndicatorOutput::Scalar(tup_vals[2]));
                    }
                }
                values.insert(name.clone(), output);
            }

            // Add price variables
            values.insert("open".to_string(), IndicatorOutput::Scalar(bar.open));
            values.insert("high".to_string(), IndicatorOutput::Scalar(bar.high));
            values.insert("low".to_string(), IndicatorOutput::Scalar(bar.low));
            values.insert("close".to_string(), IndicatorOutput::Scalar(bar.close));
            values.insert("volume".to_string(), IndicatorOutput::Scalar(bar.volume));
            if let Some(vwap) = bar.vwap {
                values.insert("vwap".to_string(), IndicatorOutput::Scalar(vwap));
            }

            // Check exits first
            self.check_exits(&bar, &values);

            // Check entries
            self.check_entries(&bar, &values);

            // Record equity
            let unrealized = self.unrealized_pnl(&bar);
            let total_equity = self.equity + unrealized;
            let drawdown = self.peak_equity - total_equity;
            let drawdown_pct = if self.peak_equity > Decimal::ZERO {
                drawdown / self.peak_equity
            } else { Decimal::ZERO };

            if total_equity > self.peak_equity {
                self.peak_equity = total_equity;
            }

            self.equity_curve.push(EquityPoint {
                timestamp: bar.timestamp,
                equity: total_equity,
                drawdown,
                drawdown_pct,
            });

            self.previous_values = values;
        }

        // Close any open position at end
        if let Some(pos) = self.position.take() {
            let Some(last_bar) = self.bars.last().cloned() else {
                return Ok(());
            };
            self.close_position(&last_bar, ExitReason::TimeExit, pos);
        }

        Ok(())
    }

    fn check_entries(&mut self, bar: &Bar, values: &HashMap<String, IndicatorOutput>) {
        if self.position.is_some() {
            return;
        }

        // Long entry
        if let Some(expr) = self.compiled.entry_long() {
            if self.eval_bool(expr, values, &self.previous_values).unwrap_or(false) {
                self.enter_position(bar, Side::Buy, values);
                return;
            }
        }

        // Short entry
        if let Some(expr) = self.compiled.entry_short() {
            if self.eval_bool(expr, values, &self.previous_values).unwrap_or(false) {
                self.enter_position(bar, Side::Sell, values);
            }
        }
    }

    fn check_exits(&mut self, bar: &Bar, values: &HashMap<String, IndicatorOutput>) {
        // Clone position to avoid borrow issues
        let pos = match self.position.clone() {
            Some(p) => p,
            None => return,
        };

        // Stop loss
        if let Some(sl) = pos.stop_loss {
            let hit = match pos.side {
                Side::Buy => bar.low <= sl,
                Side::Sell => bar.high >= sl,
            };
            if hit {
                self.close_position(bar, ExitReason::StopLoss, pos);
                return;
            }
        }

        // Take profit
        if let Some(tp) = pos.take_profit {
            let hit = match pos.side {
                Side::Buy => bar.high >= tp,
                Side::Sell => bar.low <= tp,
            };
            if hit {
                self.close_position(bar, ExitReason::TakeProfit, pos);
                return;
            }
        }

        // Time exit
        if let Some(time_exit) = self.compiled.ast.exit_rules.time_exit_minutes {
            let timeframe_minutes = match self.compiled.timeframe() {
                Some("1m") => 1,
                Some("5m") => 5,
                Some("15m") => 15,
                Some("30m") => 30,
                Some("1h") => 60,
                Some("4h") => 240,
                _ => 5,
            };
            let bars_held = self.current_bar_idx - pos.entry_bar_idx;
            if bars_held * timeframe_minutes >= time_exit as usize {
                self.close_position(bar, ExitReason::TimeExit, pos);
                return;
            }
        }

        // Signal exit
        let exit_expr = match pos.side {
            Side::Buy => self.compiled.ast.exit_rules.long_exit.as_ref(),
            Side::Sell => self.compiled.ast.exit_rules.short_exit.as_ref(),
        };

        if let Some(expr) = exit_expr {
            if self.eval_bool(expr, values, &self.previous_values).unwrap_or(false) {
                self.close_position(bar, ExitReason::SignalReversal, pos);
            }
        }
    }

    fn enter_position(&mut self, bar: &Bar, side: Side, _values: &HashMap<String, IndicatorOutput>) {
        let entry_price = self.apply_slippage(bar.close, side);
        let quantity = self.calculate_position_size(entry_price);

        if quantity == Decimal::ZERO {
            return;
        }

        let stop_loss = self.compiled.exit_stop_loss().map(|pct| {
            if side == Side::Buy {
                entry_price * (Decimal::ONE - Decimal::from_f64_retain(pct).unwrap_or(Decimal::ZERO) / Decimal::new(100, 0))
            } else {
                entry_price * (Decimal::ONE + Decimal::from_f64_retain(pct).unwrap_or(Decimal::ZERO) / Decimal::new(100, 0))
            }
        });

        let take_profit = self.compiled.exit_take_profit().map(|pct| {
            if side == Side::Buy {
                entry_price * (Decimal::ONE + Decimal::from_f64_retain(pct).unwrap_or(Decimal::ZERO) / Decimal::new(100, 0))
            } else {
                entry_price * (Decimal::ONE - Decimal::from_f64_retain(pct).unwrap_or(Decimal::ZERO) / Decimal::new(100, 0))
            }
        });

        let commission = self.calculate_commission(quantity, entry_price);
        self.equity -= commission;

        self.position = Some(PositionState {
            side,
            entry_price,
            quantity,
            entry_time: bar.timestamp,
            entry_bar_idx: self.current_bar_idx,
            stop_loss,
            take_profit,
        });
    }

    fn close_position(&mut self, bar: &Bar, reason: ExitReason, pos: PositionState) {
        let exit_price = self.apply_slippage(bar.close, pos.side.opposite());
        let commission = self.calculate_commission(pos.quantity, exit_price);

        let pnl = match pos.side {
            Side::Buy => (exit_price - pos.entry_price) * pos.quantity,
            Side::Sell => (pos.entry_price - exit_price) * pos.quantity,
        };

        let pnl_pct = if pos.entry_price != Decimal::ZERO {
            pnl / pos.entry_price
        } else { Decimal::ZERO };

        self.equity += pnl - commission;

        self.trades.push(TradeRecord {
            entry_time: pos.entry_time,
            exit_time: bar.timestamp,
            symbol: self.symbol.clone(),
            side: pos.side,
            entry_price: pos.entry_price,
            exit_price,
            quantity: pos.quantity,
            pnl,
            pnl_pct,
            commission,
            slippage: (exit_price - bar.close).abs() * pos.quantity,
            exit_reason: reason,
            bars_held: self.current_bar_idx - pos.entry_bar_idx,
        });

        self.position = None;
    }

    fn apply_slippage(&self, price: Decimal, side: Side) -> Decimal {
        let slippage = price * Decimal::from(self.config.slippage_bps) / Decimal::new(10000, 0);
        match side {
            Side::Buy => price + slippage,
            Side::Sell => price - slippage,
        }
    }

    fn calculate_commission(&self, quantity: Decimal, price: Decimal) -> Decimal {
        let notional = quantity * price;
        let per_share = self.config.commission_per_share * quantity;
        (per_share.max(self.config.commission_min)).min(notional * Decimal::new(1, 2)) // Cap at 1%
    }

    fn calculate_position_size(&self, price: Decimal) -> Decimal {
        let max_pct = self.compiled.risk_max_position_pct().unwrap_or(10.0);
        let max_position_value = self.equity * Decimal::from_f64_retain(max_pct / 100.0).unwrap_or(Decimal::ZERO);
        
        (max_position_value / price).floor()
    }

    fn unrealized_pnl(&self, bar: &Bar) -> Decimal {
        let Some(pos) = &self.position else { return Decimal::ZERO };
        match pos.side {
            Side::Buy => (bar.close - pos.entry_price) * pos.quantity,
            Side::Sell => (pos.entry_price - bar.close) * pos.quantity,
        }
    }

    fn eval_bool(&self, expr: &crate::dsl::ast::Expression, values: &HashMap<String, IndicatorOutput>, previous_values: &HashMap<String, IndicatorOutput>) -> Option<bool> {
        match expr {
            crate::dsl::ast::Expression::Literal(v) => Some(*v != 0.0),
            crate::dsl::ast::Expression::Variable(name) => {
                values.get(name).and_then(|v| match v {
                    IndicatorOutput::Scalar(d) => Some(*d != Decimal::ZERO),
                    IndicatorOutput::Bool(b) => Some(*b),
                    _ => None,
                })
            }
            crate::dsl::ast::Expression::BinaryOp { left, op, right } => {
                match op {
                    crate::dsl::ast::BinaryOperator::And => {
                        let l = self.eval_bool(left, values, previous_values)?;
                        let r = self.eval_bool(right, values, previous_values)?;
                        Some(l && r)
                    }
                    crate::dsl::ast::BinaryOperator::Or => {
                        let l = self.eval_bool(left, values, previous_values)?;
                        let r = self.eval_bool(right, values, previous_values)?;
                        Some(l || r)
                    }
                    _ => {
                        let l = self.eval_numeric(left, values, previous_values)?;
                        let r = self.eval_numeric(right, values, previous_values)?;
                        match op {
                            crate::dsl::ast::BinaryOperator::Gt => Some(l > r),
                            crate::dsl::ast::BinaryOperator::Lt => Some(l < r),
                            crate::dsl::ast::BinaryOperator::Gte => Some(l >= r),
                            crate::dsl::ast::BinaryOperator::Lte => Some(l <= r),
                            crate::dsl::ast::BinaryOperator::Eq => Some((l - r).abs() < Decimal::new(1, 8)),
                            crate::dsl::ast::BinaryOperator::Neq => Some((l - r).abs() >= Decimal::new(1, 8)),
                            _ => None,
                        }
                    }
                }
            }
            crate::dsl::ast::Expression::UnaryOp { op, expr } => {
                let v = self.eval_numeric(expr, values, previous_values)?;
                match op {
                    crate::dsl::ast::UnaryOperator::Not => Some(v == Decimal::ZERO),
                    crate::dsl::ast::UnaryOperator::Neg => Some(v != Decimal::ZERO),
                }
            }
            crate::dsl::ast::Expression::FunctionCall { name, args } => {
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
                    _ => None,
                }
            }
        }
    }

    #[allow(clippy::only_used_in_recursion)]
    fn eval_numeric(&self, expr: &crate::dsl::ast::Expression, values: &HashMap<String, IndicatorOutput>, previous_values: &HashMap<String, IndicatorOutput>) -> Option<Decimal> {
        match expr {
            crate::dsl::ast::Expression::Literal(v) => Some(Decimal::from_f64_retain(*v).unwrap_or(Decimal::ZERO)),
            crate::dsl::ast::Expression::Variable(name) => {
                values.get(name).and_then(|v| match v {
                    IndicatorOutput::Scalar(d) => Some(*d),
                    _ => None,
                })
            }
            crate::dsl::ast::Expression::BinaryOp { left, op, right } => {
                let l = self.eval_numeric(left, values, previous_values)?;
                let r = self.eval_numeric(right, values, previous_values)?;
                match op {
                    crate::dsl::ast::BinaryOperator::Add => Some(l + r),
                    crate::dsl::ast::BinaryOperator::Sub => Some(l - r),
                    crate::dsl::ast::BinaryOperator::Mul => Some(l * r),
                    crate::dsl::ast::BinaryOperator::Div => if r != Decimal::ZERO { Some(l / r) } else { None },
                    _ => None,
                }
            }
            crate::dsl::ast::Expression::UnaryOp { op, expr } => {
                let v = self.eval_numeric(expr, values, previous_values)?;
                match op {
                    crate::dsl::ast::UnaryOperator::Neg => Some(-v),
                    crate::dsl::ast::UnaryOperator::Not => Some(if v == Decimal::ZERO { Decimal::ONE } else { Decimal::ZERO }),
                }
            }
            crate::dsl::ast::Expression::FunctionCall { name: _, args } => {
                if !args.is_empty() {
                    self.eval_numeric(&args[0], values, previous_values)
                } else {
                    None
                }
            }
        }
    }

    fn build_result(&self, strategy_name: String, start: DateTime<Utc>, end: DateTime<Utc>) -> BacktestResult {
        let total_return = self.equity - self.config.initial_capital;
        let total_return_pct = if self.config.initial_capital != Decimal::ZERO {
            total_return / self.config.initial_capital
        } else { Decimal::ZERO };

        let winning_trades = self.trades.iter().filter(|t| t.pnl > Decimal::ZERO).count();
        let losing_trades = self.trades.len() - winning_trades;
        let win_rate = if !self.trades.is_empty() {
            Decimal::from(winning_trades) / Decimal::from(self.trades.len())
        } else { Decimal::ZERO };

        let gross_profit: Decimal = self.trades.iter().filter(|t| t.pnl > Decimal::ZERO).map(|t| t.pnl).sum();
        let gross_loss: Decimal = self.trades.iter().filter(|t| t.pnl < Decimal::ZERO).map(|t| t.pnl.abs()).sum();
        let profit_factor = if gross_loss != Decimal::ZERO { gross_profit / gross_loss } else { Decimal::ZERO };

        let avg_win = if winning_trades > 0 {
            gross_profit / Decimal::from(winning_trades)
        } else { Decimal::ZERO };
        let avg_loss = if losing_trades > 0 {
            gross_loss / Decimal::from(losing_trades)
        } else { Decimal::ZERO };

        let expectancy = win_rate * avg_win - (Decimal::ONE - win_rate) * avg_loss;

        let largest_win = self.trades.iter().map(|t| t.pnl).fold(Decimal::ZERO, |a, b| a.max(b));
        let largest_loss = self.trades.iter().map(|t| t.pnl).fold(Decimal::ZERO, |a, b| a.min(b));

        let avg_holding = if !self.trades.is_empty() {
            Decimal::from(self.trades.iter().map(|t| t.bars_held).sum::<usize>()) / Decimal::from(self.trades.len())
        } else { Decimal::ZERO };

        // Max drawdown
        let max_dd = self.equity_curve.iter().map(|e| e.drawdown).fold(Decimal::ZERO, |a, b| a.max(b));
        let max_dd_pct = self.equity_curve.iter().map(|e| e.drawdown_pct).fold(Decimal::ZERO, |a, b| a.max(b));

        // Sharpe ratio (simplified - daily returns)
        let sharpe = self.calculate_sharpe();
        let sortino = self.calculate_sortino();
        let calmar = if max_dd_pct != Decimal::ZERO {
            total_return_pct / max_dd_pct
        } else { Decimal::ZERO };

        // Annualized return
        let days = (end - start).num_days() as f64;
        let years = days / 365.25;
        let annualized = if years > 0.0 {
            (Decimal::ONE + total_return_pct).powf(1.0 / years) - Decimal::ONE
        } else { Decimal::ZERO };

        BacktestResult {
            strategy_name,
            start_date: start,
            end_date: end,
            initial_capital: self.config.initial_capital,
            final_capital: self.equity,
            total_return,
            total_return_pct,
            annualized_return: annualized,
            sharpe_ratio: sharpe,
            sortino_ratio: sortino,
            calmar_ratio: calmar,
            max_drawdown: max_dd,
            max_drawdown_pct: max_dd_pct,
            win_rate,
            profit_factor,
            expectancy,
            total_trades: self.trades.len(),
            winning_trades,
            losing_trades,
            avg_win,
            avg_loss,
            largest_win,
            largest_loss,
            avg_holding_period: avg_holding,
            trades: self.trades.clone(),
            equity_curve: self.equity_curve.clone(),
            monthly_returns: self.calculate_monthly_returns(),
        }
    }

    fn calculate_sharpe(&self) -> Decimal {
        if self.equity_curve.len() < 2 {
            return Decimal::ZERO;
        }

        // Daily returns
        let mut returns = Vec::new();
        for i in 1..self.equity_curve.len() {
            let prev = self.equity_curve[i - 1].equity;
            let curr = self.equity_curve[i].equity;
            if prev != Decimal::ZERO {
                returns.push((curr - prev) / prev);
            }
        }

        if returns.is_empty() {
            return Decimal::ZERO;
        }

        let mean: Decimal = returns.iter().sum::<Decimal>() / Decimal::from(returns.len());
        let variance: Decimal = returns.iter()
            .map(|r| (r - mean) * (r - mean))
            .sum::<Decimal>() / Decimal::from(returns.len());
        let std = variance.sqrt().unwrap_or(Decimal::ZERO);

        if std == Decimal::ZERO {
            Decimal::ZERO
        } else {
            // Annualize (assuming 252 trading days)
            mean * Decimal::from_f64_retain(252.0_f64.sqrt()).unwrap_or(Decimal::ZERO) / std
        }
    }

    fn calculate_sortino(&self) -> Decimal {
        if self.equity_curve.len() < 2 {
            return Decimal::ZERO;
        }

        let mut returns = Vec::new();
        for i in 1..self.equity_curve.len() {
            let prev = self.equity_curve[i - 1].equity;
            let curr = self.equity_curve[i].equity;
            if prev != Decimal::ZERO {
                returns.push((curr - prev) / prev);
            }
        }

        let negative_returns: Vec<Decimal> = returns.iter().filter(|r| **r < Decimal::ZERO).copied().collect();
        if negative_returns.is_empty() {
            return Decimal::ZERO;
        }

        let mean: Decimal = returns.iter().sum::<Decimal>() / Decimal::from(returns.len());
        let downside_variance: Decimal = negative_returns.iter()
            .map(|r| (r - mean) * (r - mean))
            .sum::<Decimal>() / Decimal::from(negative_returns.len());
        let downside_std = downside_variance.sqrt().unwrap_or(Decimal::ZERO);

        if downside_std == Decimal::ZERO {
            Decimal::ZERO
        } else {
            mean * Decimal::from_f64_retain(252.0_f64.sqrt()).unwrap_or(Decimal::ZERO) / downside_std
        }
    }

    fn calculate_monthly_returns(&self) -> Vec<MonthlyReturn> {
        // Simplified - would group equity curve by month
        Vec::new()
    }
}
