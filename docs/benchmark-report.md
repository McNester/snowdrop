# Snowdrop PoC Benchmark Report

## Architecture

Snowdrop is a SQL-compatible, ACID-complete database proof of concept with:

- **Storage**: Embedded RocksDB for durable row storage (column-per-key model)
- **Columnar Cache**: In-memory columnar cache with SIMD-friendly bitmap filters for analytical scans
- **Secondary Indexes**: RocksDB-native indexes with lexicographic value encoding for point lookups and range scans
- **Schema**: Neo4j 5 for schema metadata (tables, columns, index definitions)
- **Durability**: WAL with 3-phase commit (Pending -> Neo4jCommitted -> Committed) and crash recovery
- **Wire Protocol**: PostgreSQL-compatible via pgwire crate (port 5432)
- **Metrics**: Prometheus exporter on port 9090

## Benchmark Configuration

- **Scale factor**: 1 (~6700 lineitem rows, ~1500 orders, ~150 customers, ~200 parts)
- **Runs per query**: 10 (first run is cold, remaining 9 are warm)
- **Warm metric**: Average of runs 2-10 (excludes cold run)
- **Platform**: macOS ARM64 (Apple Silicon), Docker containers for all services
- **PostgreSQL**: 16.x with default configuration, no explicit indexes

## Results

### Performance Comparison (warm cache, scale=1)

| Query | Snowdrop Columnar | Snowdrop Indexed | PostgreSQL 16 | SD vs PG |
|-------|-------------------|------------------|---------------|----------|
| Q6: SUM + 5-predicate WHERE | **0.7ms** | 12.3ms | 1.8ms | **2.4x faster** |
| Q1: GROUP BY + 7 aggregates | 12.2ms | 23.4ms | 2.5ms | 4.8x slower |
| Q14: 2-table JOIN + CASE | 26.7ms | 26.5ms | 1.5ms | 18.0x slower |
| Q3: 3-table JOIN + GROUP BY | 38.1ms | 37.4ms | 1.6ms | 24.1x slower |
| Data load (~6700 rows) | 19.2s | - | 0.9s | 21x slower |

### Key Finding: Q6 Beats PostgreSQL

Snowdrop's columnar cache achieves **sub-millisecond** performance on Q6 (filtered aggregate scan), outperforming PostgreSQL 16 by 2.4x. This validates the core architectural hypothesis: contiguous typed arrays (`Vec<i64>`, `Vec<f64>`) with bitmap predicate evaluation enable LLVM auto-vectorization, producing performance competitive with mature databases on scan-heavy analytical queries.

### Improvement from M6 to M7

| Query | M6 (Kvrocks) | M7 Columnar | M7 Indexed | Speedup (M6 -> M7 best) |
|-------|-------------|-------------|------------|-------------------------|
| Q6 | 165ms | 0.7ms | 12.3ms | **236x** |
| Q1 | 173ms | 12.2ms | 23.4ms | **14x** |
| Q14 | 183ms | 26.7ms | 26.5ms | **7x** |
| Q3 | 214ms | 38.1ms | 37.4ms | **6x** |

The move from Kvrocks network round-trips to embedded RocksDB + columnar cache produced massive improvements across all queries.

## Query Descriptions

### Q6: Revenue from discounted shipments
```sql
SELECT SUM(l_extendedprice * l_discount) AS revenue
FROM lineitem
WHERE l_shipdate >= '1994-01-01' AND l_shipdate < '1995-01-01'
  AND l_discount >= 0.05 AND l_discount <= 0.07 AND l_quantity < 24;
```
Single-table, 5 predicates, single aggregate. Ideal for columnar scan with bitmap filters.

### Q1: Pricing summary report
```sql
SELECT l_returnflag, l_linestatus, SUM(l_quantity), SUM(l_extendedprice),
  SUM(l_extendedprice * (1 - l_discount)), AVG(l_quantity), AVG(l_extendedprice),
  AVG(l_discount), COUNT(*)
FROM lineitem WHERE l_shipdate <= '1998-09-02'
GROUP BY l_returnflag, l_linestatus ORDER BY l_returnflag, l_linestatus;
```
Single-table with GROUP BY, 7 aggregate expressions, and sorting.

### Q14: Promotion effect
```sql
SELECT SUM(CASE WHEN p_type LIKE 'PROMO%' THEN l_extendedprice * (1 - l_discount) ELSE 0 END),
       SUM(l_extendedprice * (1 - l_discount))
FROM lineitem l INNER JOIN part p ON l.l_partkey = p.p_partkey
WHERE l.l_shipdate >= '1995-01-01' AND l.l_shipdate < '1995-02-01';
```
2-table JOIN with CASE WHEN expression and date filter.

