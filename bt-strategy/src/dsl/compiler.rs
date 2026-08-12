use crate::dsl::ast::*;
use crate::dsl::parser::{StrategyParser, Rule};
use pest::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CompileError {
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Semantic error: {0}")]
    Semantic(String),
    #[error("Unknown indicator: {0}")]
    UnknownIndicator(String),
    #[error("Unknown variable: {0}")]
    UnknownVariable(String),
    #[error("Type error: {0}")]
    Type(String),
}

pub type CompileResult<T> = Result<T, CompileError>;

pub struct StrategyCompiler {
    builtin_indicators: HashMap<String, IndicatorInfo>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct IndicatorInfo {
    name: String,
    min_params: usize,
    max_params: usize,
    output_type: IndicatorOutputType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IndicatorOutputType {
    Scalar,
    Tuple(usize),
}

impl StrategyCompiler {
    pub fn new() -> Self {
        let mut builtin = HashMap::new();
        builtin.insert("RSI".to_string(), IndicatorInfo { name: "RSI".to_string(), min_params: 0, max_params: 1, output_type: IndicatorOutputType::Scalar });
        builtin.insert("EMA".to_string(), IndicatorInfo { name: "EMA".to_string(), min_params: 0, max_params: 1, output_type: IndicatorOutputType::Scalar });
        builtin.insert("SMA".to_string(), IndicatorInfo { name: "SMA".to_string(), min_params: 0, max_params: 1, output_type: IndicatorOutputType::Scalar });
        builtin.insert("BOLLINGER".to_string(), IndicatorInfo { name: "BOLLINGER".to_string(), min_params: 0, max_params: 2, output_type: IndicatorOutputType::Tuple(3) });
        builtin.insert("BB".to_string(), IndicatorInfo { name: "BB".to_string(), min_params: 0, max_params: 2, output_type: IndicatorOutputType::Tuple(3) });
        builtin.insert("MACD".to_string(), IndicatorInfo { name: "MACD".to_string(), min_params: 0, max_params: 3, output_type: IndicatorOutputType::Tuple(3) });
        builtin.insert("ATR".to_string(), IndicatorInfo { name: "ATR".to_string(), min_params: 0, max_params: 1, output_type: IndicatorOutputType::Scalar });
        builtin.insert("VWAP".to_string(), IndicatorInfo { name: "VWAP".to_string(), min_params: 0, max_params: 0, output_type: IndicatorOutputType::Scalar });
        builtin.insert("VOLUME_SMA".to_string(), IndicatorInfo { name: "VOLUME_SMA".to_string(), min_params: 0, max_params: 1, output_type: IndicatorOutputType::Scalar });
        builtin.insert("STDDEV".to_string(), IndicatorInfo { name: "STDDEV".to_string(), min_params: 0, max_params: 1, output_type: IndicatorOutputType::Scalar });
        builtin.insert("HIGHEST".to_string(), IndicatorInfo { name: "HIGHEST".to_string(), min_params: 0, max_params: 1, output_type: IndicatorOutputType::Scalar });
        builtin.insert("LOWEST".to_string(), IndicatorInfo { name: "LOWEST".to_string(), min_params: 0, max_params: 1, output_type: IndicatorOutputType::Scalar });
        builtin.insert("CROSS".to_string(), IndicatorInfo { name: "CROSS".to_string(), min_params: 2, max_params: 2, output_type: IndicatorOutputType::Scalar });
        builtin.insert("CROSS_OVER".to_string(), IndicatorInfo { name: "CROSS_OVER".to_string(), min_params: 2, max_params: 2, output_type: IndicatorOutputType::Scalar });
        builtin.insert("CROSS_UNDER".to_string(), IndicatorInfo { name: "CROSS_UNDER".to_string(), min_params: 2, max_params: 2, output_type: IndicatorOutputType::Scalar });

        Self { builtin_indicators: builtin }
    }

    pub fn compile(&self, source: &str) -> CompileResult<CompiledStrategy> {
        let pairs = StrategyParser::parse(Rule::strategy, source)
            .map_err(|e| CompileError::Parse(e.to_string()))?;

        let mut strategy = Strategy {
            name: String::new(),
            metadata: StrategyMetadata::default(),
            indicators: HashMap::new(),
            entry_rules: EntryRules::default(),
            exit_rules: ExitRules::default(),
            risk_rules: RiskRules::default(),
        };

        for pair in pairs {
            if pair.as_rule() == Rule::strategy {
                for inner in pair.into_inner() {
                    match inner.as_rule() {
                        Rule::metadata => self.parse_metadata(inner, &mut strategy)?,
                        Rule::indicator_block => self.parse_indicators(inner, &mut strategy)?,
                        Rule::entry_block => self.parse_entry(inner, &mut strategy)?,
                        Rule::exit_block => self.parse_exit(inner, &mut strategy)?,
                        Rule::risk_block => self.parse_risk(inner, &mut strategy)?,
                        _ => {}
                    }
                }
            }
        }

        self.validate(&strategy)?;
        Ok(CompiledStrategy { ast: strategy })
    }

    fn parse_metadata(&self, pair: pest::iterators::Pair<Rule>, strategy: &mut Strategy) -> CompileResult<()> {
        for inner in pair.into_inner() {
            match inner.as_rule() {
                Rule::string => strategy.name = inner.as_str().trim_matches('"').to_string(),
                Rule::metadata_field => {
                    let key = inner.as_str().split(':').next().unwrap().trim();
                    let value_pair = inner.into_inner().next().unwrap();
                    let value = value_pair.as_str();

                    match key {
                        "universe" => strategy.metadata.universe = Some(value.trim_matches('"').to_string()),
                        "timeframe" => strategy.metadata.timeframe = Some(value.trim_matches('"').to_string()),
                        "session" => strategy.metadata.session = Some(value.trim_matches('"').to_string()),
                        "enabled" => strategy.metadata.enabled = value == "true",
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn parse_indicators(&self, pair: pest::iterators::Pair<Rule>, strategy: &mut Strategy) -> CompileResult<()> {
        for inner in pair.into_inner() {
            if inner.as_rule() == Rule::indicator_def {
                let mut def_iter = inner.into_inner();
                let name = def_iter.next().unwrap().as_str().to_string();
                let call = def_iter.next().unwrap();
                let indicator = self.parse_indicator_call(call)?;
                strategy.indicators.insert(name, indicator);
            }
        }
        Ok(())
    }

    fn parse_indicator_call(&self, pair: pest::iterators::Pair<Rule>) -> CompileResult<IndicatorDef> {
        let kind_str = pair.as_str().split('(').next().unwrap().trim();
        let params: Vec<f64> = pair.into_inner().map(|p| p.as_str().parse().unwrap_or(0.0)).collect();

        let kind_upper = kind_str.to_uppercase();
        let kind = match kind_upper.as_str() {
            "RSI" => IndicatorKind::RSI,
            "EMA" => IndicatorKind::EMA,
            "SMA" => IndicatorKind::SMA,
            "BOLLINGER" | "BB" => IndicatorKind::BollingerBands,
            "MACD" => IndicatorKind::MACD,
            "ATR" => IndicatorKind::ATR,
            "VWAP" => IndicatorKind::VWAP,
            "VOLUME_SMA" => IndicatorKind::VolumeSMA,
            "STDDEV" => IndicatorKind::StdDev,
            "HIGHEST" => IndicatorKind::Highest,
            "LOWEST" => IndicatorKind::Lowest,
            "CROSS" => IndicatorKind::Cross,
            "CROSS_OVER" => IndicatorKind::CrossOver,
            "CROSS_UNDER" => IndicatorKind::CrossUnder,
            _ => return Err(CompileError::UnknownIndicator(kind_str.to_string())),
        };

        if let Some(info) = self.builtin_indicators.get(&kind_upper) {
            if params.len() < info.min_params || params.len() > info.max_params {
                return Err(CompileError::Semantic(format!(
                    "Indicator {} expects {} to {} parameters, got {}",
                    kind_str, info.min_params, info.max_params, params.len()
                )));
            }
        }

        // Validate all indicator parameters against safe numerical bounds (Items 6, 10, 14)
        for &param in &params {
            if param <= 0.0 || param > 5000.0 {
                return Err(CompileError::Semantic(format!(
                    "Indicator {} requires all parameters in range 0 < param <= 5000, got {}",
                    kind_str, param
                )));
            }
        }

        Ok(IndicatorDef {
            name: kind_str.to_string(),
            kind,
            params,
        })
    }

    fn parse_entry(&self, pair: pest::iterators::Pair<Rule>, strategy: &mut Strategy) -> CompileResult<()> {
        for inner in pair.into_inner() {
            if inner.as_rule() == Rule::entry_rule {
                let side = inner.as_str().split(':').next().unwrap().trim();
                let expr = self.parse_expression(inner.into_inner().next().unwrap())?;

                match side {
                    "long" => strategy.entry_rules.long = Some(expr),
                    "short" => strategy.entry_rules.short = Some(expr),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn parse_exit(&self, pair: pest::iterators::Pair<Rule>, strategy: &mut Strategy) -> CompileResult<()> {
        for inner in pair.into_inner() {
            if inner.as_rule() == Rule::exit_rule {
                let key = inner.as_str().split(':').next().unwrap().trim();
                let val_pair = inner.into_inner().next().unwrap();

                match key {
                    "stop_loss" => strategy.exit_rules.stop_loss = val_pair.as_str().trim_end_matches('%').parse().ok(),
                    "take_profit" => strategy.exit_rules.take_profit = val_pair.as_str().trim_end_matches('%').parse().ok(),
                    "trailing_stop" => strategy.exit_rules.trailing_stop = val_pair.as_str().trim_end_matches('%').parse().ok(),
                    "long_exit" => strategy.exit_rules.long_exit = Some(self.parse_expression(val_pair)?),
                    "short_exit" => strategy.exit_rules.short_exit = Some(self.parse_expression(val_pair)?),
                    "time_exit" => strategy.exit_rules.time_exit_minutes = val_pair.as_str().trim_end_matches("min").trim().parse().ok(),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn parse_risk(&self, pair: pest::iterators::Pair<Rule>, strategy: &mut Strategy) -> CompileResult<()> {
        for inner in pair.into_inner() {
            if inner.as_rule() == Rule::risk_rule {
                let key = inner.as_str().split(':').next().unwrap().trim();
                let val_pair = inner.into_inner().next().unwrap();
                let value = val_pair.as_str();

                match key {
                    "max_position" => strategy.risk_rules.max_position_pct = value.trim_end_matches('%').parse().ok(),
                    "max_daily_loss" => strategy.risk_rules.max_daily_loss_pct = value.trim_end_matches('%').parse().ok(),
                    "max_drawdown" => strategy.risk_rules.max_drawdown_pct = value.trim_end_matches('%').parse().ok(),
                    "max_correlation" => strategy.risk_rules.max_correlation = value.parse().ok(),
                    "position_sizing" => strategy.risk_rules.position_sizing = Some(value.trim_matches('"').to_string()),
                    "max_leverage" => strategy.risk_rules.max_leverage = value.parse().ok(),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn parse_expression(&self, pair: pest::iterators::Pair<Rule>) -> CompileResult<Expression> {
        match pair.as_rule() {
            Rule::expression | Rule::logical_expr => self.parse_logical(pair),
            Rule::comparison => self.parse_comparison(pair),
            Rule::term => self.parse_term(pair),
            Rule::factor => self.parse_factor(pair),
            Rule::unary => self.parse_unary(pair),
            _ => self.parse_primary(pair),
        }
    }

    fn parse_logical(&self, pair: pest::iterators::Pair<Rule>) -> CompileResult<Expression> {
        let inner_pair = match pair.as_rule() {
            Rule::expression => pair.into_inner().next().ok_or_else(|| CompileError::Parse("Unexpected end of expression".to_string()))?,
            _ => pair,
        };
        let mut inner = inner_pair.into_inner();
        let mut left = self.parse_comparison(inner.next().ok_or_else(|| CompileError::Parse("Unexpected end of expression".to_string()))?)?;

        while let (Some(op_pair), Some(right_pair)) = (inner.next(), inner.next()) {
            let op = match op_pair.as_str() {
                "&&" | "AND" => BinaryOperator::And,
                "||" | "OR" => BinaryOperator::Or,
                _ => continue,
            };
            let right = self.parse_comparison(right_pair)?;
            left = Expression::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_comparison(&self, pair: pest::iterators::Pair<Rule>) -> CompileResult<Expression> {
        let mut inner = pair.into_inner();
        let mut left = self.parse_term(inner.next().ok_or_else(|| CompileError::Parse("Unexpected end of expression".to_string()))?)?;

        while let (Some(op_pair), Some(right_pair)) = (inner.next(), inner.next()) {
            let op_str = op_pair.as_str();
            let right = self.parse_term(right_pair)?;

            let op = match op_str {
                ">" => BinaryOperator::Gt,
                "<" => BinaryOperator::Lt,
                ">=" => BinaryOperator::Gte,
                "<=" => BinaryOperator::Lte,
                "==" => BinaryOperator::Eq,
                "!=" => BinaryOperator::Neq,
                _ => return Err(CompileError::Semantic(format!("Unknown operator: {}", op_str))),
            };

            left = Expression::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_term(&self, pair: pest::iterators::Pair<Rule>) -> CompileResult<Expression> {
        let mut inner = pair.into_inner();
        let mut left = self.parse_factor(inner.next().ok_or_else(|| CompileError::Parse("Unexpected end of expression".to_string()))?)?;

        while let (Some(op_pair), Some(right_pair)) = (inner.next(), inner.next()) {
            let op_str = op_pair.as_str();
            let right = self.parse_factor(right_pair)?;

            let op = match op_str {
                "+" => BinaryOperator::Add,
                "-" => BinaryOperator::Sub,
                _ => continue,
            };

            left = Expression::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_factor(&self, pair: pest::iterators::Pair<Rule>) -> CompileResult<Expression> {
        let mut inner = pair.into_inner();
        let mut left = self.parse_unary(inner.next().ok_or_else(|| CompileError::Parse("Unexpected end of expression".to_string()))?)?;

        while let (Some(op_pair), Some(right_pair)) = (inner.next(), inner.next()) {
            let op_str = op_pair.as_str();
            let right = self.parse_unary(right_pair)?;

            let op = match op_str {
                "*" => BinaryOperator::Mul,
                "/" => BinaryOperator::Div,
                _ => continue,
            };

            left = Expression::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_unary(&self, pair: pest::iterators::Pair<Rule>) -> CompileResult<Expression> {
        let text = pair.as_str().trim();
        if let Some(first_char) = text.chars().next() {
            if (first_char == '!' || first_char == '-') && !(first_char == '-' && text.len() > 1 && text.chars().nth(1).unwrap().is_ascii_digit()) {
                let op = match first_char {
                    '!' => UnaryOperator::Not,
                    '-' => UnaryOperator::Neg,
                    _ => unreachable!(),
                };
                let primary_pair = pair.into_inner().next().ok_or_else(|| CompileError::Parse("Unexpected end of expression".to_string()))?;
                let expr = self.parse_primary(primary_pair)?;
                return Ok(Expression::UnaryOp { op, expr: Box::new(expr) });
            }
        }
        let primary_pair = pair.into_inner().next().ok_or_else(|| CompileError::Parse("Unexpected end of expression".to_string()))?;
        self.parse_primary(primary_pair)
    }

    fn parse_primary(&self, pair: pest::iterators::Pair<Rule>) -> CompileResult<Expression> {
        let inner = match pair.as_rule() {
            Rule::primary => pair.into_inner().next().unwrap(),
            _ => pair,
        };
        match inner.as_rule() {
            Rule::number => Ok(Expression::Literal(inner.as_str().parse().unwrap_or(0.0))),
            Rule::identifier => Ok(Expression::Variable(inner.as_str().to_string())),
            Rule::function_call => self.parse_function_call(inner),
            Rule::expression | Rule::logical_expr => self.parse_expression(inner),
            Rule::unary => self.parse_unary(inner),
            _ => Err(CompileError::Semantic(format!("Unexpected primary: {:?}", inner.as_rule()))),
        }
    }

    fn parse_function_call(&self, pair: pest::iterators::Pair<Rule>) -> CompileResult<Expression> {
        let mut inner = pair.into_inner();
        let name = inner.next().ok_or_else(|| CompileError::Parse("Unexpected end of expression".to_string()))?.as_str().to_string();
        let mut args = Vec::new();

        for arg in inner {
            args.push(self.parse_expression(arg)?);
        }

        Ok(Expression::FunctionCall { name, args })
    }

    fn validate(&self, strategy: &Strategy) -> CompileResult<()> {
        // Check for forward references (indicator used before defined)
        let mut defined = std::collections::HashSet::new();
        for name in strategy.indicators.keys() {
            defined.insert(name.clone());
        }

        // Validate entry rules reference defined indicators
        self.validate_expression_refs(&strategy.entry_rules.long, &defined)?;
        self.validate_expression_refs(&strategy.entry_rules.short, &defined)?;

        // Validate exit rules
        self.validate_expression_refs(&strategy.exit_rules.long_exit, &defined)?;
        self.validate_expression_refs(&strategy.exit_rules.short_exit, &defined)?;

        Ok(())
    }

    fn validate_expression_refs(&self, expr: &Option<Expression>, defined: &std::collections::HashSet<String>) -> CompileResult<()> {
        if let Some(e) = expr {
            self.validate_expr_refs_recursive(e, defined)?;
        }
        Ok(())
    }

    fn validate_expr_refs_recursive(&self, expr: &Expression, defined: &std::collections::HashSet<String>) -> CompileResult<()> {
        match expr {
            Expression::Variable(name) => {
                // Allow special variables
                let special = ["open", "high", "low", "close", "volume", "vwap",
                              "avg_volume", "rsi", "ema", "sma", "bb_upper", "bb_middle", "bb_lower",
                              "macd", "macd_signal", "macd_hist", "atr", "vwap"];
                if !defined.contains(name) && !special.contains(&name.as_str()) {
                    return Err(CompileError::UnknownVariable(name.clone()));
                }
                Ok(())
            }
            Expression::BinaryOp { left, right, .. } => {
                self.validate_expr_refs_recursive(left, defined)?;
                self.validate_expr_refs_recursive(right, defined)
            }
            Expression::UnaryOp { expr, .. } => {
                self.validate_expr_refs_recursive(expr, defined)
            }
            Expression::FunctionCall { args, .. } => {
                for arg in args {
                    self.validate_expr_refs_recursive(arg, defined)?;
                }
                Ok(())
            }
            Expression::Literal(_) => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledStrategy {
    pub ast: Strategy,
}

impl CompiledStrategy {
    pub fn name(&self) -> &str {
        &self.ast.name
    }

    pub fn timeframe(&self) -> Option<&str> {
        self.ast.metadata.timeframe.as_deref()
    }

    pub fn indicators(&self) -> &HashMap<String, IndicatorDef> {
        &self.ast.indicators
    }

    pub fn entry_long(&self) -> Option<&Expression> {
        self.ast.entry_rules.long.as_ref()
    }

    pub fn entry_short(&self) -> Option<&Expression> {
        self.ast.entry_rules.short.as_ref()
    }

    pub fn exit_stop_loss(&self) -> Option<f64> {
        self.ast.exit_rules.stop_loss
    }

    pub fn exit_take_profit(&self) -> Option<f64> {
        self.ast.exit_rules.take_profit
    }

    pub fn risk_max_position_pct(&self) -> Option<f64> {
        self.ast.risk_rules.max_position_pct
    }
}

impl Default for StrategyCompiler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_simple_strategy() {
        let source = r#"
            strategy "TestStrategy" {
                universe: "SPY"
                timeframe: "5m"
            }
            indicators {
                rsi: RSI(14)
                bb: BB(20, 2.0)
            }
            entry {
                long: rsi < 30 && close < bb_lower
                short: rsi > 70 && close > bb_upper
            }
            exit {
                stop_loss: 2%
                take_profit: 4%
            }
            risk {
                max_position: 5%
            }
        "#;

        let compiler = StrategyCompiler::new();
        let result = compiler.compile(source);
        assert!(result.is_ok(), "Compile failed: {:?}", result.err());
    }

    #[test]
    fn test_compile_with_invalid_indicator() {
        let source = r#"
            strategy "Test" {}
            indicators {
                foo: INVALID_INDICATOR(10)
            }
        "#;

        let compiler = StrategyCompiler::new();
        let result = compiler.compile(source);
        assert!(result.is_err());
    }

    #[test]
    fn test_compile_unknown_variable() {
        let source = r#"
            strategy "Test" {}
            indicators {
                rsi: RSI(14)
            }
            entry {
                long: unknown_var > 50
            }
        "#;

        let compiler = StrategyCompiler::new();
        let result = compiler.compile(source);
        assert!(result.is_err());
    }

    #[test]
    fn test_compile_out_of_bounds_indicator_period() {
        let source = r#"
            strategy "Test" {}
            indicators {
                huge_rsi: RSI(100000)
            }
        "#;

        let compiler = StrategyCompiler::new();
        let result = compiler.compile(source);
        assert!(result.is_err(), "Must reject indicator periods exceeding safe upper bounds (5000)");
        if let Err(CompileError::Semantic(msg)) = result {
            assert!(msg.contains("0 < param <= 5000") || msg.contains("0 < period <= 5000"), "Error message must indicate bounds violation: {}", msg);
        } else {
            panic!("Expected semantic error for out of bounds period");
        }
    }

    #[test]
    fn test_compile_multi_param_indicator_bounds() {
        let source = r#"
            strategy "Test" {}
            indicators {
                bad_bb: BB(20, 10000)
            }
        "#;
        let compiler = StrategyCompiler::new();
        assert!(compiler.compile(source).is_err(), "Must reject out-of-bounds second parameters like BB std dev > 5000");

        let source_macd = r#"
            strategy "Test" {}
            indicators {
                bad_macd: MACD(12, 26, 0)
            }
        "#;
        assert!(compiler.compile(source_macd).is_err(), "Must reject zero parameters like MACD signal period = 0");
    }
}