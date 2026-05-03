# Snowdrop Performance Architecture

> Embedded RocksDB, in-memory columnar cache, SIMD-filtered scans

Note: `Snowdrop architecture.pdf` is now partially historical. The current source of truth is this document plus the repository code. In particular, secondary indexes now live in RocksDB, while Neo4j is used for schema metadata only.

---

## Overview

Snowdrop's read path evolved through four optimization stages to close a 110x gap with PostgreSQL 16 on full-scan TPC-H queries. The final architecture beats PostgreSQL on filtered scans (Q6) and remains within 2-3x on aggregate queries (Q1).

| Stage | Q6 latency | vs PostgreSQL 16 |
|---|---|---|
| Kvrocks TCP (original) | 165ms | 110x slower |
| Embedded RocksDB + row-oriented keys | 17ms | 11x slower |
| Zero-copy RowView + lazy decode + fused scan/filter | 5.7ms | 3.8x slower |
| **In-memory columnar cache + SIMD filters** | **0.8ms** | **1.9x faster** |

---

## Storage Layer: Embedded RocksDB

The original design used Apache Kvrocks (Redis protocol over RocksDB) as an external TCP service. Profiling revealed that network round-trips consumed ~70% of query time. The storage layer was replaced with an embedded RocksDB instance running in-process.

### StorageBackend Trait

Both implementations share a common trait (`executor/src/storage.rs`):

```rust
#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn ping(&self) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;
    async fn mget(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>>;
    async fn mset(&self, pairs: &[(String, Vec<u8>)]) -> Result<()>;
    async fn del(&self, keys: &[String]) -> Result<u64>;
    async fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>>;
    async fn exists(&self, key: &str) -> Result<bool>;
}
```

The `STORAGE_BACKEND` env var selects the implementation: `rocksdb` (default) or `kvrocks`.

Runtime listener ports are also env-driven:

- `PGWIRE_PORT` (default `5432`)
- `HEALTH_PORT` (default `8080`)
- `METRICS_PORT` (default `9090`)

### RocksDB Configuration

- Bloom filters on prefix reads
- 64MB write buffer
- Level compaction
- Data directory: `/data/rocks` (configurable via `ROCKS_PATH`)

---

## Row-Oriented Storage Format

### Key Format

Each row is stored as a single key-value pair:

```
{db_id}:{table_id}:{row_id}  ->  [row bytes]
```

Key length: 18 chars (2+1+3+1+11). This replaces the original one-key-per-column format (21-char keys), reducing key count by the number of columns per row (typically 10-16x).

### Zero-Copy Row Format (RowView)

Row values use a flat binary format that supports zero-copy reads (`executor/src/row_format.rs`):

```
[version: 0xFF][num_cols: u16][col_id: u32, offset: u32, len: u16]...[raw bytes]
```

- **Version byte** `0xFF` distinguishes from legacy bincode format (backward compatible)
- **Directory entries**: 10 bytes each, enable O(1) column access by index
- **RowView** borrows the raw `&[u8]` and reads columns via offset arithmetic -- no heap allocation
- `find_col(col_id)` does linear scan of the directory (fast for typical 5-20 columns)

Legacy bincode-encoded rows (version byte != `0xFF`) are detected and decoded transparently.

---

## Schema Cache

An in-memory HashMap caches Neo4j schema lookups (`describe_table`, `get_database`, `get_table`). Eliminates Neo4j round-trips on the read path.

- **Warmed on startup**: all existing schemas loaded into memory
- **Invalidated by DDL**: CREATE/DROP TABLE, ALTER TABLE clear affected entries
- **Impact**: schema lookup dropped from ~7.7ms to ~0.0ms

---

## In-Memory Columnar Cache

The highest-impact optimization. For single-table full scans, row data is extracted into contiguous typed arrays on first access, then reused for subsequent queries.

### Architecture

```
RocksDB (row-oriented, durable)
  |
  | first scan: extract into typed arrays
  v
In-Memory Columnar Cache (per-table)
  |
  | SIMD-filtered comparison loops
  v
Bitmap of qualifying rows
  |
  | materialize only matching rows
  v
Row objects for aggregation / pgwire encoding
```

### Typed Column Arrays (`executor/src/column_store.rs`)

Each column is stored as a native Rust array matching its SQL type:

| SQL Type | Rust Storage | Width |
|---|---|---|
| TINYINT / SMALLINT / INT / BIGINT | `Vec<i64>` | 8 bytes |
| FLOAT / DOUBLE / DECIMAL / NUMERIC | `Vec<f64>` | 8 bytes |
| DATE | `Vec<i32>` (days since epoch) | 4 bytes |
| VARCHAR / TEXT / CHAR | `Vec<String>` | variable |
| BOOLEAN | `Vec<bool>` | 1 byte |

Dates are converted from "YYYY-MM-DD" strings to `i32` days-since-epoch using Howard Hinnant's civil calendar algorithm. This enables SIMD integer comparison instead of string comparison.

Each column has a companion `NullBitmap` (u64-packed bits) tracking NULL values.

### Column Cache Lifecycle

