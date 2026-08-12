use criterion::{black_box, criterion_group, criterion_main, Criterion};
use bt_core::risk_limits::{GlobalRiskLimits, RiskLimits, RiskManager};
use bt_core::types::{Order, OrderStatus, OrderType, Side, Symbol, Venue, AssetClass};
use rust_decimal::Decimal;

fn benchmark_validate_order(c: &mut Criterion) {
    let limits = RiskLimits {
        global: GlobalRiskLimits {
            max_daily_loss_pct: Decimal::new(2, 2),
            max_drawdown_pct: Decimal::new(5, 2),
            max_leverage: Decimal::new(2, 0),
            max_open_orders: 100,
            max_order_size_usd: Decimal::new(10000, 0),
            max_portfolio_heat_pct: Decimal::new(20, 2),
            max_sector_exposure_pct: Decimal::new(15, 2),
            correlation_recompute_interval: 100,
        },
        ..Default::default()
    };
    
    let rm = RiskManager::new(limits);
    let order = Order {
        id: uuid::Uuid::new_v4(),
        client_order_id: "test".to_string(),
        symbol: Symbol::new(Venue::Nse, "RELIANCE", AssetClass::Equity),
        side: Side::Buy,
        order_type: OrderType::Limit,
        quantity: Decimal::new(100, 0),
        limit_price: Some(Decimal::new(150, 0)),
        stop_price: None,
        trail_amount: None,
        trail_percent: None,
        status: OrderStatus::New,
        filled_quantity: Decimal::ZERO,
        avg_fill_price: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        tags: std::collections::HashMap::new(),
        time_in_force: bt_core::types::TimeInForce::Gtc,
        gtd_date: None,
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("risk_manager_validate_order", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = rm.validate_order(black_box(&order)).await;
        })
    });
}

criterion_group!(benches, benchmark_validate_order);
criterion_main!(benches);