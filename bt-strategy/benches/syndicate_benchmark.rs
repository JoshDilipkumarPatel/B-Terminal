use criterion::{black_box, criterion_group, criterion_main, Criterion};
use bt_strategy::syndicate::{SyndicateCouncil, MarketRegimeContext};
use bt_strategy::sentiment::SentimentScorer;
use tokio::runtime::Runtime;

fn bench_syndicate_convene_offline(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let council = SyndicateCouncil::new();

    c.bench_function("syndicate_convene_bull_trend_offline", |b| {
        b.to_async(&rt).iter(|| async {
            let decision = council.convene(black_box("NSE:TCS"), black_box(MarketRegimeContext::BullTrend), black_box(false), black_box(1000.0), black_box(0.0), black_box(0.0), black_box(None)).await;
            black_box(decision)
        });
    });

    c.bench_function("syndicate_convene_rangebound_offline", |b| {
        b.to_async(&rt).iter(|| async {
            let decision = council.convene(black_box("NSE:RELIANCE"), black_box(MarketRegimeContext::Rangebound), black_box(false), black_box(1000.0), black_box(0.0), black_box(0.0), black_box(None)).await;
            black_box(decision)
        });
    });

    c.bench_function("syndicate_convene_bear_trend_offline", |b| {
        b.to_async(&rt).iter(|| async {
            let decision = council.convene(black_box("NSE:INFY"), black_box(MarketRegimeContext::BearTrend), black_box(false), black_box(1000.0), black_box(0.0), black_box(0.0), black_box(None)).await;
            black_box(decision)
        });
    });

    c.bench_function("syndicate_convene_vol_shock_offline", |b| {
        b.to_async(&rt).iter(|| async {
            let decision = council.convene(black_box("NSE:HDFCBANK"), black_box(MarketRegimeContext::VolatilityShock), black_box(false), black_box(1000.0), black_box(0.0), black_box(0.0), black_box(None)).await;
            black_box(decision)
        });
    });

    c.bench_function("syndicate_convene_with_veto_offline", |b| {
        b.to_async(&rt).iter(|| async {
            let decision = council.convene(black_box("NSE:TCS"), black_box(MarketRegimeContext::VolatilityShock), black_box(true), black_box(1000.0), black_box(0.0), black_box(0.0), black_box(None)).await;
            black_box(decision)
        });
    });
}

fn bench_sentiment_scorer(c: &mut Criterion) {
    let bullish_text = "fii inflow and profit growth makes me bullish on nifty outperform surge";
    let bearish_text = "heavy fii outflow and market crash leads to loss default plunge";
    let neutral_text = "rbi updates repo rate for sensex stable guidance";

    c.bench_function("sentiment_scorer_bullish", |b| {
        b.iter(|| {
            let res = SentimentScorer::score(black_box(bullish_text));
            black_box(res)
        });
    });

    c.bench_function("sentiment_scorer_bearish", |b| {
        b.iter(|| {
            let res = SentimentScorer::score(black_box(bearish_text));
            black_box(res)
        });
    });

    c.bench_function("sentiment_scorer_neutral", |b| {
        b.iter(|| {
            let res = SentimentScorer::score(black_box(neutral_text));
            black_box(res)
        });
    });

    c.bench_function("sentiment_scorer_long_text", |b| {
        let long_text = bullish_text.repeat(100);
        b.iter(|| {
            let res = SentimentScorer::score(black_box(&long_text));
            black_box(res)
        });
    });
}

criterion_group!(benches, bench_syndicate_convene_offline, bench_sentiment_scorer);
criterion_main!(benches);