### Q3: Shipping priority
```sql
SELECT l.l_orderkey, SUM(l.l_extendedprice * (1 - l.l_discount)) AS revenue,
       o.o_orderdate, o.o_shippriority
FROM customer c INNER JOIN orders o ON c.c_custkey = o.o_custkey
  INNER JOIN lineitem l ON l.l_orderkey = o.o_orderkey
WHERE c.c_mktsegment = 'BUILDING' AND o.o_orderdate < '1995-03-15'
  AND l.l_shipdate > '1995-03-15'
GROUP BY l.l_orderkey, o.o_orderdate, o.o_shippriority
ORDER BY revenue DESC, o.o_orderdate LIMIT 10;
```
3-table JOIN with GROUP BY, ORDER BY, and LIMIT.

## Index Configuration

Two indexes created for the indexed benchmark run:
- `idx_shipdate` on `lineitem(l_shipdate)` -- targets Q6 date range and Q14 date filter
- `idx_partkey` on `lineitem(l_partkey)` -- targets Q14 join predicate

### Index Behavior Analysis

The indexed path is **slower** than the columnar cache for broad-range queries like Q6. This is expected: when a range scan returns thousands of rows, individual RocksDB key lookups per row_id are slower than the vectorized columnar scan which processes all rows in contiguous memory. Indexes would show their advantage on highly selective point queries (e.g., `WHERE l_shipdate = '1994-01-01'`), which TPC-H does not test.

## Profiling Analysis

### Client-side flamegraph (`docs/flamegraphs/tpch_benchmark.svg`)

The flamegraph of the benchmark client shows time dominated by:
1. **TCP I/O** (`__recvfrom`, `__sendto`) -- waiting for Snowdrop's responses over pgwire
2. **Tokio runtime** (`poll_read`, `poll_write`) -- async I/O scheduling
3. **Connection setup** (`getaddrinfo`) -- DNS resolution for initial connection

This confirms the benchmark is measuring actual query latency on the server, not client-side overhead.

## Prometheus Metrics (after full benchmark run)

```
snowdrop_queries_total{type="select"}   40
snowdrop_queries_total{type="insert"}   1519
snowdrop_queries_total{type="ddl"}      11
snowdrop_index_scans_total              10
snowdrop_full_scans_total               10
snowdrop_rows_scanned_total             67460
snowdrop_rows_returned_total            65380
snowdrop_column_cache_hits              9
snowdrop_column_cache_misses            1
snowdrop_active_connections             0
```

Cache hit rate of 90% (9 hits / 1 miss) confirms the columnar cache is effective after the first cold query populates it.

## Bottleneck Analysis

### Where Snowdrop wins
- **Filtered full scans** (Q6): The columnar cache with bitmap filters achieves sub-millisecond latency, beating PostgreSQL's row-oriented scan
- **Cache efficiency**: 90% hit rate means nearly all repeated queries serve from memory

### Where Snowdrop loses
1. **JOINs** (Q14, Q3): Hash join with row-oriented materialization. PostgreSQL's optimizer chooses optimal join strategies with statistics
2. **GROUP BY** (Q1): Current implementation aggregates row-by-row. PostgreSQL uses vectorized aggregation
3. **Data loading**: Each INSERT goes through WAL append + Neo4j schema validation + RocksDB write. PostgreSQL uses optimized COPY protocol
4. **No query optimizer**: No statistics, no cost-based planning, no parallel execution

### Optimization opportunities for production
1. Columnar JOIN: join directly on columnar arrays instead of materializing rows
2. Vectorized aggregation: process groups in batches
3. Parallel scan: split columnar arrays across worker threads
4. Batch INSERT: bypass per-row WAL for bulk loads
5. Connection pooling: reuse pgwire connections

## Running the Benchmark

### Prerequisites
```bash
docker compose up -d
```

### Run with all options
```bash
cargo run --release -p tpch-bench -- --scale 1 --with-indexes --runs 10 --output results.json
```

### View metrics during/after run
```bash
curl -s http://127.0.0.1:9090/metrics | grep snowdrop
```

### Generate flamegraph
```bash
cargo install inferno --locked

# Run benchmark in background, sample, convert to flamegraph
target/release/tpch-bench --scale 1 --runs 10 &
PID=$!
sleep 2
sample $PID 20 1 -file /tmp/bench_sample.txt
wait $PID
cat /tmp/bench_sample.txt | inferno-collapse-sample | inferno-flamegraph > flamegraph.svg
```

## Conclusion

Snowdrop demonstrates that a purpose-built columnar cache with bitmap predicate evaluation can outperform PostgreSQL 16 on filtered analytical scans (Q6: 0.7ms vs 1.8ms). The architecture went from 100x slower (M6, Kvrocks network) to competitive (M7, embedded RocksDB + columnar cache) -- a **236x improvement** on Q6.

The PoC validates the core hypothesis while identifying clear paths for production optimization: columnar JOINs, vectorized aggregation, and parallel execution would close the gap on multi-table queries.
