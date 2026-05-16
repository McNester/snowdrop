# Snowdrop Benchmark Results

This document keeps the current benchmark snapshot separate from the architectural explanation in [performance-architecture.md](/e:/snowdrop-1/docs/performance-architecture.md:1).

It is intended as the compact reference for presentation material, quick comparisons, and thesis updates.

## Current TPC-H Snapshot

Source of this snapshot:

- [performance-architecture.md](/e:/snowdrop-1/docs/performance-architecture.md:1)
- the focused TPC-H workload described in [benchmarking.md](/e:/snowdrop-1/docs/benchmarking.md:1)

## Snowdrop vs PostgreSQL 16

TPC-H, scale factor `1`, warm-cache comparison:

| Query | Snowdrop (cold) | Snowdrop (warm) | PostgreSQL 16 | Warm vs PG |
|---|---|---|---|---|
| Q6 (SUM + date range) | 8.2ms | **0.8ms** | 1.5ms | **1.9x faster** |
| Q1 (GROUP BY + 6 aggregates) | 15.9ms | **10.9ms** | 4.6ms | 2.4x slower |
| Q14 (2-table JOIN + CASE) | 42.2ms | **26.1ms** | 1.5ms | 17x slower |
| Q3 (3-table JOIN + GROUP BY) | 62.6ms | **44.3ms** | 1.6ms | 28x slower |

## What These Numbers Mean

- Snowdrop is already strong on filtered, scan-heavy analytical reads, especially `Q6`.
- Snowdrop remains materially behind PostgreSQL on multi-table TPC-H JOIN queries.
- The benchmark should be interpreted as a workload signal, not as blanket proof that Snowdrop is faster on every SQL pattern.

## Main Bottleneck Interpretation

The main remaining bottleneck is the JOIN path:

- single-table scan-heavy queries can use the in-memory columnar cache and SIMD-friendly filtering
- multi-table JOIN queries still rely on a much less optimized relation path
- PostgreSQL still has a clearly superior mature optimizer and join execution engine

This matches the current architecture notes in [performance-architecture.md](/e:/snowdrop-1/docs/performance-architecture.md:1).

## How To Refresh The Results

Preferred unified run:

```powershell
docker compose up -d --build orchestrator
$env:PGPASSWORD = "<your-postgres-password>"
cargo run -p snowdrop-testkit -- run-all --with-tpch
```

Direct benchmark run:

```powershell
docker compose up -d --build orchestrator
$env:PGPASSWORD = "<your-postgres-password>"
cargo run --release -p tpch-bench -- --scale 1 --runs 10 --with-indexes --seed 42 --output reports/benchmarks/tpch-results.json
```

Related docs:

- [benchmarking.md](/e:/snowdrop-1/docs/benchmarking.md:1)
- [performance-architecture.md](/e:/snowdrop-1/docs/performance-architecture.md:1)