1. **Build**: On first scan of a table, all rows are extracted from RocksDB into typed arrays
2. **Use**: Subsequent scans read directly from the cache (zero RocksDB I/O)
3. **Invalidate**: Any INSERT, UPDATE, or DELETE on the table clears the cache
4. **Rebuild**: Next scan rebuilds from RocksDB

### SIMD-Friendly Filter Functions

Predicates are evaluated column-at-a-time using tight scalar loops that LLVM auto-vectorizes to NEON (ARM) or AVX2/SSE (x86):

```rust
pub fn filter_i64(values: &[i64], op: CmpOp, threshold: i64) -> Bitmap {
    let mut bm = Bitmap::all_false(len);
    for (i, &v) in values.iter().enumerate() {
        let pass = match op {
            CmpOp::Lt => v < threshold,
            CmpOp::GtEq => v >= threshold,
            // ...
        };
        bm.words[i / 64] |= (pass as u64) << (i % 64);
    }
    bm
}
```

Key properties for auto-vectorization:
- Contiguous arrays of primitive types
- No branches in the hot loop (boolean-to-int conversion)
- Results packed into u64 bitmasks

### Predicate Decomposition

WHERE clauses are decomposed into per-column bitmap operations:

```sql
WHERE l_shipdate >= '1994-01-01'      -- filter_i32(date_col, GtEq, 8766) -> bitmap1
  AND l_shipdate < '1995-01-01'       -- filter_i32(date_col, Lt, 9131)   -> bitmap2
  AND l_discount BETWEEN 0.05 AND 0.07 -- filter_f64 GtEq + LtEq         -> bitmap3 & bitmap4
  AND l_quantity < 24                  -- filter_i64(qty_col, Lt, 24)      -> bitmap5
-- Final: bitmap1 & bitmap2 & bitmap3 & bitmap4 & bitmap5
```

Compound predicates (AND/OR) combine bitmaps with bitwise operations. Unsupported patterns (LIKE, CASE, subqueries, arithmetic expressions) fall back to row-by-row evaluation using the standard `evaluate_predicate` function.

---

## Query Execution: Read Path (Updated)

```
1. Parse SQL -> AST
2. Schema lookup from in-memory cache (0ms)
3. Compute needed column set (WHERE + SELECT + ORDER BY + GROUP BY)
4. Check columnar cache for this table:
   a. Cache miss: scan RocksDB, build typed arrays, cache them
   b. Cache hit: use existing arrays (0ms I/O)
5. Decompose WHERE into per-column SIMD filters -> bitmap
6. Materialize matching rows (only needed columns, only matching rows)
7. Apply ORDER BY, GROUP BY, aggregates on materialized rows
8. Encode to pgwire and stream to client
```

### JOIN Path

JOIN queries still use the row-oriented `fetch_rows` path (no columnar cache). Each table is scanned independently, rows are prefixed with table alias, and a nested-loop or hash join is performed in the filter buffer. This is the primary remaining optimization opportunity.

---

## Benchmark Results (TPC-H, scale=1, ~6700 lineitem rows)

### Snowdrop vs PostgreSQL 16 (no indexes on either side)

| Query | Snowdrop (cold) | Snowdrop (warm) | PostgreSQL 16 | Warm vs PG |
|---|---|---|---|---|
| Q6 (SUM + date range) | 8.2ms | **0.8ms** | 1.5ms | **1.9x faster** |
| Q1 (GROUP BY + 6 aggregates) | 15.9ms | **10.9ms** | 4.6ms | 2.4x slower |
| Q14 (2-table JOIN + CASE) | 42.2ms | **26.1ms** | 1.5ms | 17x slower (JOIN) |
| Q3 (3-table JOIN + GROUP BY) | 62.6ms | **44.3ms** | 1.6ms | 28x slower (JOIN) |

### Remaining Bottlenecks

| Component | Q6 contribution | Notes |
|---|---|---|
| Column cache lookup | ~0.01ms | HashMap get |
| SIMD bitmap filter | ~0.05ms | Auto-vectorized i32/f64 comparisons |
| Row materialization | ~0.5ms | Build HashMap<String, TypedValue> for matching rows |
| Aggregate computation | ~0.1ms | SUM over matching rows |
| pgwire encoding | ~0.1ms | Single result row |

---

## Testing

Run all tests:
```bash
cargo test
```

Run TPC-H benchmark:
```bash
docker compose up -d
cargo run --release -p tpch-bench -- --scale 1 --port 5432 --seed 42
```

Custom host port example:
```powershell
$env:SNOWDROP_HOST_PG_PORT = "15432"
$env:SNOWDROP_HOST_HEALTH_PORT = "18080"
$env:SNOWDROP_HOST_METRICS_PORT = "19090"
$env:POSTGRES_HOST_PORT = "15433"
docker compose up -d

cargo run --release -p tpch-bench -- --scale 1 --port 15432 --pg-port 15433 --seed 42
```

The benchmark loads data, runs each TPC-H query 3 times (discarding cold run), and compares against PostgreSQL 16 on identical data.
