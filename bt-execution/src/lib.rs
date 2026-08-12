//! bt-execution - Order execution and broker adapters

pub mod broker;
pub mod alpaca;
pub mod simulator;
pub mod oms;
pub mod algo_orders;
pub mod groww;
pub mod coindcx;
pub mod zerodha;
pub mod upstox;
pub mod angel_one;
pub mod idempotency_store;

pub use broker::{BrokerAdapter, BrokerConfig, BrokerType, BrokerCredentials, BrokerEndpoints, BrokerHealth, BrokerAccountInfo, AccountType, AccountStatus, RateLimitConfig};
pub use alpaca::AlpacaAdapter;
pub use simulator::{SimulatorAdapter, SimulatorConfig};
pub use oms::{OrderManagementSystem, OMSConfig, OrderBuilder, OrderTracking};
pub use algo_orders::{TwapExecutor, VwapExecutor, IcebergExecutor, PreTradeCheck};
pub use groww::GrowwAdapter;
pub use coindcx::CoinDcxAdapter;
pub use zerodha::ZerodhaAdapter;
pub use upstox::UpstoxAdapter;
pub use angel_one::{AngelOneAdapter, AngelOneConfig};