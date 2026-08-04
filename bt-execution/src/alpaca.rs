use crate::broker::{BrokerAdapter, BrokerConfig, BrokerType, BrokerHealth, BrokerAccountInfo, AccountType, AccountStatus};
use async_trait::async_trait;
use bt_core::{ExecutionEvent, OrderAck, OrderId};
use bt_core::types::{Order, OrderStatus, OrderType, Side, Symbol, Position, Account, TimeInForce, Venue, AssetClass};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::time::Duration as TokioDuration;
use tracing::info;
use uuid::Uuid;

const ALPACA_PAPER_URL: &str = "https://paper-api.alpaca.markets";
const ALPACA_LIVE_URL: &str = "https://api.alpaca.markets";
const ALPACA_PAPER_WS: &str = "wss://paper-api.alpaca.markets/stream";
const ALPACA_LIVE_WS: &str = "wss://api.alpaca.markets/stream";

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct AlpacaOrder {
    id: String,
    client_order_id: String,
    created_at: String,
    updated_at: String,
    submitted_at: Option<String>,
    filled_at: Option<String>,
    expired_at: Option<String>,
    canceled_at: Option<String>,
    failed_at: Option<String>,
    replaced_at: Option<String>,
    replaced_by: Option<String>,
    replaces: Option<String>,
    asset_id: String,
    symbol: String,
    asset_class: String,
    qty: String,
    filled_qty: String,
    filled_avg_price: Option<String>,
    order_type: String,
    side: String,
    time_in_force: String,
    limit_price: Option<String>,
    stop_price: Option<String>,
    status: String,
    extended_hours: bool,
    legs: Option<Vec<AlpacaOrder>>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct AlpacaPosition {
    asset_id: String,
    symbol: String,
    exchange: String,
    asset_class: String,
    avg_entry_price: String,
    qty: String,
    side: String,
    market_value: String,
    cost_basis: String,
    unrealized_pl: String,
    unrealized_plpc: String,
    unrealized_intraday_pl: String,
    unrealized_intraday_plpc: String,
    current_price: String,
    lastday_price: String,
    change_today: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct AlpacaAccount {
    id: String,
    account_number: String,
    status: String,
    currency: String,
    equity: String,
    last_equity: String,
    multiplier: String,
    buying_power: String,
    initial_margin: String,
    maintenance_margin: String,
    daytrading_buying_power: String,
    long_market_value: String,
    short_market_value: String,
    cash: String,
    pattern_day_trader: bool,
    account_blocked: bool,
    trade_suspended_by_user: bool,
    equity_percentage: String,
    created_at: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct AlpacaTradeUpdate {
    event: String,
    order: AlpacaOrder,
    timestamp: String,
}

pub struct AlpacaAdapter {
    config: BrokerConfig,
    client: reqwest::Client,
    event_tx: broadcast::Sender<ExecutionEvent>,
    base_url: String,
    #[allow(dead_code)]
    ws_url: String,
    connected: bool,
    account_id: Option<String>,
    rate_limiter: Arc<governor::DefaultDirectRateLimiter>,
}

impl AlpacaAdapter {
    pub async fn new(config: BrokerConfig) -> anyhow::Result<Self> {
        let (event_tx, _) = broadcast::channel(1000);
        let client = reqwest::Client::builder()
            .timeout(TokioDuration::from_secs(30))
            .build()?;

        let (base_url, ws_url) = if config.paper_trading {
            (ALPACA_PAPER_URL.to_string(), ALPACA_PAPER_WS.to_string())
        } else {
            (ALPACA_LIVE_URL.to_string(), ALPACA_LIVE_WS.to_string())
        };

        let rate_limiter = Arc::new(governor::RateLimiter::direct(
            governor::Quota::per_second(std::num::NonZeroU32::new(10).unwrap())
                .allow_burst(std::num::NonZeroU32::new(20).unwrap())
        ));

        Ok(Self {
            config,
            client,
            event_tx,
            base_url,
            ws_url,
            connected: false,
            account_id: None,
            rate_limiter,
        })
    }

    fn auth_headers(&self) -> anyhow::Result<reqwest::header::HeaderMap> {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(key) = &self.config.credentials.api_key {
            headers.insert("APCA-API-KEY-ID", reqwest::header::HeaderValue::from_str(key).map_err(|e| anyhow::anyhow!("Invalid header value: {}", e))?);
        }
        if let Some(secret) = &self.config.credentials.api_secret {
            headers.insert("APCA-API-SECRET-KEY", reqwest::header::HeaderValue::from_str(secret).map_err(|e| anyhow::anyhow!("Invalid header value: {}", e))?);
        }
        Ok(headers)
    }

    fn map_order_type(&self, order_type: OrderType) -> &'static str {
        match order_type {
            OrderType::Market => "market",
            OrderType::Limit => "limit",
            OrderType::Stop => "stop",
            OrderType::StopLimit => "stop_limit",
            OrderType::TrailingStop => "trailing_stop",
            _ => "market",
        }
    }

    fn map_time_in_force(&self, tif: TimeInForce) -> &'static str {
        match tif {
            TimeInForce::Day => "day",
            TimeInForce::Gtc => "gtc",
            TimeInForce::Ioc => "ioc",
            TimeInForce::Fok => "fok",
            TimeInForce::Gtd => "day",
            TimeInForce::Atc => "day",
        }
    }

    fn map_side(&self, side: Side) -> &'static str {
        match side {
            Side::Buy => "buy",
            Side::Sell => "sell",
        }
    }

    fn parse_order_status(&self, status: &str) -> OrderStatus {
        match status {
            "new" => OrderStatus::New,
            "partially_filled" => OrderStatus::PartialFill,
            "filled" => OrderStatus::Filled,
            "canceled" | "cancelled" => OrderStatus::Cancelled,
            "rejected" => OrderStatus::Rejected,
            "expired" => OrderStatus::Expired,
            "pending_new" => OrderStatus::PendingNew,
            "accepted" => OrderStatus::Accepted,
            "pending_cancel" => OrderStatus::PendingCancel,
            _ => OrderStatus::New,
        }
    }

    fn convert_order(&self, alpaca: &AlpacaOrder) -> Order {
        let symbol = Symbol::new(Venue::Alpaca, &alpaca.symbol, AssetClass::Equity);
        let mut order = Order::new(
            symbol,
            match alpaca.side.as_str() { "buy" => Side::Buy, _ => Side::Sell },
            match alpaca.order_type.as_str() {
                "market" => OrderType::Market,
                "limit" => OrderType::Limit,
                "stop" => OrderType::Stop,
                "stop_limit" => OrderType::StopLimit,
                "trailing_stop" => OrderType::TrailingStop,
                _ => OrderType::Market,
            },
            alpaca.qty.parse().unwrap_or(Decimal::ZERO),
        );

        order.id = Uuid::parse_str(&alpaca.id).unwrap_or_else(|_| Uuid::new_v4());
        order.client_order_id = alpaca.client_order_id.clone();
        order.status = self.parse_order_status(&alpaca.status);
        order.filled_quantity = alpaca.filled_qty.parse().unwrap_or(Decimal::ZERO);
        order.avg_fill_price = alpaca.filled_avg_price.as_ref().and_then(|s| s.parse().ok());
        order.limit_price = alpaca.limit_price.as_ref().and_then(|s| s.parse().ok());
        order.stop_price = alpaca.stop_price.as_ref().and_then(|s| s.parse().ok());
        order.time_in_force = match alpaca.time_in_force.as_str() {
            "day" => TimeInForce::Day,
            "gtc" => TimeInForce::Gtc,
            "ioc" => TimeInForce::Ioc,
            "fok" => TimeInForce::Fok,
            _ => TimeInForce::Day,
        };
        order.created_at = DateTime::parse_from_rfc3339(&alpaca.created_at).map(|d| d.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now());
        order.updated_at = DateTime::parse_from_rfc3339(&alpaca.updated_at).map(|d| d.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now());

        order
    }

    async fn wait_for_rate_limit(&self) {
        self.rate_limiter.until_ready().await;
    }
}

