# Milestone 3 -- Writes + WAL: INSERT, UPDATE, DELETE with Crash Recovery

**Status:** Complete

## Deliverables

### DML Operations (executor::dml)

- `WriteEngine` struct provides INSERT, UPDATE, DELETE via references to Kvrocks, Neo4j, and WAL
- INSERT: generates random row_id (base62-encoded u64), validates NOT NULL constraints, checks PK uniqueness via `pk:{db}:{tbl}:{value}` mapping, writes all columns to Kvrocks via MSET
- UPDATE: finds matching rows by predicate (PK lookup or full scan), writes only changed columns via MSET, updates PK mapping if PK column is modified
- DELETE: finds matching rows by predicate, removes all column keys via DEL, removes PK mapping, deletes RowMeta node from Neo4j

### WAL Integration

Every write follows the prescribed sequence:
1. WAL append with status `Pending`, fsync
2. Neo4j transaction (IndexEntry management deferred to M7)
3. WAL status -> `Neo4jCommitted` (commit point)
4. Kvrocks MSET/DEL
5. WAL status -> `Committed`

PK mapping writes happen before the Neo4jCommitted transition so they can be replayed.

### Crash Recovery

- On startup, WAL replay identifies entries in `Neo4jCommitted` state
- `replay_wal_entries()` re-executes Kvrocks writes (MSET for INSERT/UPDATE, DEL for DELETE)
- PK mappings are rebuilt during replay using Neo4j schema lookup (PK column resolution by table_id)
- `Pending` entries are marked `RolledBack`
- Kvrocks writes are idempotent -- safe to replay

### WAL Compaction

- `WalFile::compact(max_age_secs)` removes old terminal entries (Committed/RolledBack)
- Rewrites WAL to temp file, atomic rename
- Re-scans after compaction to fix file offsets

### Kvrocks Operations (executor::kvrocks)

- `mset()`: write multiple key-value pairs atomically
- `get()`: read a single key
- `mget()`: batch read multiple keys
- `del()`: delete multiple keys
- `scan_keys()`: SCAN with pattern match (MATCH + COUNT 1000)
- `exists()`: check key existence

### Query Router Extensions

- INSERT: parses column list and VALUES, converts expressions to bytes via `expr_to_bytes()`
- UPDATE: parses SET assignments and WHERE clause via `parse_simple_where()`
- DELETE: parses WHERE clause, dispatches to WriteEngine
- Expression-to-bytes conversion handles: string literals, numeric literals, booleans, NULL

## Architecture Decisions

- **PK mapping in Kvrocks** (`pk:{db}:{tbl}:{value}` -> `row_id`): avoids Neo4j round-trip for PK lookups, two GET round trips for single-row access
- **Non-PK predicate via SCAN**: M3 does full table scan + filter for non-indexed predicates. Indexes in M7 will make this efficient
- **WAL stores db_id/table_id/row_id, not names**: allows correct replay even if schema is renamed (IDs are immutable)
- **Neo4j lookup during replay**: PK column resolution cached per (db_id, table_id) to minimize queries

## Test Summary

- 41 total tests across workspace (11 keys, 9 types, 13 wal, 8 executor)
- Unit tests: resolve_columns, validate_not_null, build_kv_pairs
- WAL tests: append/scan, replay Neo4jCommitted, replay marks Pending as RolledBack, compaction, entry_id monotonicity

## Acceptance Criteria Verification

| # | Criteria | Result |
|---|---|---|
| 1 | INSERT INTO users VALUES succeeds, Kvrocks keys visible | Pass |
| 2 | UPDATE users SET name WHERE id succeeds | Pass |
| 3 | DELETE FROM users WHERE id succeeds | Pass |
| 4 | Crash recovery: SIGKILL after Neo4j commit, Kvrocks replay on restart | Pass |
| 5 | Duplicate PK returns error | Pass |
| 6 | NULL into NOT NULL column returns error | Pass |

## Testing Steps

```bash
# Build and start
docker compose down -v && docker compose up --build -d

# Wait for ready
curl -sf http://localhost:8080/health

# Create schema
psql -h localhost -p 5432 -U snowdrop -d snowdrop -c "CREATE DATABASE testdb;"
psql -h localhost -p 5432 -U snowdrop -d snowdrop -c "CREATE TABLE testdb.users (id BIGINT PRIMARY KEY, name VARCHAR(255) NOT NULL, email VARCHAR(255));"

# INSERT
psql -h localhost -p 5432 -U snowdrop -d snowdrop -c "INSERT INTO testdb.users (id, name, email) VALUES (1, 'Alice', 'alice@test.com');"

# Verify in Kvrocks
docker compose exec kvrocks redis-cli -p 6666 KEYS '*'

# UPDATE
psql -h localhost -p 5432 -U snowdrop -d snowdrop -c "UPDATE testdb.users SET name = 'Bob' WHERE id = 1;"

# DELETE
psql -h localhost -p 5432 -U snowdrop -d snowdrop -c "DELETE FROM testdb.users WHERE id = 1;"

# Duplicate PK (should error)
psql -h localhost -p 5432 -U snowdrop -d snowdrop -c "INSERT INTO testdb.users (id, name, email) VALUES (1, 'A', 'a@t.com');"
psql -h localhost -p 5432 -U snowdrop -d snowdrop -c "INSERT INTO testdb.users (id, name, email) VALUES (1, 'B', 'b@t.com');"

# NOT NULL violation (should error)
psql -h localhost -p 5432 -U snowdrop -d snowdrop -c "INSERT INTO testdb.users (id, email) VALUES (2, 'no-name@t.com');"

# Crash recovery: kill orchestrator, delete Kvrocks data, patch WAL status to Neo4jCommitted, restart
```
