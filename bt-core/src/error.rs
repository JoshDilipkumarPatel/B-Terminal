use thiserror::Error;

#[derive(Error, Debug)]
pub enum BtError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Data feed error: {0}")]
    DataFeed(String),

    #[error("Provider error ({provider}): {message}")]
    Provider { provider: String, message: String },

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),

    #[error("Symbol not found: {0}")]
    SymbolNotFound(String),

    #[error("Invalid symbol: {0}")]
    InvalidSymbol(String),

    #[error("Order error: {0}")]
    Order(String),

    #[error("Order rejected: {reason}")]
    OrderRejected { reason: String },

    #[error("Order not found: {0}")]
    OrderNotFound(String),

    #[error("Insufficient funds: required {required}, available {available}")]
    InsufficientFunds { required: String, available: String },

    #[error("Position error: {0}")]
    Position(String),

    #[error("Risk limit breached: {limit_type} - current: {current}, limit: {limit}")]
    RiskLimitBreached {
        limit_type: String,
        current: String,
        limit: String,
    },

    #[error("Kill switch activated: {reason}")]
    KillSwitch { reason: String },

    #[error("Strategy error: {0}")]
    Strategy(String),

    #[error("Strategy compilation error: {0}")]
    StrategyCompilation(String),

    #[error("Strategy validation error: {0}")]
    StrategyValidation(String),

    #[error("Backtest error: {0}")]
    Backtest(String),

    #[error("Execution error: {0}")]
    Execution(String),

    #[error("Broker error ({broker}): {message}")]
    Broker { broker: String, message: String },

    #[error("Broker connection failed: {0}")]
    BrokerConnection(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Math error: {0}")]
    Math(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),
}

impl BtError {
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    pub fn data_feed(msg: impl Into<String>) -> Self {
        Self::DataFeed(msg.into())
    }

    pub fn provider(provider: impl Into<String>, msg: impl Into<String>) -> Self {
        Self::Provider {
            provider: provider.into(),
            message: msg.into(),
        }
    }

    pub fn connection(msg: impl Into<String>) -> Self {
        Self::Connection(msg.into())
    }

    pub fn auth(msg: impl Into<String>) -> Self {
        Self::Auth(msg.into())
    }

    pub fn rate_limit(msg: impl Into<String>) -> Self {
        Self::RateLimit(msg.into())
    }

    pub fn symbol_not_found(symbol: impl Into<String>) -> Self {
        Self::SymbolNotFound(symbol.into())
    }

    pub fn invalid_symbol(msg: impl Into<String>) -> Self {
        Self::InvalidSymbol(msg.into())
    }

    pub fn order(msg: impl Into<String>) -> Self {
        Self::Order(msg.into())
    }

    pub fn order_rejected(reason: impl Into<String>) -> Self {
        Self::OrderRejected { reason: reason.into() }
    }

    pub fn order_not_found(id: impl Into<String>) -> Self {
        Self::OrderNotFound(id.into())
    }

    pub fn insufficient_funds(required: impl Into<String>, available: impl Into<String>) -> Self {
        Self::InsufficientFunds {
            required: required.into(),
            available: available.into(),
        }
    }

    pub fn position(msg: impl Into<String>) -> Self {
        Self::Position(msg.into())
    }

    pub fn risk_limit(limit_type: impl Into<String>, current: impl Into<String>, limit: impl Into<String>) -> Self {
        Self::RiskLimitBreached {
            limit_type: limit_type.into(),
            current: current.into(),
            limit: limit.into(),
        }
    }

    pub fn kill_switch(reason: impl Into<String>) -> Self {
        Self::KillSwitch { reason: reason.into() }
    }

    pub fn strategy(msg: impl Into<String>) -> Self {
        Self::Strategy(msg.into())
    }

    pub fn strategy_compilation(msg: impl Into<String>) -> Self {
        Self::StrategyCompilation(msg.into())
    }

    pub fn strategy_validation(msg: impl Into<String>) -> Self {
        Self::StrategyValidation(msg.into())
    }

    pub fn backtest(msg: impl Into<String>) -> Self {
        Self::Backtest(msg.into())
    }

    pub fn execution(msg: impl Into<String>) -> Self {
        Self::Execution(msg.into())
    }

    pub fn broker(broker: impl Into<String>, msg: impl Into<String>) -> Self {
        Self::Broker {
            broker: broker.into(),
            message: msg.into(),
        }
    }

    pub fn broker_connection(msg: impl Into<String>) -> Self {
        Self::BrokerConnection(msg.into())
    }

    pub fn parse(msg: impl Into<String>) -> Self {
        Self::Parse(msg.into())
    }

    pub fn math(msg: impl Into<String>) -> Self {
        Self::Math(msg.into())
    }

    pub fn timeout(msg: impl Into<String>) -> Self {
        Self::Timeout(msg.into())
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    pub fn not_implemented(feature: impl Into<String>) -> Self {
        Self::NotImplemented(feature.into())
    }
}

pub type Result<T> = std::result::Result<T, BtError>;

pub fn bail<T>(err: BtError) -> Result<T> {
    Err(err)
}

#[macro_export]
macro_rules! bt_bail {
    ($($arg:tt)*) => {
        return Err($crate::error::BtError::internal(format!($($arg)*)))
    };
}