#[async_trait]
impl BrokerAdapter for AlpacaAdapter {
    fn broker_type(&self) -> BrokerType {
        BrokerType::Alpaca
    }

    fn name(&self) -> &str {
        &self.config.name
    }

    fn is_paper(&self) -> bool {
        self.config.paper_trading
    }

    async fn connect(&mut self) -> anyhow::Result<()> {
        info!("Connecting to Alpaca ({})...", if self.config.paper_trading { "paper" } else { "live" });

        // Test connection by getting account
        let account = self.get_account().await?;
        self.account_id = Some(account.id.to_string());
        self.connected = true;

        info!("Connected to Alpaca: {}", account.id);
        Ok(())
    }

    async fn disconnect(&mut self) -> anyhow::Result<()> {
        self.connected = false;
        info!("Disconnected from Alpaca");
        Ok(())
    }

    async fn place_order(&self, order: Order) -> anyhow::Result<OrderId> {
        self.wait_for_rate_limit().await;

        let url = format!("{}/v2/orders", self.base_url);
        let mut body = serde_json::json!({
            "symbol": order.symbol.ticker,
            "qty": order.quantity.to_string(),
            "side": self.map_side(order.side),
            "type": self.map_order_type(order.order_type),
            "time_in_force": self.map_time_in_force(order.time_in_force),
            "client_order_id": order.client_order_id,
        });

        if let Some(price) = order.limit_price {
            body["limit_price"] = serde_json::json!(price.to_string());
        }
        if let Some(price) = order.stop_price {
            body["stop_price"] = serde_json::json!(price.to_string());
        }
        if let Some(price) = order.trail_amount {
            body["trail_price"] = serde_json::json!(price.to_string());
        }
        if let Some(pct) = order.trail_percent {
            body["trail_percent"] = serde_json::json!(pct.to_string());
        }
        if order.time_in_force == TimeInForce::Gtd {
            if let Some(date) = order.gtd_date {
                body["time_in_force"] = serde_json::json!("gtd");
                body["extended_hours"] = serde_json::json!(false);
                body["gtd_date"] = serde_json::json!(date.format("%Y-%m-%d").to_string());
            }
        }

        let response = self.client
            .post(&url)
            .headers(self.auth_headers()?)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Alpaca order failed: {}", error_text));
        }

