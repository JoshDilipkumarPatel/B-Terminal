use criterion::{black_box, criterion_group, criterion_main, Criterion};
use bt_data::orderbook_aggregator::OrderBookAnalyzer;
use bt_core::events::{OrderBook, PriceLevel};
use bt_core::types::{Symbol, Side, Venue};
use rust_decimal::Decimal;
use chrono::Utc;

fn create_test_book() -> OrderBook {
    OrderBook {
        symbol: Symbol::parse("BTC/USD").unwrap(),
        bids: vec![
            PriceLevel { price: Decimal::new(10000, 0), size: Decimal::new(2, 0), order_count: None },
            PriceLevel { price: Decimal::new(9990, 0), size: Decimal::new(5, 0), order_count: None },
            PriceLevel { price: Decimal::new(9980, 0), size: Decimal::new(3, 0), order_count: None },
        ],
        asks: vec![
            PriceLevel { price: Decimal::new(10010, 0), size: Decimal::new(1, 0), order_count: None },
            PriceLevel { price: Decimal::new(10020, 0), size: Decimal::new(4, 0), order_count: None },
            PriceLevel { price: Decimal::new(10030, 0), size: Decimal::new(5, 0), order_count: None },
        ],
        timestamp: Utc::now(),
        venue: Venue::Binance,
    }
}

fn bench_verify_market_integrity(c: &mut Criterion) {
    let book = create_test_book();

    c.bench_function("verify_market_integrity_normal", |b| {
        b.iter(|| {
            let res = OrderBookAnalyzer::verify_market_integrity(black_box(&book), black_box(500));
            black_box(res)
        });
    });

    // Test with crossed market (should trigger HaltedOrMalformed)
    let mut crossed_book = book.clone();
    crossed_book.bids[0].price = Decimal::new(10025, 0);

    c.bench_function("verify_market_integrity_crossed", |b| {
        b.iter(|| {
            let res = OrderBookAnalyzer::verify_market_integrity(black_box(&crossed_book), black_box(500));
            black_box(res)
        });
    });

    // Test with empty side (halt)
    let mut empty_book = book.clone();
    empty_book.bids.clear();

    c.bench_function("verify_market_integrity_empty_bids", |b| {
        b.iter(|| {
            let res = OrderBookAnalyzer::verify_market_integrity(black_box(&empty_book), black_box(500));
            black_box(res)
        });
    });

    // Test with spread distortion
    let mut wide_spread_book = book.clone();
    wide_spread_book.asks[0].price = Decimal::new(10600, 0); // 6% spread

    c.bench_function("verify_market_integrity_wide_spread", |b| {
        b.iter(|| {
            let res = OrderBookAnalyzer::verify_market_integrity(black_box(&wide_spread_book), black_box(500));
            black_box(res)
        });
    });
}

fn bench_imbalance(c: &mut Criterion) {
    let book = create_test_book();

    c.bench_function("imbalance", |b| {
        b.iter(|| {
            let res = OrderBookAnalyzer::imbalance(black_box(&book));
            black_box(res)
        });
    });
}

fn bench_spread_and_mid(c: &mut Criterion) {
    let book = create_test_book();

    c.bench_function("spread", |b| {
        b.iter(|| {
            let res = OrderBookAnalyzer::spread(black_box(&book));
            black_box(res)
        });
    });

    c.bench_function("mid_price", |b| {
        b.iter(|| {
            let res = OrderBookAnalyzer::mid_price(black_box(&book));
            black_box(res)
        });
    });
}

fn bench_depth_within_bps(c: &mut Criterion) {
    let book = create_test_book();

    c.bench_function("depth_within_bps", |b| {
        b.iter(|| {
            let res = OrderBookAnalyzer::depth_within_bps(black_box(&book), black_box(20));
            black_box(res)
        });
    });
}

fn bench_fill_price(c: &mut Criterion) {
    let book = create_test_book();

    c.bench_function("estimated_fill_price_buy", |b| {
        b.iter(|| {
            let res = OrderBookAnalyzer::estimated_fill_price(black_box(&book), black_box(Decimal::new(3, 0)), black_box(Side::Buy));
            black_box(res)
        });
    });

    c.bench_function("estimated_fill_price_sell", |b| {
        b.iter(|| {
            let res = OrderBookAnalyzer::estimated_fill_price(black_box(&book), black_box(Decimal::new(3, 0)), black_box(Side::Sell));
            black_box(res)
        });
    });

    c.bench_function("estimated_fill_price_insufficient", |b| {
        b.iter(|| {
            let res = OrderBookAnalyzer::estimated_fill_price(black_box(&book), black_box(Decimal::new(20, 0)), black_box(Side::Buy));
            black_box(res)
        });
    });
}

fn bench_market_impact(c: &mut Criterion) {
    let book = create_test_book();

    c.bench_function("market_impact_pct", |b| {
        b.iter(|| {
            let res = OrderBookAnalyzer::market_impact_pct(black_box(&book), black_box(Decimal::new(3, 0)), black_box(Side::Buy));
            black_box(res)
        });
    });
}

criterion_group!(
    benches,
    bench_verify_market_integrity,
    bench_imbalance,
    bench_spread_and_mid,
    bench_depth_within_bps,
    bench_fill_price,
    bench_market_impact
);
criterion_main!(benches);