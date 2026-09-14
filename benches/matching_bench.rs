use criterion::{black_box, criterion_group, criterion_main, Criterion};
use matching_engine::*;
use std::time::SystemTime;

fn ts() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

fn bench_insert_limit(c: &mut Criterion) {
    c.bench_function("insert_limit_order", |b| {
        let risk = RiskEngine::new(RiskConfig::default());
        let mut engine = MatchingEngine::new(risk);
        let mut id = 0u64;
        b.iter(|| {
            id += 1;
            let order = Order {
                id: OrderId(id),
                account: AccountId(1),
                side: Side::Bid,
                order_type: OrderType::Limit,
                price: Price::from_f64(100.0 + (id % 50) as f64),
                qty: Qty(10),
                remaining: Qty(10),
                tif: TimeInForce::GTC,
                timestamp: 0,
            };
            black_box(engine.submit_order(order).ok());
        });
    });
}

fn bench_match_full(c: &mut Criterion) {
    c.bench_function("match_full_fill", |b| {
        let risk = RiskEngine::new(RiskConfig::default());
        let mut engine = MatchingEngine::new(risk);
        for i in 0..100 {
            let order = Order {
                id: OrderId(i),
                account: AccountId(1),
                side: Side::Ask,
                order_type: OrderType::Limit,
                price: Price::from_f64(100.0),
                qty: Qty(10),
                remaining: Qty(10),
                tif: TimeInForce::GTC,
                timestamp: 0,
            };
            engine.submit_order(order).ok();
        }
        let mut taker_id = 100u64;
        b.iter(|| {
            taker_id += 1;
            let order = Order {
                id: OrderId(taker_id),
                account: AccountId(2),
                side: Side::Bid,
                order_type: OrderType::Limit,
                price: Price::from_f64(100.0),
                qty: Qty(10),
                remaining: Qty(10),
                tif: TimeInForce::GTC,
                timestamp: 0,
            };
            black_box(engine.submit_order(order).ok());
        });
    });
}

fn bench_cancel(c: &mut Criterion) {
    c.bench_function("cancel_order", |b| {
        let risk = RiskEngine::new(RiskConfig::default());
        let mut engine = MatchingEngine::new(risk);
        for i in 0..1000 {
            let order = Order {
                id: OrderId(i),
                account: AccountId(1),
                side: Side::Bid,
                order_type: OrderType::Limit,
                price: Price::from_f64(100.0 + (i % 100) as f64),
                qty: Qty(10),
                remaining: Qty(10),
                tif: TimeInForce::GTC,
                timestamp: 0,
            };
            engine.submit_order(order).ok();
        }
        let mut cancel_id = 0u64;
        b.iter(|| {
            black_box(engine.cancel_order(OrderId(cancel_id)));
            cancel_id += 1;
        });
    });
}

fn bench_wal_append(c: &mut Criterion) {
    c.bench_function("wal_mmap_append_single", |b| {
        let path = "bench_wal_append.wal";
        let mut writer = WalWriter::create(path).unwrap();
        let mut seq = 0u64;
        b.iter(|| {
            seq += 1;
            let event = WalEvent::new_insert(seq, ts(), seq, 1, 1, 10000, 10, 0, 0);
            black_box(writer.append(event).ok());
        });
        writer.flush().ok();
        drop(writer);
        std::fs::remove_file(path).ok();
    });
}

fn bench_wal_batch(c: &mut Criterion) {
    c.bench_function("wal_mmap_batch_pipeline", |b| {
        let path = "bench_wal_batch.wal";
        let risk = RiskEngine::new(RiskConfig::default());
        let mut engine = MatchingEngine::new(risk);
        let mut writer = WalWriter::create(path).unwrap();
        let mut seq = 0u64;

        b.iter(|| {
            let batch_size = 64;
            let mut events = Vec::with_capacity(batch_size);
            for _ in 0..batch_size {
                seq += 1;
                events.push(WalEvent::new_insert(seq, ts(), seq, 1, 1, 10000, 10, 0, 0));
            }
            // Group commit: single mmap flush for batch
            let start = writer.append_batch(&events).unwrap();
            writer.flush_range(start, batch_size * 80).ok();

            // Process through matching engine
            for _ in 0..batch_size {
                seq += 1;
                let order = Order {
                    id: OrderId(seq),
                    account: AccountId(1),
                    side: Side::Bid,
                    order_type: OrderType::Limit,
                    price: Price(10000),
                    qty: Qty(10),
                    remaining: Qty(10),
                    tif: TimeInForce::GTC,
                    timestamp: 0,
                };
                black_box(engine.submit_order(order).ok());
            }
        });

        writer.flush().ok();
        drop(writer);
        std::fs::remove_file(path).ok();
    });
}

fn bench_wal_full_pipeline(c: &mut Criterion) {
    c.bench_function("wal_full_pipeline", |b| {
        let path = "bench_pipeline.wal";
        let risk = RiskEngine::new(RiskConfig::default());
        let mut engine = MatchingEngine::new(risk);
        let mut writer = WalWriter::create(path).unwrap();
        let mut seq = 0u64;

        b.iter(|| {
            seq += 1;
            let event = WalEvent::new_insert(seq, ts(), seq, 1, 1, 10000, 10, 0, 0);
            writer.append(event).ok();

            let order = Order {
                id: OrderId(seq),
                account: AccountId(1),
                side: Side::Bid,
                order_type: OrderType::Limit,
                price: Price(10000),
                qty: Qty(10),
                remaining: Qty(10),
                tif: TimeInForce::GTC,
                timestamp: 0,
            };
            black_box(engine.submit_order(order).ok());
        });

        writer.flush().ok();
        drop(writer);
        std::fs::remove_file(path).ok();
    });
}

criterion_group!(
    benches,
    bench_insert_limit,
    bench_match_full,
    bench_cancel,
    bench_wal_append,
    bench_wal_batch,
    bench_wal_full_pipeline
);
criterion_main!(benches);
