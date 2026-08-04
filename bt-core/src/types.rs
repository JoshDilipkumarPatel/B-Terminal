use anyhow::Result;
use chrono::{DateTime, Utc};
pub use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub type OrderId = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetClass {
    Equity,
    Option,
    Future,
    Crypto,
    Forex,
    FixedIncome,
    Commodity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Venue {
    // Equity
    Nyse,
    Nasdaq,
    Amex,
    Arca,
    Iex,
    Polygon,
    // Crypto
    Binance,
    BinanceUs,
    Coinbase,
    CoinbasePro,
    Kraken,
    Bybit,
    Okx,
    Kucoin,
    // Broker
    Alpaca,
    InteractiveBrokers,
    // Synthetic
    Simulator,
    // Indian Markets & Brokers
    Nse,
    Bse,
    Mcx,
    Groww,
    CoinDcx,
}

impl Venue {
    pub fn venue_name(&self) -> &'static str {
        match self {
            Venue::Nyse => "NYSE",
            Venue::Nasdaq => "NASDAQ",
            Venue::Amex => "AMEX",
            Venue::Arca => "ARCA",
            Venue::Iex => "IEX",
            Venue::Polygon => "POLYGON",
            Venue::Binance => "BINANCE",
            Venue::BinanceUs => "BINANCEUS",
            Venue::Coinbase => "COINBASE",
            Venue::CoinbasePro => "COINBASEPRO",
            Venue::Kraken => "KRAKEN",
            Venue::Bybit => "BYBIT",
            Venue::Okx => "OKX",
            Venue::Kucoin => "KUCOIN",
            Venue::Alpaca => "ALPACA",
            Venue::InteractiveBrokers => "IBKR",
            Venue::Simulator => "SIM",
            Venue::Nse => "NSE",
            Venue::Bse => "BSE",
            Venue::Mcx => "MCX",
            Venue::Groww => "GROWW",
            Venue::CoinDcx => "COINDCX",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Symbol {
    pub venue: Venue,
    pub ticker: String,
    pub asset_class: AssetClass,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry: Option<DateTime<Utc>>,     // For options/futures
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strike: Option<Decimal>,            // For options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option_type: Option<OptionType>,    // For options
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OptionType {
    Call,
    Put,
}

impl Symbol {
    pub fn parse(s: &str) -> anyhow::Result<Self> {
        Ok(Self::new(Venue::Nasdaq, s, AssetClass::Equity))
    }

    pub fn new(venue: Venue, ticker: impl Into<String>, asset_class: AssetClass) -> Self {
        Self {
            venue,
            ticker: ticker.into(),
            asset_class,
            expiry: None,
            strike: None,
            option_type: None,
        }
    }

    pub fn option(
        venue: Venue,
        ticker: impl Into<String>,
        expiry: DateTime<Utc>,
        strike: Decimal,
        option_type: OptionType,
    ) -> Self {
        Self {
            venue,
            ticker: ticker.into(),
            asset_class: AssetClass::Option,
            expiry: Some(expiry),
            strike: Some(strike),
            option_type: Some(option_type),
        }
    }

    pub fn future(venue: Venue, ticker: impl Into<String>, expiry: DateTime<Utc>) -> Self {
        Self {
            venue,
            ticker: ticker.into(),
            asset_class: AssetClass::Future,
            expiry: Some(expiry),
            strike: None,
            option_type: None,
        }
    }

    pub fn crypto(venue: Venue, ticker: impl Into<String>) -> Self {
        Self {
            venue,
            ticker: ticker.into(),
            asset_class: AssetClass::Crypto,
            expiry: None,
            strike: None,
            option_type: None,
        }
    }

    pub fn normalized(&self) -> String {
        match self.asset_class {
            AssetClass::Option => {
                format!(
                    "{}:{}:{}:{}:{}",
                    self.venue.venue_name(),
                    self.ticker,
                    self.expiry.map(|d| d.format("%Y%m%d").to_string()).unwrap_or_default(),
                    self.strike.map(|s| s.to_string()).unwrap_or_default(),
                    match self.option_type {
                        Some(OptionType::Call) => "C",
                        Some(OptionType::Put) => "P",
                        None => "",
                    }
                )
            }
            AssetClass::Future => {
                format!(
                    "{}:{}:{}",
                    self.venue.venue_name(),
                    self.ticker,
                    self.expiry.map(|d| d.format("%Y%m%d").to_string()).unwrap_or_default()
                )
            }
            _ => format!("{}:{}", self.venue.venue_name(), self.ticker),
        }
    }
}

impl std::fmt::Display for Symbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.normalized())
    }
}

impl std::str::FromStr for Symbol {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        // Format: VENUE:TICKER[:EXPIRY[:STRIKE[:TYPE]]]
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() < 2 {
            anyhow::bail!("Invalid symbol format: {}", s);
        }

        let venue = match parts[0].to_uppercase().as_str() {
            "NYSE" => Venue::Nyse,
            "NASDAQ" => Venue::Nasdaq,
            "AMEX" => Venue::Amex,
            "ARCA" => Venue::Arca,
            "IEX" => Venue::Iex,
            "POLYGON" => Venue::Polygon,
            "BINANCE" => Venue::Binance,
            "BINANCEUS" => Venue::BinanceUs,
            "COINBASE" => Venue::Coinbase,
            "COINBASEPRO" => Venue::CoinbasePro,
            "KRAKEN" => Venue::Kraken,
            "BYBIT" => Venue::Bybit,
            "OKX" => Venue::Okx,
            "KUCOIN" => Venue::Kucoin,
            "ALPACA" => Venue::Alpaca,
            "IB" | "IBKR" => Venue::InteractiveBrokers,
            "SIM" => Venue::Simulator,
            "NSE" => Venue::Nse,
            "BSE" => Venue::Bse,
            "MCX" => Venue::Mcx,
            "GROWW" => Venue::Groww,
            "COINDCX" => Venue::CoinDcx,
            _ => anyhow::bail!("Unknown venue: {}", parts[0]),
        };

        let ticker = parts[1].to_string();

        if parts.len() == 2 {
            return Ok(Symbol::new(venue, ticker, AssetClass::Equity));
        }

        let asset_class = if parts.len() >= 4 && !parts[3].is_empty() {
            AssetClass::Option
        } else if parts.len() >= 3 && !parts[2].is_empty() {
            AssetClass::Future
        } else {
            AssetClass::Equity
        };

        let expiry = if parts.len() >= 3 && !parts[2].is_empty() {
            Some(DateTime::parse_from_str(parts[2], "%Y%m%d")?.with_timezone(&Utc))
        } else {
            None
        };

        let strike = if parts.len() >= 4 && !parts[3].is_empty() {
            Some(parts[3].parse()?)
        } else {
            None
        };

        let option_type = if parts.len() >= 5 && !parts[4].is_empty() {
            match parts[4] {
                "C" | "CALL" => Some(OptionType::Call),
                "P" | "PUT" => Some(OptionType::Put),
                _ => anyhow::bail!("Invalid option type: {}", parts[4]),
            }
        } else {
            None
        };

        Ok(Self {
            venue,
            ticker,
            asset_class,
            expiry,
            strike,
            option_type,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    pub fn opposite(&self) -> Self {
        match self {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit,
    Stop,
    StopLimit,
    TrailingStop,
    TrailingStopLimit,
    Iceberg,
    Twap,
    Vwap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TimeInForce {
    Day,
    Gtc,        // Good Till Cancelled
    Ioc,        // Immediate Or Cancel
    Fok,        // Fill Or Kill
    Gtd,        // Good Till Date
    Atc,        // At The Close
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: Uuid,
    pub client_order_id: String,
    pub symbol: Symbol,
    pub side: Side,
    pub order_type: OrderType,
    pub quantity: Decimal,
    pub limit_price: Option<Decimal>,
    pub stop_price: Option<Decimal>,
    pub trail_amount: Option<Decimal>,
    pub trail_percent: Option<Decimal>,
    pub time_in_force: TimeInForce,
    pub gtd_date: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: OrderStatus,
    pub filled_quantity: Decimal,
    pub avg_fill_price: Option<Decimal>,
    pub tags: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderStatus {
    New,
    PendingNew,
    Accepted,
    PartialFill,
    Filled,
    PendingCancel,
    Cancelled,
    Rejected,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: Symbol,
    pub quantity: Decimal,
    pub avg_entry_price: Decimal,
    pub current_price: Option<Decimal>,
    pub unrealized_pnl: Option<Decimal>,
    pub realized_pnl: Decimal,
    pub market_value: Option<Decimal>,
    pub opened_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: Uuid,
    pub equity: Decimal,
    pub cash: Decimal,
    pub buying_power: Decimal,
    pub initial_margin: Decimal,
    pub maintenance_margin: Decimal,
    pub day_trading_buying_power: Decimal,
    pub long_market_value: Decimal,
    pub short_market_value: Decimal,
    pub currency: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    pub id: Uuid,
    pub order_id: Uuid,
    pub symbol: Symbol,
    pub side: Side,
    pub quantity: Decimal,
    pub price: Decimal,
    pub venue: Venue,
    pub timestamp: DateTime<Utc>,
    pub commission: Decimal,
    pub liquidity: Liquidity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Liquidity {
    Added,
    Removed,
    Routed,
}

impl Order {
    pub fn new(
        symbol: Symbol,
        side: Side,
        order_type: OrderType,
        quantity: Decimal,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            client_order_id: Uuid::now_v7().to_string(),
            symbol,
            side,
            order_type,
            quantity,
            limit_price: None,
            stop_price: None,
            trail_amount: None,
            trail_percent: None,
            time_in_force: TimeInForce::Day,
            gtd_date: None,
            created_at: now,
            updated_at: now,
            status: OrderStatus::New,
            filled_quantity: Decimal::ZERO,
            avg_fill_price: None,
            tags: HashMap::new(),
        }
    }

    pub fn with_limit(mut self, price: Decimal) -> Self {
        self.limit_price = Some(price);
        self
    }

    pub fn with_stop(mut self, price: Decimal) -> Self {
        self.stop_price = Some(price);
        self
    }

    pub fn with_trail_amount(mut self, amount: Decimal) -> Self {
        self.trail_amount = Some(amount);
        self
    }

    pub fn with_trail_percent(mut self, percent: Decimal) -> Self {
        self.trail_percent = Some(percent);
        self
    }

    pub fn with_tif(mut self, tif: TimeInForce) -> Self {
        self.time_in_force = tif;
        self
    }

    pub fn with_gtd(mut self, date: DateTime<Utc>) -> Self {
        self.time_in_force = TimeInForce::Gtd;
        self.gtd_date = Some(date);
        self
    }

    pub fn with_client_id(mut self, id: impl Into<String>) -> Self {
        self.client_order_id = id.into();
        self
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.status,
            OrderStatus::New
                | OrderStatus::PendingNew
                | OrderStatus::Accepted
                | OrderStatus::PartialFill
                | OrderStatus::PendingCancel
        )
    }

    pub fn remaining_quantity(&self) -> Decimal {
        self.quantity - self.filled_quantity
    }

    pub fn update_fill(&mut self, qty: Decimal, price: Decimal) {
        let total_cost = self.avg_fill_price.unwrap_or(Decimal::ZERO) * self.filled_quantity
            + price * qty;
        self.filled_quantity += qty;
        if !self.filled_quantity.is_zero() {
            self.avg_fill_price = Some(total_cost / self.filled_quantity);
        }
        self.updated_at = Utc::now();

        if self.filled_quantity >= self.quantity {
            self.status = OrderStatus::Filled;
        } else {
            self.status = OrderStatus::PartialFill;
        }
    }
}
