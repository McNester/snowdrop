# Milestone 6: TPC-H Benchmark vs PostgreSQL 16

## Overview

TPC-H-inspired benchmark comparing Snowdrop against PostgreSQL 16 on four analytical queries. Scale factor 1 (~6500 lineitem rows, 1500 orders, 150 customers, 200 parts).

## Benchmark Results (scale=1)

| Query | Snowdrop (warm) | PostgreSQL 16 (warm) | Ratio |
|-------|-----------------|---------------------|-------|
| Q6: SUM + date range filter | 165ms | 1.4ms | 118x |
| Q1: GROUP BY + 7 aggregates | 173ms | 2.1ms | 82x |
| Q14: 2-table JOIN + CASE WHEN | 183ms | 1.5ms | 122x |
| Q3: 3-table JOIN + GROUP BY + ORDER BY + LIMIT | 214ms | 1.5ms | 143x |
| Data load (~6500 lineitem rows) | 24.6s | 0.9s | 27x |

## Analysis

Snowdrop is ~100x slower than PostgreSQL 16 on analytical queries at this scale. Primary bottlenecks:

1. **No indexes**: Every query performs a full Kvrocks SCAN. PostgreSQL uses seq scans too at this scale, but its in-process storage is fundamentally faster than network round-trips to Kvrocks.
2. **Network overhead**: Each row fetch requires RESP protocol round-trips to Kvrocks over TCP. PostgreSQL reads directly from shared buffers.
3. **No query optimizer**: Snowdrop uses naive hash join with full materialization. No statistics, no cost-based planning.
4. **Single-threaded execution**: No parallel scan or parallel aggregation.
5. **INSERT batching**: Snowdrop processes each batch INSERT through WAL + Neo4j schema validation + Kvrocks MSET, while PostgreSQL uses optimized bulk loading.

## Features Added for M6

- **sndsql CLI**: Custom SQL client (`sndsql`) replacing psql for Snowdrop interaction
- **Arithmetic expressions**: `price * discount`, `1 - discount` in SELECT, WHERE, and aggregates
- **CASE WHEN**: Both simple and searched CASE in projections
- **Multi-table JOIN**: Chained hash join supporting 3+ tables with correct join key direction detection
- **Expression-aware aggregates**: `SUM(price * (1 - discount))` evaluates per-row expressions
- **GROUP BY with ORDER BY + LIMIT**: Full analytical query pipeline

## TPC-H Queries

### Q6: Revenue from discounted shipments
```sql
SELECT SUM(l_extendedprice * l_discount) AS revenue
FROM lineitem
WHERE l_shipdate >= '1994-01-01' AND l_shipdate < '1995-01-01'
  AND l_discount >= 0.05 AND l_discount <= 0.07
  AND l_quantity < 24;
```

### Q1: Pricing summary report
```sql
SELECT l_returnflag, l_linestatus,
  SUM(l_quantity), SUM(l_extendedprice),
  SUM(l_extendedprice * (1 - l_discount)),
  AVG(l_quantity), AVG(l_extendedprice), AVG(l_discount),
  COUNT(*)
FROM lineitem
WHERE l_shipdate <= '1998-09-02'
GROUP BY l_returnflag, l_linestatus
ORDER BY l_returnflag, l_linestatus;
```

### Q14: Promotion effect
```sql
SELECT SUM(CASE WHEN p_type LIKE 'PROMO%' THEN l_extendedprice * (1 - l_discount) ELSE 0 END),
       SUM(l_extendedprice * (1 - l_discount))
FROM lineitem l INNER JOIN part p ON l.l_partkey = p.p_partkey
WHERE l.l_shipdate >= '1995-01-01' AND l.l_shipdate < '1995-02-01';
```

### Q3: Shipping priority (3-table join)
```sql
SELECT l.l_orderkey, SUM(l.l_extendedprice * (1 - l.l_discount)) AS revenue,
       o.o_orderdate, o.o_shippriority
FROM customer c
  INNER JOIN orders o ON c.c_custkey = o.o_custkey
  INNER JOIN lineitem l ON l.l_orderkey = o.o_orderkey
WHERE c.c_mktsegment = 'BUILDING'
  AND o.o_orderdate < '1995-03-15'
  AND l.l_shipdate > '1995-03-15'
GROUP BY l.l_orderkey, o.o_orderdate, o.o_shippriority
ORDER BY revenue DESC, o.o_orderdate
LIMIT 10;
```

## Running the Benchmark

### Prerequisites
```bash
docker compose up -d
```

### Run
```bash
cargo run --release --bin tpch-bench
```

Optional scale factor argument (default 1):
```bash
cargo run --release --bin tpch-bench -- 5
```

### Verify individual queries with sndsql
```bash
cargo install --path sndsql --locked
sndsql -d snowdrop -c "SELECT SUM(l_extendedprice * l_discount) FROM tpch.lineitem WHERE l_shipdate >= '1994-01-01' AND l_shipdate < '1995-01-01';"
```

## Next Steps (M7: Indexes)

Adding B-tree indexes via Neo4j IndexEntry nodes should dramatically improve indexed queries. Target: Q6 under 10ms with l_shipdate indexed.
