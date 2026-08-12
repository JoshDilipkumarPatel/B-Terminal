use criterion::{criterion_group, criterion_main, Criterion, black_box};
use bt_data::vpin::{VpinEstimator, TradeTick};

fn bench_vpin_on_trade(c: &mut Criterion) {
    let mut estimator = VpinEstimator::new(1000.0, 50);

    let trade = TradeTick {
        price: 65000.0,
        volume: 10.0,
        is_buyer_maker: false,
    };

    c.bench_function("vpin_on_trade", |b| {
        b.iter(|| {
            estimator.update(black_box(&trade));
        })
    });
}

criterion_group!(benches, bench_vpin_on_trade);
criterion_main!(benches);
