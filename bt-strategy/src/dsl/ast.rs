use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Strategy {
    pub name: String,
    pub metadata: StrategyMetadata,
    pub indicators: HashMap<String, IndicatorDef>,
    pub entry_rules: EntryRules,
    pub exit_rules: ExitRules,
    pub risk_rules: RiskRules,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrategyMetadata {
    pub universe: Option<String>,
    pub timeframe: Option<String>,
    pub session: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorDef {
    pub name: String,
    pub kind: IndicatorKind,
    pub params: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IndicatorKind {
    RSI,
    EMA,
    SMA,
    BollingerBands,
    MACD,
    ATR,
    VWAP,
    VolumeSMA,
    StdDev,
    Highest,
    Lowest,
    Cross,
    CrossOver,
    CrossUnder,
    Custom(String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntryRules {
    pub long: Option<Expression>,
    pub short: Option<Expression>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExitRules {
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
    pub trailing_stop: Option<f64>,
    pub long_exit: Option<Expression>,
    pub short_exit: Option<Expression>,
    pub time_exit_minutes: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RiskRules {
    pub max_position_pct: Option<f64>,
    pub max_daily_loss_pct: Option<f64>,
    pub max_drawdown_pct: Option<f64>,
    pub max_correlation: Option<f64>,
    pub position_sizing: Option<String>,
    pub max_leverage: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expression {
    Literal(f64),
    Variable(String),
    BinaryOp {
        left: Box<Expression>,
        op: BinaryOperator,
        right: Box<Expression>,
    },
    UnaryOp {
        op: UnaryOperator,
        expr: Box<Expression>,
    },
    FunctionCall {
        name: String,
        args: Vec<Expression>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOperator {
    Add,
    Sub,
    Mul,
    Div,
    Gt,
    Lt,
    Gte,
    Lte,
    Eq,
    Neq,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOperator {
    Not,
    Neg,
}

impl Expression {
    pub fn literal(value: f64) -> Self {
        Expression::Literal(value)
    }

    pub fn variable(name: impl Into<String>) -> Self {
        Expression::Variable(name.into())
    }

    pub fn gt(left: Expression, right: Expression) -> Self {
        Expression::BinaryOp {
            left: Box::new(left),
            op: BinaryOperator::Gt,
            right: Box::new(right),
        }
    }

    pub fn lt(left: Expression, right: Expression) -> Self {
        Expression::BinaryOp {
            left: Box::new(left),
            op: BinaryOperator::Lt,
            right: Box::new(right),
        }
    }

    pub fn gte(left: Expression, right: Expression) -> Self {
        Expression::BinaryOp {
            left: Box::new(left),
            op: BinaryOperator::Gte,
            right: Box::new(right),
        }
    }

    pub fn lte(left: Expression, right: Expression) -> Self {
        Expression::BinaryOp {
            left: Box::new(left),
            op: BinaryOperator::Lte,
            right: Box::new(right),
        }
    }

    pub fn eq(left: Expression, right: Expression) -> Self {
        Expression::BinaryOp {
            left: Box::new(left),
            op: BinaryOperator::Eq,
            right: Box::new(right),
        }
    }

    pub fn and(left: Expression, right: Expression) -> Self {
        Expression::BinaryOp {
            left: Box::new(left),
            op: BinaryOperator::And,
            right: Box::new(right),
        }
    }

    pub fn or(left: Expression, right: Expression) -> Self {
        Expression::BinaryOp {
            left: Box::new(left),
            op: BinaryOperator::Or,
            right: Box::new(right),
        }
    }

    pub fn not(expr: Expression) -> Self {
        Expression::UnaryOp {
            op: UnaryOperator::Not,
            expr: Box::new(expr),
        }
    }

    pub fn call(name: impl Into<String>, args: Vec<Expression>) -> Self {
        Expression::FunctionCall {
            name: name.into(),
            args,
        }
    }
}