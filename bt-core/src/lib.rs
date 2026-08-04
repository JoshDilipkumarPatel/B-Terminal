//! bt-core - Core types, events, config, and utilities for Bloomberg Terminal recreation

pub mod alarms;
pub mod config;
pub mod error;
pub mod events;
pub mod kill_switch;
pub mod risk_limits;
pub mod types;

pub use config::Config;
pub use error::{BtError, Result};
pub use events::{create_event_bus, EventReceiver, EventSender, MarketEvent, SignalEvent, ExecutionEvent, RiskEvent, KillReason, OrderAck, OrderFill, OrderPartialFill, OrderReject};
pub use kill_switch::{GlobalKillSwitch, KillSwitchStatus, AutoKillSwitchMonitor};
pub use risk_limits::{RiskManager, RiskLimits, GlobalRiskLimits, StrategyRiskLimits, SymbolRiskLimits, RiskCheckResult, RiskMetrics};
pub use types::{
    Symbol, Venue, AssetClass, OptionType, Side, OrderType, TimeInForce,
    Order, OrderId, OrderStatus, Position, Account, Fill, Liquidity,
};
pub use alarms::{AlarmTier, AlarmEvent, AcousticAlarmShield};
