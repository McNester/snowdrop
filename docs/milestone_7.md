# Milestone 7: Secondary Indexes, Metrics, and Benchmark Report

## Overview

Final PoC milestone. Adds RocksDB-native secondary indexes for fast point lookups and range scans, Prometheus metrics instrumentation, and an enhanced benchmark suite comparing Snowdrop vs PostgreSQL 16 with and without indexes.

## Architecture Decisions

### RocksDB-Native Indexes (not Neo4j)

The original spec used Neo4j `IndexEntry` nodes. Since the architecture moved to embedded RocksDB, storing index entries in Neo4j would add 1-5ms network latency per lookup -- slower than the columnar cache. Instead, index entries are stored as RocksDB keys with empty values:

```
idx:{db_b62}:{tbl_b62}:{col_b62}:{value_encoded}:{row_b62}
```

Neo4j retains index metadata only (which columns are indexed, index names).

### Lexicographic Value Encoding

Index keys must sort lexicographically in the same order as their natural values:

| Type | Encoding | Example |
|------|----------|---------|
| Integers | Offset encoding: `(v as u64).wrapping_add(u64::MAX/2 + 1)`, zero-padded to 20 digits | `-100` -> `"09223372036854775708"` |
| Floats | IEEE 754 sign-bit flip -> hex | `3.14` -> `"4009...` |
| Dates | ISO-8601 pass-through | `"1994-01-01"` |
| Strings | Raw UTF-8 | `"BUILDING"` |
| Booleans | `"0"` / `"1"` | |

### Hybrid Query Strategy

The query planner tries an index scan first, then falls back to the columnar cache:

1. Extract equality/range predicates from WHERE clause
2. Check if predicate column is indexed (from cached schema)
3. If indexed: point lookup or range scan -> row IDs -> MGET -> post-filter remaining predicates
4. If not indexed: full columnar cache scan (existing path)

## Features Added

### Index Infrastructure (`keys/`, `executor/src/index.rs`)
- Index key construction: `build_index_key`, `index_prefix`, `index_point_prefix`, `parse_index_key`
- Value encoding for all supported types with lexicographic ordering
- Point lookup and range scan via RocksDB prefix iteration
- Backfill and drop helpers for existing data

### Schema Metadata (`schema/src/ddl.rs`)
- `create_index` / `drop_index` on `Neo4jConnection`
- `get_indexed_columns` / `list_indexes` queries
- Extended `ColumnInfo` with `indexed: bool` and `index_name: Option<String>`

### Write Path Index Maintenance (`executor/src/dml.rs`)
- INSERT: builds index entries in same `mset` batch as row data
- UPDATE: deletes old index keys, writes new ones after column merge
- DELETE: reads row before deletion to build index keys for cleanup
- WAL replay: reconstructs index entries from row data + schema

### Query Planner (`orchestrator/src/query_router.rs`)
- `CREATE INDEX` / `DROP INDEX` statement handling with backfill
- `try_index_scan`: extracts index candidates from WHERE, executes point/range lookup
- `extract_index_candidate`: parses equality, range, and comparison predicates
- Logs scan type ("index" vs "columnar") for each SELECT

### Prometheus Metrics
- Exporter on `:9090` via `metrics-exporter-prometheus`
- Query counters: `snowdrop_queries_total{type}`, `snowdrop_query_duration_seconds{type}`
- Scan counters: `snowdrop_index_scans_total`, `snowdrop_full_scans_total`
- Row counters: `snowdrop_rows_scanned_total`, `snowdrop_rows_returned_total`
- Connection gauge: `snowdrop_active_connections`
- Cache gauge: `snowdrop_column_cache_entries`

### Enhanced Benchmark Suite (`tpch-bench/`)
- Structured JSON output (`--output results.json`)
- Multi-scale support (`--scale 1,10`)
- Index benchmark mode (`--with-indexes`)
- Auto-generated markdown report (`docs/benchmark-report.md`)
- PostgreSQL 16 comparison (configurable via `--pg-port`)

## Test Coverage

124 tests total:
- 85 executor tests (including 14 new index tests)
- 17 keys tests (including 6 new index key tests)
- 9 RocksDB storage tests
- 13 WAL tests

## Running

### Prerequisites
```bash
docker compose up -d
```

### Create an Index
```sql
CREATE INDEX idx_shipdate ON lineitem(l_shipdate);
```

### Verify Index Usage
```bash
# Query with index -- check orchestrator logs for "scan_type=index"
sndsql -d tpch -c "SELECT COUNT(*) FROM lineitem WHERE l_shipdate = '1994-01-01';"
```

### Run Benchmark
```bash
cargo run --release -p tpch-bench -- --scale 1 --with-indexes --output results.json
```

### View Metrics
```bash
curl -s localhost:9090/metrics | grep snowdrop
```

### Generate Flamegraph
```bash
cargo install flamegraph
cargo flamegraph -p tpch-bench -- --scale 1
```

## Files Modified

| File | Changes |
|------|---------|
| `keys/src/lib.rs` | Index key construction + 6 tests |
| `executor/src/index.rs` | New: encoding, scan, backfill + 14 tests |
| `executor/src/lib.rs` | Register index module |
| `executor/src/dml.rs` | Index maintenance in INSERT/UPDATE/DELETE/replay |
| `executor/src/filter.rs` | Updated test fixtures |
| `executor/Cargo.toml` | Added metrics dep |
| `schema/src/ddl.rs` | Index DDL methods, extended ColumnInfo |
| `orchestrator/src/query_router.rs` | Index-aware planner, CREATE/DROP INDEX, metrics |
| `orchestrator/src/main.rs` | Prometheus exporter init |
| `orchestrator/src/pgwire_handler.rs` | Connection gauge |
| `orchestrator/Cargo.toml` | Added metrics + prometheus deps |
| `Cargo.toml` (workspace) | Added metrics workspace deps |
| `tpch-bench/src/main.rs` | Full rewrite with structured output |
| `tpch-bench/Cargo.toml` | Added serde, serde_json |
