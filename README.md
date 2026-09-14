# Matching Engine

High-throughput, deterministic order matching engine with pre-trade risk checks, WAL persistence, and crash recovery.

## Architecture

```
Order → RiskEngine → MatchingEngine → OrderBook → WAL → Disk
                       ↓
                   SPSC Ring Buffer
                       ↓
                   Recovery (replay)
```

### Core Components

| Module | Purpose |
|---|---|
| `types` | `Symbol`, `OrderId`, `Price`, `Qty`, `Side`, `OrderType`, `TimeInForce`, `Trade` |
| `book` | `OrderBook` — `BTreeMap<Price, PriceLevel>` with price-time priority, STP logic |
| `risk` | `RiskEngine` — 5 pre-trade checks: notional, position, fat-finger, margin, short-sale |
| `engine` | `MatchingEngine` — orchestrates book + risk + STP + DAY TIF expiry |
| `registry` | `MatchingRegistry` — multi-asset routing via `HashMap<Symbol, MatchingEngine>` |
| `wal` | `WalWriter`/`WalReader` — memory-mapped WAL with 80-byte events and CRC32 integrity |
| `pipeline` | `SpscRing<T>` — lock-free SPSC ring buffer with cache-line padded atomics |
| `recovery` | `Recovery::replay_wal()` — deterministic state rebuild from WAL on crash |

## Order Types

| Type | Behavior |
|---|---|
| `Limit` | Rests in book at limit price, matches against opposite side |
| `Market` | Takes best available liquidity, no price limit |
| `IOC` | Immediate-or-Cancel — fills what it can, remainder discarded |
| `FOK` | Fill-or-Kill — entire order must fill or reject |
| `PostOnly` | Rejects if would cross existing liquidity |

## Risk Checks

| Check | Description |
|---|---|
| Notional | Rejects if `price × qty` exceeds `max_notional` |
| Position | Rejects if resulting position exceeds `max_position` |
| Fat-finger | Rejects if price deviates >5% from midpoint |
| Margin | Rejects if margin utilization exceeds 80% |
| Short-sale | Rejects sell orders when position ≤ 0 and short-selling disabled |

## Self-Trade Prevention

Three policies available via `MatchingEngine::with_stp()`:

- `CancelPassive` — cancels the resting order
- `CancelAggressive` — cancels the incoming order
- `DecrementAndCancel` — fills half, cancels both

## Persistence

Memory-mapped WAL with zero-copy writes:

```rust
let mut writer = WalWriter::create("exchange.wal")?;
writer.append(WalEvent::new_insert(seq, ts, order_id, account, symbol, price, qty, side, order_type))?;
writer.flush()?;
```

80-byte `repr(C)` events with CRC32 integrity. Auto-extends file (2x growth, 256MB cap).

Batch writes for group-commit:

```rust
writer.append_batch(&events)?;       // write N events
writer.flush_range(offset, len)?;    // single flush for batch
```

## Recovery

Replay WAL to rebuild engine state after crash:

```rust
let engine = Recovery::replay_wal("exchange.wal", RiskEngine::new(config))?;
```

Handles corrupt events gracefully (skips, logs, continues).

## Usage

```rust
use matching_engine::*;

// Single asset
let risk = RiskEngine::new(RiskConfig::default());
let mut engine = MatchingEngine::new(risk);

let order = Order {
    id: OrderId(1),
    account: AccountId(1),
    side: Side::Bid,
    order_type: OrderType::Limit,
    price: Price::from_f64(100.0),
    qty: Qty(10),
    remaining: Qty(10),
    tif: TimeInForce::GTC,
    timestamp: 0,
};

match engine.submit_order(order) {
    Ok(trades) => println!("Matched {} trades", trades.len()),
    Err(reason) => println!("Rejected: {:?}", reason),
}

// Multi-asset
let mut registry = MatchingRegistry::new();
registry.register(Symbol(1), MatchingEngine::new(RiskEngine::new(RiskConfig::default())));
registry.submit_order(Symbol(1), order)?;
```

## Performance

Benchmarks on single core (release profile):

| Operation | Latency |
|---|---|
| Insert limit order | ~263ns |
| Match full fill | ~274ns |
| Cancel order | ~13ns |
| WAL mmap single write | ~810ns |
| Full pipeline (WAL + match) | ~1.04μs |

**~1 million orders/second** single-core throughput.

## Benchmarking

```bash
cargo bench --bench matching_bench
```

Benchmarks: `insert_limit_order`, `match_full_fill`, `cancel_order`, `wal_mmap_append_single`, `wal_mmap_batch_pipeline`, `wal_full_pipeline`.

## Testing

```bash
cargo test
```

28 tests covering: match, partial fill, cancel, priority, risk rejection, STP (3 policies), L1/L2/L3 book access, multi-asset registry, DAY TIF expiry, WAL roundtrip, WAL batch, WAL extend, SPSC ring buffer, concurrent SPSC, WAL replay (insert+cancel), corrupt event handling, event counting.

## Project Structure

```
src/
├── main.rs          # Entry point
├── lib.rs           # Library crate
├── types.rs         # Domain types
├── book.rs          # Order book (BTreeMap)
├── risk.rs          # Pre-trade risk engine
├── engine.rs        # Matching engine
├── registry.rs      # Multi-asset registry
├── wal.rs           # Memory-mapped WAL
├── pipeline.rs      # SPSC ring buffer
├── recovery.rs      # WAL replay
└── tests.rs         # Integration tests
benches/
└── matching_bench.rs # Criterion benchmarks
```

## Dependencies

- `memmap2` — memory-mapped file I/O
- `criterion` (dev) — benchmarking

## License

MIT
