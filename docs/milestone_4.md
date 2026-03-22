# Milestone 4 -- SELECT (Single Table)

**Status:** Complete

## Deliverables

### TypedValue System (executor::typed_value)

- `TypedValue` enum: `Int(i64)`, `Float(f64)`, `Text(String)`, `Boolean(bool)`, `Null`
- `from_bytes(bytes, data_type)`: deserializes Kvrocks raw bytes using column type metadata
- SQL-compatible comparison: `sql_eq`, `sql_lt`, `sql_gt` with NULL semantics (NULL = NULL is false)
- `cmp_order`: total ordering for ORDER BY (NULL sorts first)
- `sql_like`: pattern matching with prefix%, %suffix, %contains%
- Numeric coercion: Int/Float cross-type comparisons, `as_f64()` for aggregates

### Predicate Evaluation (executor::filter)

- Full WHERE clause evaluation from sqlparser AST
- Operators: `=`, `!=`, `<`, `<=`, `>`, `>=`
- Compound: `AND`, `OR`, `NOT`
- Special: `IS NULL`, `IS NOT NULL`, `IN (...)`, `BETWEEN`, `LIKE`/`NOT LIKE`
- Nested expressions via `Expr::Nested`
- `bytes_to_row()`: converts raw Kvrocks data to typed Row (HashMap<col_name, TypedValue>)

### SELECT Pipeline (query_router)

1. Parse SELECT AST, resolve FROM table
2. Resolve column projection (SELECT * or named columns)
3. Detect aggregate vs. plain select
4. Fetch rows: SCAN + single batched MGET
5. Apply WHERE predicate filter
6. For aggregates: compute and return single-row result
7. For plain SELECT: apply ORDER BY, LIMIT/OFFSET, stream to pgwire

### Aggregates

- `COUNT(*)`: row count. `COUNT(col)`: non-NULL count
- `SUM(col)`: numeric accumulation, returns NULL on empty set
- `AVG(col)`: SUM/COUNT as float, returns NULL on empty set
- `MIN(col)`, `MAX(col)`: running min/max, skip NULLs
- `GROUP BY`: HashMap bucketing by group key columns

### Performance Optimizations

- **Single MGET**: all column keys fetched in one round-trip instead of per-row MGET
- **COUNT(\*) fast path**: `SELECT COUNT(*) FROM table` without WHERE skips MGET entirely, counts unique row_ids from SCAN keys only
- Result: COUNT(\*) on 100k rows completes in ~0.6s

## Test Summary

- 74 total tests (41 executor: 20 TypedValue + 16 filter + 5 older, 11 keys, 9 types, 13 wal)
- TypedValue: deserialization, comparisons, LIKE patterns, NULL semantics
- Filter: all predicate types (eq, neq, lt, gt, and, or, between, in, is_null, like)
- bytes_to_row integration

## Acceptance Criteria Verification

| # | Criteria | Result |
|---|---|---|
| 1 | `SELECT * FROM users` returns all rows | Pass |
| 2 | `SELECT name, email FROM users WHERE id = 1` returns correct row | Pass |
| 3 | `SELECT * FROM users WHERE name LIKE 'alice%'` returns correct rows | Pass |
| 4 | `SELECT * FROM users ORDER BY name ASC LIMIT 10 OFFSET 20` | Pass |
| 5 | `SELECT COUNT(*), AVG(age) FROM users` returns correct aggregates | Pass |
| 6 | `SELECT COUNT(*) FROM users WHERE age > 30 AND age < 50` | Pass |
| 7 | 100k rows, `SELECT COUNT(*)` returns 100000, under 1 second | Pass (0.6s) |
| 8 | SELECT returning 0 rows returns empty result set, not error | Pass |

## Testing Steps

```bash
docker compose down -v && docker compose up --build -d
curl -sf http://localhost:8080/health

# Setup
psql -h localhost -p 5432 -U snowdrop -d snowdrop -c "CREATE DATABASE testdb;"
psql -h localhost -p 5432 -U snowdrop -d snowdrop -c "CREATE TABLE testdb.users (id BIGINT PRIMARY KEY, name VARCHAR(255) NOT NULL, age INT, email VARCHAR(255));"
psql -h localhost -p 5432 -U snowdrop -d snowdrop -c "INSERT INTO testdb.users (id, name, age, email) VALUES (1, 'alice', 25, 'alice@test.com');"
psql -h localhost -p 5432 -U snowdrop -d snowdrop -c "INSERT INTO testdb.users (id, name, age, email) VALUES (2, 'bob', 35, 'bob@test.com');"

# SELECT tests
psql ... -c "SELECT * FROM testdb.users;"
psql ... -c "SELECT name FROM testdb.users WHERE id = 1;"
psql ... -c "SELECT * FROM testdb.users WHERE name LIKE 'ali%';"
psql ... -c "SELECT * FROM testdb.users ORDER BY name ASC LIMIT 1 OFFSET 1;"
psql ... -c "SELECT COUNT(*), AVG(age) FROM testdb.users;"
psql ... -c "SELECT COUNT(*) FROM testdb.users WHERE age > 20 AND age < 40;"

# Performance: insert 100k rows (batches of 100), then:
psql ... -c "SELECT COUNT(*) FROM testdb.users;"
```