        let alpaca_order: AlpacaOrder = response.json().await?;
        let order_id = Uuid::parse_str(&alpaca_order.id)?;

        // Emit ack event
        let ack = OrderAck {
            order_id,
            client_order_id: order.client_order_id.clone(),
            broker_order_id: alpaca_order.id.clone(),
            timestamp: Utc::now(),
        };
        let _ = self.event_tx.send(ExecutionEvent::OrderAcknowledged(ack));

        Ok(order_id)
    }

    async fn cancel_order(&self, order_id: OrderId) -> anyhow::Result<()> {
        self.wait_for_rate_limit().await;

        let url = format!("{}/v2/orders/{}", self.base_url, order_id);
        let response = self.client
            .delete(&url)
            .headers(self.auth_headers()?)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Alpaca cancel failed: {}", error_text));
        }

        let _ = self.event_tx.send(ExecutionEvent::OrderCancelled(order_id));
        Ok(())
    }

    async fn cancel_all_orders(&self) -> anyhow::Result<()> {
        self.wait_for_rate_limit().await;

        let url = format!("{}/v2/orders", self.base_url);
        let response = self.client
            .delete(&url)
            .headers(self.auth_headers()?)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Alpaca cancel all failed: {}", error_text));
        }

        Ok(())
    }

    async fn get_order(&self, order_id: OrderId) -> anyhow::Result<Option<Order>> {
        self.wait_for_rate_limit().await;

        let url = format!("{}/v2/orders/{}", self.base_url, order_id);
        let response = self.client
            .get(&url)
            .headers(self.auth_headers()?)
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !response.status().is_success() {
            return Ok(None);
        }

        let alpaca_order: AlpacaOrder = response.json().await?;
        Ok(Some(self.convert_order(&alpaca_order)))
    }

    async fn get_open_orders(&self) -> anyhow::Result<Vec<Order>> {
        self.wait_for_rate_limit().await;

        let url = format!("{}/v2/orders", self.base_url);
        let response = self.client
            .get(&url)
            .headers(self.auth_headers()?)
            .query(&[("status", "open"), ("limit", "500")])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await?;
            return Err(anyhow::anyhow!("HTTP {}: {}", status, text));
        }

        let orders: Vec<AlpacaOrder> = response.json().await?;
        Ok(orders.iter().map(|o| self.convert_order(o)).collect())
    }

    async fn get_order_history(&self, limit: usize) -> anyhow::Result<Vec<Order>> {
        self.wait_for_rate_limit().await;

        let url = format!("{}/v2/orders", self.base_url);
        let response = self.client
            .get(&url)
            .headers(self.auth_headers()?)
            .query(&[("status", "all"), ("limit", &limit.to_string())])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await?;
            return Err(anyhow::anyhow!("HTTP {}: {}", status, text));
        }

        let orders: Vec<AlpacaOrder> = response.json().await?;
        Ok(orders.iter().map(|o| self.convert_order(o)).collect())
    }

    async fn get_positions(&self) -> anyhow::Result<Vec<Position>> {
        self.wait_for_rate_limit().await;

        let url = format!("{}/v2/positions", self.base_url);
        let response = self.client
            .get(&url)
            .headers(self.auth_headers()?)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await?;
            return Err(anyhow::anyhow!("HTTP {}: {}", status, text));
        }

        let positions: Vec<AlpacaPosition> = response.json().await?;
        let mut result = Vec::new();

        for pos in positions {
            let symbol = Symbol::new(Venue::Alpaca, &pos.symbol, AssetClass::Equity);
            let qty = pos.qty.parse::<Decimal>().unwrap_or(Decimal::ZERO);
            let avg_price = pos.avg_entry_price.parse().unwrap_or(Decimal::ZERO);
            let current = pos.current_price.parse().unwrap_or(Decimal::ZERO);

            result.push(Position {
                symbol,
                quantity: if pos.side == "short" { -qty } else { qty },
                avg_entry_price: avg_price,
                current_price: Some(current),
                unrealized_pnl: Some(pos.unrealized_pl.parse().unwrap_or(Decimal::ZERO)),
                realized_pnl: Decimal::ZERO,
                market_value: Some(pos.market_value.parse().unwrap_or(Decimal::ZERO)),
                opened_at: Utc::now(),
                updated_at: Utc::now(),
            });
        }

        Ok(result)
    }

    async fn get_position(&self, symbol: &Symbol) -> anyhow::Result<Option<Position>> {
        self.wait_for_rate_limit().await;

        let url = format!("{}/v2/positions/{}", self.base_url, symbol.ticker);
        let response = self.client
            .get(&url)
            .headers(self.auth_headers()?)
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await?;
            return Err(anyhow::anyhow!("HTTP {}: {}", status, text));
        }

        let pos: AlpacaPosition = response.json().await?;
        let qty = pos.qty.parse::<Decimal>().unwrap_or(Decimal::ZERO);
        let avg_price = pos.avg_entry_price.parse().unwrap_or(Decimal::ZERO);
        let current = pos.current_price.parse().unwrap_or(Decimal::ZERO);

        Ok(Some(Position {
            symbol: symbol.clone(),
            quantity: if pos.side == "short" { -qty } else { qty },
            avg_entry_price: avg_price,
            current_price: Some(current),
            unrealized_pnl: Some(pos.unrealized_pl.parse().unwrap_or(Decimal::ZERO)),
            realized_pnl: Decimal::ZERO,
            market_value: Some(pos.market_value.parse().unwrap_or(Decimal::ZERO)),
            opened_at: Utc::now(),
            updated_at: Utc::now(),
        }))
    }

    async fn get_account(&self) -> anyhow::Result<Account> {
        self.wait_for_rate_limit().await;

        let url = format!("{}/v2/account", self.base_url);
        let response = self.client
            .get(&url)
            .headers(self.auth_headers()?)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed to get Alpaca account"));
        }

        let account: AlpacaAccount = response.json().await?;

        Ok(Account {
            id: Uuid::parse_str(&account.id).unwrap_or_else(|_| Uuid::new_v4()),
            equity: account.equity.parse().unwrap_or(Decimal::ZERO),
            cash: account.cash.parse().unwrap_or(Decimal::ZERO),
            buying_power: account.buying_power.parse().unwrap_or(Decimal::ZERO),
            initial_margin: account.initial_margin.parse().unwrap_or(Decimal::ZERO),
            maintenance_margin: account.maintenance_margin.parse().unwrap_or(Decimal::ZERO),
            day_trading_buying_power: account.daytrading_buying_power.parse().unwrap_or(Decimal::ZERO),
            long_market_value: account.long_market_value.parse().unwrap_or(Decimal::ZERO),
            short_market_value: account.short_market_value.parse().unwrap_or(Decimal::ZERO),
            currency: account.currency,
            updated_at: Utc::now(),
        })
    }

    async fn get_accounts(&self) -> anyhow::Result<Vec<BrokerAccountInfo>> {
        let account = self.get_account().await?;
        Ok(vec![BrokerAccountInfo {
            id: account.id.to_string(),
            name: "Primary".to_string(),
            account_type: AccountType::Margin,
            status: AccountStatus::Active,
            currency: account.currency,
        }])
    }

    fn events(&self) -> broadcast::Receiver<ExecutionEvent> {
        self.event_tx.subscribe()
    }

    async fn health_check(&self) -> anyhow::Result<BrokerHealth> {
        let start = std::time::Instant::now();
        let _ = self.get_account().await?;
        let latency = start.elapsed().as_millis() as u64;

        Ok(BrokerHealth {
            healthy: self.connected,
            latency_ms: latency,
            last_order_ms: None,
            error_rate: 0.0,
            connection_status: if self.connected { "connected" } else { "disconnected" }.to_string(),
        })
    }
}