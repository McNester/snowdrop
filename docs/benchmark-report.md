# Snowdrop Benchmark Report

This benchmark-only report is now considered legacy.

The current source of truth for generated test and benchmark output is:

- [test-report.md](/e:/snowdrop-1/docs/test-report.md:1)

`tpch-bench` still produces raw benchmark JSON, but the main human-facing markdown report is now owned by `snowdrop-testkit`.

Scale factor: 1 (~6000 lineitem rows)

## Architecture

Snowdrop is a SQL-compatible database PoC with:
- Embedded RocksDB for durable row storage
- In-memory columnar cache with SIMD-friendly bitmap filters
- RocksDB-native secondary indexes with lexicographic value encoding
- Neo4j for schema metadata only
- WAL for crash recovery
- PostgreSQL wire protocol (pgwire)

## Results

| Query | Snowdrop (warm) | SD Indexed (warm) | PostgreSQL 16 | vs PG (baseline) |
|---|---|---|---|---|
| Q6 | 0.8ms | 4.0ms | 36.3ms | 44.9x faster |
| Q1 | 16.3ms | 17.2ms | 77.4ms | 4.8x faster |
| Q14 | 7.8ms | 7.7ms | 38.9ms | 5.0x faster |
| Q3 | 16.8ms | 16.6ms | 43.7ms | 2.6x faster |

## Query Descriptions

- **Q6**: Revenue from discounted shipments (SUM + 5-predicate WHERE)
- **Q1**: Pricing summary (GROUP BY + 7 aggregates)
- **Q14**: Promotion effect (2-table JOIN + CASE WHEN)
- **Q3**: Shipping priority (3-table JOIN + GROUP BY + ORDER BY + LIMIT)

## Index Configuration

- `idx_shipdate` on `lineitem(l_shipdate)` -- benefits Q6 date range
- `idx_partkey` on `lineitem(l_partkey)` -- benefits Q14 join

## Analysis

### Single-Table Scans (Q6, Q1)

The in-memory columnar cache with SIMD-friendly bitmap filters enables sub-millisecond filtered scans on warm cache. Column data is stored as contiguous typed arrays (Vec<i64>, Vec<f64>, etc.) enabling LLVM auto-vectorization of comparison loops.

### JOIN Queries (Q14, Q3)

JOINs remain the primary bottleneck. The current implementation uses nested-loop joins with row-oriented materialization. Hash join and columnar join paths are the main optimization opportunity.

### Secondary Indexes

RocksDB-native secondary indexes store entries as prefix-iterable keys (`idx:{db}:{tbl}:{col}:{value_encoded}:{row_id}`). Integer values use offset encoding for lexicographic sort order. For selective queries, index scan avoids full table scan. For broad scans, the columnar cache path is preferred.

## Metrics

Prometheus metrics are exposed on `:9090/metrics` including:
- `snowdrop_queries_total{type}` -- query count by type
- `snowdrop_query_duration_seconds{type}` -- latency histogram
- `snowdrop_index_scans_total` / `snowdrop_full_scans_total`
- `snowdrop_rows_scanned_total` / `snowdrop_rows_returned_total`
- `snowdrop_active_connections`

## Running

```bash
docker compose up -d
export PGPASSWORD="<your-postgres-password>"
cargo run --release -p tpch-bench -- --scale 1 --with-indexes --seed 42 --output results.json
```

For containerized benchmark clients, use `--host` and `--pg-host` to target service names on the Docker network.
When PostgreSQL comparison is enabled, `tpch-bench` reads the password from `PGPASSWORD` first and falls back to `POSTGRES_PASSWORD`.

## Flamegraph

```bash
cargo install flamegraph
cargo flamegraph --bin tpch-bench -- --scale 1
```
