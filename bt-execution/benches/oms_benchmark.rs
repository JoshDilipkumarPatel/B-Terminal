use criterion::{black_box, criterion_group, criterion_main, Criterion};
use bt_execution::broker::BrokerConfig;
use bt_execution::simulator::SimulatorAdapter;
use bt_execution::oms::{OrderManagementSystem, OMSConfig};
use bt_core::types::{Symbol, Side, OrderType, Order, Venue, AssetClass};
use rust_decimal::Decimal;
use tokio::runtime::Runtime;

fn bench_oms_idempotency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    // Setup OMS with simulator. Persistence disabled so the benchmark measures the
    // in-memory rejection path (no SQLite I/O in the hot loop).
    let oms = OrderManagementSystem::new_with_components(
        OMSConfig {
            enable_idempotency_persistence: false,
            ..OMSConfig::default()
        },
        "Simulator".to_string(),
        None,
        None,
    );
    let sim = SimulatorAdapter::new(BrokerConfig::default());
    let sym = Symbol::new(Venue::Nse, "RELIANCE", AssetClass::Equity);
    
    rt.block_on(async {
        sim.update_price(sym.clone(), Decimal::new(2500, 0)).await;
        let _ = oms.add_broker(sim).await;
    });

    let mut order = Order::new(
        sym,
        Side::Buy,
        OrderType::Market,
        Decimal::new(10, 0)
    );
    order.client_order_id = "BENCH-IDEMPOTENCY-001".to_string();

    c.bench_function("oms_idempotency_duplicate_rejection", |b| {
        b.to_async(&rt).iter(|| async {
            // Because it's idempotent, the first call (or previous iterations) cached it, 
            // so this will instantly hit the 300s cache and return an error (Idempotent Rejection).
            // We are benchmarking how fast this rejection path takes (must be sub-millisecond).
            let res = oms.submit_order(black_box(order.clone()), Some("Simulator".to_string())).await;
            black_box(res)
        });
    });
}

criterion_group!(benches, bench_oms_idempotency);
criterion_main!(benches);
