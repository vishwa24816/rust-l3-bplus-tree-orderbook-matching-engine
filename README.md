# Ultra-Low-Latency Execution Engine Subsystem — EVM/SVM Hybrid Architecture

A high-performance, local execution engine for EVM and SVM bytecode. Written in Rust with zero network dependencies. Executes transactions deterministically, journals state changes, and persists to SQLite.

## Architecture

Workspace monorepo with 4 crates, each with a single responsibility:

```
                    ┌─────────────┐
                    │  main.rs    │  CLI entry point
                    └──────┬──────┘
                           │
              ┌────────────┼────────────┐
              │            │            │
        ┌─────┴─────┐ ┌───┴───┐ ┌─────┴─────┐
        │  decoder   │ │  vm   │ │  storage  │
        │  router    │ │       │ │           │
        └─────┬──────┘ └───┬───┘ └─────┬─────┘
              │            │            │
              │     ┌──────┴──────┐     │
              │     │   journal   │     │
              │     └─────────────┘     │
              └─────────┬───────────────┘
                    StateJournal
```

| Layer | Crate | Purpose |
|-------|-------|---------|
| CLI | `vm/src/main.rs` | Parses hex input, file, or stdin. Routes to EVM or SVM. |
| Routing | `decoder/src/router.rs` | Detects engine via 4-byte ELF magic. |
| Execution | `vm/src/interpreter.rs`, `vm/src/svm.rs` | Stack-based EVM (~40 opcodes) and register-based SVM (~13 opcodes). |
| State | `journal/src/lib.rs` | `StateJournal` — holds pending mutations. |
| Persistence | `storage/src/sqlite_store.rs` | SQLite batch write via `rusqlite`. |

## Crates

| Crate | Type | Dependencies |
|-------|------|-------------|
| `decoder` | lib | `secp256k1`, `sha2`, `thiserror` |
| `vm` | lib + bin | `primitive-types` (U256), `sha2`, `hex`, `journal`, `decoder` |
| `journal` | lib | *(none — std only)* |
| `storage` | lib | `rusqlite` (bundled), `thiserror` |

## Quick Start

```bash
# Build
cargo build --release

# Execute EVM bytecode
echo -n "600560030100" | target/release/execution-engine -e hex

# Execute SVM bytecode
echo -n "010000000500010000000300ff" | target/release/execution-engine svm:hex

# Execute from file
target/release/execution-engine path/to/bytecode.bin
```

## Supported Opcodes

### EVM (~40 opcodes)

Stack arithmetic: `ADD`, `SUB`, `MUL`, `DIV`, `MOD`, `EXP`, `SIGNEXTEND`
Comparison: `LT`, `GT`, `SLT`, `SGT`, `EQ`, `ISZERO`, `AND`, `OR`, `XOR`, `NOT`, `BYTE`, `SHL`, `SHR`, `SAR`
Stack: `PUSH1`–`PUSH32`, `POP`, `DUP1`–`DUP16`, `SWAP1`–`SWAP16`
Memory: `MLOAD`, `MSTORE`, `MSTORE8`, `MSIZE`
Storage: `SLOAD`, `SSTORE`
Flow: `JUMP`, `JUMPI`, `JUMPDEST`, `STOP`, `RETURN`, `REVERT`, `INVALID`
Crypto: `SHA3`
Gas: `GAS`, `GASPRICE`

### SVM (13 opcodes)

`HALT`, `ADD`, `SUB`, `MUL`, `DIV`, `OR`, `NOT`, `SHL`, `SHR`
`STORE`, `LOAD`, `WRITE_ACCOUNT`, `MOV_IMM`

## Testing

```bash
cargo test                    # All 25 tests
cargo test --package vm       # VM tests (11)
cargo test --package decoder  # Decoder tests (7)
cargo test --package journal  # Journal tests (6)
cargo test --package storage  # Storage tests (1)
cargo bench                   # Criterion benchmarks
```

## Benchmarks

| Benchmark | Operation |
|-----------|-----------|
| `evm_add` | PUSH + PUSH + ADD + STOP |
| `evm_sstore` | PUSH + PUSH + SSTORE + STOP |
| `svm_register_add` | MOV_IMM + MOV_IMM + ADD + HALT |
| `svm_account_write` | MOV_IMM + MOV_IMM + WRITE_ACCOUNT + HALT |

**Performance:** EVM ~1.65 us/op, SVM ~192 ns/op (9x faster).

## Project Structure

```
├── Cargo.toml                # Workspace root
├── Cargo.lock
├── crates/
│   ├── decoder/              # Transaction parsing + engine routing
│   │   └── src/
│   │       ├── lib.rs        # Re-exports
│   │       ├── types.rs      # TxHeader (62 bytes), decode_transaction()
│   │       ├── verify.rs     # secp256k1 ECDSA signature verification
│   │       └── router.rs     # EngineRouter, ExecutionEngine enum
│   ├── vm/                   # Dual interpreter + CLI
│   │   ├── benches/engines.rs
│   │   └── src/
│   │       ├── lib.rs        # Re-exports + 11 unit tests
│   │       ├── main.rs       # CLI binary
│   │       ├── interpreter.rs # EVM execute() — ~40 opcodes
│   │       ├── svm.rs        # SVM svm_execute() — 13 opcodes
│   │       ├── opcodes.rs    # gas_cost() table
│   │       └── state.rs      # ExecutionState, ExecutionError
│   ├── journal/              # Transactional state journal
│   │   └── src/lib.rs        # StateJournal + 6 tests
│   └── storage/              # SQLite persistence
│       └── src/
│           ├── lib.rs
│           └── sqlite_store.rs # SqliteStore + 1 test
```

## Design Principles

- **No traits or dynamic dispatch** — all dispatch via `match` on enums/opcodes.
- **No async** — synchronous, blocking execution.
- **No serialization framework** — manual byte parsing with `from_le_bytes()`/`to_le_bytes()`.
- **Minimal dependencies** — journal crate has zero external deps.
- **StateJournal is the only shared mutable state** between VM and storage.

## License

MIT
