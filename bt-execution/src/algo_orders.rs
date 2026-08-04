use std::sync::Arc;
use tokio::sync::RwLock;
use bt_core::types::*;
use crate::broker::BrokerAdapter;
use anyhow::Result;
use rust_decimal::Decimal;


#[derive(Debug, thiserror::Error)]
#[error("Insufficient funds: available {available}, required {required}, deficit {deficit}")]
pub struct InsufficientFundsError {
    pub available: Decimal,
    pub required: Decimal,
    pub deficit: Decimal,
}

pub struct PreTradeCheck;
impl PreTradeCheck {
    pub async fn verify_balance(
        broker: &dyn BrokerAdapter,
        order: &Order,
        estimated_price: Decimal,
    ) -> Result<Decimal> {
        let account = broker.get_account().await?;
        let required = order.quantity * estimated_price * Decimal::new(1005, 3); // 1.005
        let available = account.buying_power;
        
        if available < required {
            return Err(InsufficientFundsError {
                available,
                required,
                deficit: required - available,
            }.into());
        }
        
        Ok(available)
    }
}

pub struct TwapExecutor {
    parent_order: Order,
    duration_seconds: u64,
    num_slices: usize,
    broker: Arc<dyn BrokerAdapter>,
    #[allow(dead_code)]
    child_fills: Arc<RwLock<Vec<(Decimal, Decimal)>>>, // (qty, price)
}

impl TwapExecutor {
    pub fn new(parent_order: Order, broker: Arc<dyn BrokerAdapter>, duration_seconds: u64, num_slices: usize) -> Self {
        Self {
            parent_order,
            duration_seconds,
            num_slices,
            broker,
            child_fills: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    pub async fn execute(&self) -> Result<Vec<OrderId>> {
        use tokio::time::{sleep, Duration};
        use rand::Rng;
        
        let mut child_order_ids = Vec::new();
        let total_qty = self.parent_order.quantity;
        let slice_qty = total_qty / Decimal::new(self.num_slices as i64, 0);
        let interval = self.duration_seconds / self.num_slices as u64;
        
        for _ in 0..self.num_slices {
            let jitter = rand::thread_rng().gen_range(0.85..=1.15);
            let sleep_time = (interval as f64 * jitter) as u64;
            sleep(Duration::from_secs(sleep_time)).await;
            
            let child_order = Order::new(
                self.parent_order.symbol.clone(),
                self.parent_order.side,
                OrderType::Market,
                slice_qty,
            );
            
            let order_id = self.broker.place_order(child_order).await?;
            child_order_ids.push(order_id);
        }
        
        Ok(child_order_ids)
    }
}

pub struct VwapExecutor {
    parent_order: Order,
    broker: Arc<dyn BrokerAdapter>,
}

const NSE_VOLUME_PROFILE: [(f64, f64); 12] = [
    (9.25, 0.15), (9.75, 0.12), (10.25, 0.08), (10.75, 0.06),
    (11.25, 0.05), (11.75, 0.05), (12.25, 0.04), (12.75, 0.05),
    (13.25, 0.06), (13.75, 0.08), (14.25, 0.12), (14.75, 0.14),
];

impl VwapExecutor {
    pub fn new(parent_order: Order, broker: Arc<dyn BrokerAdapter>) -> Self {
        Self { parent_order, broker }
    }
    
    pub async fn execute(&self) -> Result<Vec<OrderId>> {
        use tokio::time::{sleep, Duration};
        let mut child_order_ids = Vec::new();
        let total_qty = self.parent_order.quantity;
        
        for (_, weight) in NSE_VOLUME_PROFILE.iter() {
            let slice_qty = total_qty * Decimal::from_f64_retain(*weight).unwrap_or(Decimal::ZERO);
            if slice_qty.is_zero() { continue; }
            
            let child_order = Order::new(
                self.parent_order.symbol.clone(),
                self.parent_order.side,
                OrderType::Market,
                slice_qty,
            );
            
            let order_id = self.broker.place_order(child_order).await?;
            child_order_ids.push(order_id);
            sleep(Duration::from_secs(1800)).await; // Wait 30 mins approx
        }
        
        Ok(child_order_ids)
    }
}

pub struct IcebergExecutor {
    parent_order: Order,
    visible_qty: Decimal,
    broker: Arc<dyn BrokerAdapter>,
}

impl IcebergExecutor {
    pub fn new(parent_order: Order, visible_qty: Decimal, broker: Arc<dyn BrokerAdapter>) -> Self {
        Self { parent_order, visible_qty, broker }
    }
    
    pub async fn execute(&self) -> Result<Vec<OrderId>> {
        use tokio::time::{sleep, Duration};
        let mut child_order_ids = Vec::new();
        let mut remaining_qty = self.parent_order.quantity;
        
        while remaining_qty > Decimal::ZERO {
            let slice_qty = if remaining_qty > self.visible_qty { self.visible_qty } else { remaining_qty };
            
            let mut child_order = Order::new(
                self.parent_order.symbol.clone(),
                self.parent_order.side,
                OrderType::Limit,
                slice_qty,
            );
            if let Some(price) = self.parent_order.limit_price {
                child_order = child_order.with_limit(price);
            }
            
            let order_id = self.broker.place_order(child_order).await?;
            child_order_ids.push(order_id);
            
            // In a real system, we'd wait for fill event. Here we just simulate a short sleep.
            sleep(Duration::from_secs(1)).await;
            
            remaining_qty -= slice_qty;
        }
        
        Ok(child_order_ids)
    }
}
