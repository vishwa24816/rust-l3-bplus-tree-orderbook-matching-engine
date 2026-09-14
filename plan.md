
Upgrade the Write-Ahead Log (`src/wal.rs`) and ring-buffer processing pipeline (`src/pipeline.rs`) to use zero-copy memory-mapped file persistence (`memmap2`) paired with dynamic group-commit batch flushing. This must eliminate `File::write_all` syscalls and reduce end-to-end WAL pipeline latency from ~5.7μs to sub-microsecond bounds (<500ns amortized).

Implement these optimizations using idiomatic, zero-allocation Rust:

### 1. Zero-Copy Memory-Mapped WAL (`src/wal.rs`)
- **Pre-Allocated File Mapping:** Refactor `WalWriter` to pre-allocate WAL files in fixed chunks (e.g., 64MB/256MB) using `File::set_len` and map them directly into process virtual memory using `memmap2::MmapMut`.
- **Zero-Copy Byte Slice Casting:** Replace serializing/copying event structs with direct pointer casting or `bytemuck` zero-copy slices (`#[repr(C, packed)]`). Writing a WAL event must perform a direct pointer copy (`unsafe { std::ptr::copy_nonoverlapping }`) into the active memory-mapped slice offset.
- **Ring-Boundary Pre-allocation:** Auto-extend and mmap new log segment files seamlessly when the current chunk offset reaches capacity.

### 2. Group-Commit & Micro-Batching Engine (`src/pipeline.rs`)
- **Adaptive Batching Loop:** Update the journaling consumer loop in `src/pipeline.rs` to process available events from the SPSC ring buffer in adaptive micro-batches (e.g., up to 64 or 128 events per loop iteration).
- **Amortized Flushing Strategy:**
  - Write all pending events in the current batch directly into the `MmapMut` slice in continuous memory order.
  - Issue a single asynchronous/range flush operation (`mmap_mut.flush_range(offset, length)` or `msync(MS_ASYNC)`) once per batch rather than once per event.
  - Advance the `persisted_sequence_id` atomically only after the entire batch slice is committed.

### 3. Benchmarking & Verification (`benches/matching_bench.rs`, `tests/`)
- Update `benches/matching_bench.rs` to benchmark:
  1. `wal_mmap_append_single`: Latency of a single zero-copy memory-mapped write (~20–50ns target).
  2. `wal_mmap_batch_pipeline`: End-to-end latency of a 64-event group commit (<200ns per-event amortized target).
- Add integration tests verifying that `WalReader` cleanly reads and validates CRC32 checksums from memory-mapped log files produced by `MmapMut`.
- Ensure deterministic replay (`src/recovery.rs`) remains 100% compliant with the mmap binary layout.

### Execution Instructions
1. Add `memmap2 = "0.9"` (or latest version) and `bytemuck = "1.14"` to `Cargo.toml`.
2. Apply modifications to `src/wal.rs`, `src/pipeline.rs`, and `src/recovery.rs`.
3. Validate compilation, safety, and benchmarks by running `cargo check`, `cargo test`, and `cargo bench` via `bash`. Correct any performance regressions or diagnostics immediately.
