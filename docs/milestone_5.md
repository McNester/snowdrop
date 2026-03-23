# Milestone 5 -- JOIN (INNER JOIN, LEFT JOIN)

**Status:** Complete

## Deliverables

### JOIN Detection and Routing

- `handle_select` detects JOIN presence in FROM clause, delegates to `handle_join_select`
- Supports single JOIN per query (M5 scope)
- Table alias resolution: `FROM users u JOIN orders o` prefixes row keys as `u.col`, `o.col`

### JOIN Execution (handle_join_select)

- Parse left and right tables with aliases from AST
- Extract equi-join ON conditions (supports multi-column joins)
- Fetch both table's rows via single MGET each
- Hash join: build HashMap on right table's join key values, probe with left
- INNER JOIN: only emit rows where both sides match
- LEFT JOIN: emit unmatched left rows with NULL-filled right columns

### Qualified Column Name Resolution

- `filter.rs`: `resolve_value` for `Identifier` tries fuzzy match `*.col_name` for JOIN rows
- `filter.rs`: `CompoundIdentifier` resolves `alias.column` directly
- `sort_rows`: handles `CompoundIdentifier` ORDER BY expressions
- `lookup_row_value`: fuzzy match used throughout aggregates and sorting

### JOIN Aggregates

- Aggregate detection works in JOIN path (COUNT, SUM, AVG, MIN, MAX)
- `extract_function_col_arg` handles `CompoundIdentifier` (e.g., `SUM(o.amount)`)
- `compute_single_aggregate` uses `lookup_row_value` for prefixed column names

### Helper Functions

- `extract_table_info`: table name + alias from TableFactor AST node
- `extract_join_conditions`: parse ON expr into (left_col, right_col) pairs
- `prefix_row`: prefix all row keys with "alias."
- `build_combined_schema`: merge schemas with prefixed column names
- `resolve_join_projection`: resolve SELECT items to (display_name, row_key) pairs
- `find_column_key`: resolve unqualified column to qualified key in combined schema

## Test Summary

- 74 total tests (41 executor, 11 keys, 9 types, 13 WAL)
- Existing filter tests validate predicate evaluation with qualified names
- JOIN logic tested via Docker acceptance criteria (no unit-test-friendly mock for dual-table SCAN/MGET)

## Acceptance Criteria Verification

| # | Criteria | Result |
|---|---|---|
| 1 | INNER JOIN returns matching rows only | Pass (alice 2 orders, bob 1 order, charlie excluded) |
| 2 | LEFT JOIN includes unmatched left rows with NULLs | Pass (charlie appears with NULL order columns) |
| 3 | JOIN + WHERE filters on both tables | Pass (amount > 60 returns alice/bob) |
| 4 | JOIN + ORDER BY + LIMIT | Pass (top 2 by amount DESC) |
| 5 | JOIN 10k users x 50k orders under 1 second | Pass (0.643s) |
| 6 | Unindexed FK uses hash join fallback | Pass |

## Testing Steps

```bash
docker compose down -v && docker compose up --build -d
curl -sf http://localhost:8080/health

# Setup
psql -h localhost -p 5432 -U snowdrop -d snowdrop -c "CREATE DATABASE testdb;"
psql -h localhost -p 5432 -U snowdrop -d snowdrop -c "CREATE TABLE testdb.users (id BIGINT PRIMARY KEY, name VARCHAR(255) NOT NULL, email VARCHAR(255));"
psql -h localhost -p 5432 -U snowdrop -d snowdrop -c "CREATE TABLE testdb.orders (id BIGINT PRIMARY KEY, user_id BIGINT NOT NULL, amount INT NOT NULL, product VARCHAR(255));"

psql ... -c "INSERT INTO testdb.users (id, name, email) VALUES (1, 'alice', 'alice@test.com');"
psql ... -c "INSERT INTO testdb.users (id, name, email) VALUES (2, 'bob', 'bob@test.com');"
psql ... -c "INSERT INTO testdb.users (id, name, email) VALUES (3, 'charlie', 'charlie@test.com');"
psql ... -c "INSERT INTO testdb.orders (id, user_id, amount, product) VALUES (101, 1, 50, 'widget');"
psql ... -c "INSERT INTO testdb.orders (id, user_id, amount, product) VALUES (102, 1, 75, 'gadget');"
psql ... -c "INSERT INTO testdb.orders (id, user_id, amount, product) VALUES (103, 2, 100, 'thing');"

# AC1: INNER JOIN
psql ... -c "SELECT u.name, o.amount, o.product FROM testdb.users u INNER JOIN testdb.orders o ON u.id = o.user_id;"

# AC2: LEFT JOIN
psql ... -c "SELECT u.name, o.amount, o.product FROM testdb.users u LEFT JOIN testdb.orders o ON u.id = o.user_id;"

# AC3: JOIN + WHERE
psql ... -c "SELECT u.name, o.amount FROM testdb.users u INNER JOIN testdb.orders o ON u.id = o.user_id WHERE o.amount > 60;"

# AC4: JOIN + ORDER BY + LIMIT
psql ... -c "SELECT u.name, o.product FROM testdb.users u INNER JOIN testdb.orders o ON u.id = o.user_id ORDER BY o.amount DESC LIMIT 2;"

# AC5: Performance (insert 10k users + 50k orders in batches, then:)
time psql ... -c "SELECT COUNT(*) FROM testdb.users u INNER JOIN testdb.orders o ON u.id = o.user_id;"
```
