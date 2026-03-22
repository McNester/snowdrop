# Milestone 1 -- Skeleton and Connectivity

**Status:** Complete

## Deliverables

### Rust Workspace (5 crates)

| Crate | Type | Purpose |
|---|---|---|
| `orchestrator` | binary | pgwire TCP listener, health endpoint, startup sequence |
| `executor` | library | Kvrocks connectivity (future: query execution, filter buffer) |
| `wal` | library | WAL entry types, append, status update, replay, file management |
| `schema` | library | Neo4j connectivity (future: schema graph operations) |
| `keys` | library | base62 encoding/decoding, Kvrocks key construction, scan patterns |

### keys crate

- `encode_base62(value, width)` / `decode_base62(s)` with full roundtrip support
- Key construction: `build_key(db_id, table_id, row_id, col_id)` producing 21-char keys
- Scan patterns: `table_scan_pattern`, `column_scan_pattern`, `row_scan_pattern`, `db_scan_pattern`
- Fixed-width encoding: db_id=2, table_id=3, row_id=11, col_id=2

### WAL engine

- `WalHeader` (entry_id, timestamp, status, payload_len) with binary read/write
- `WalPayload` version-wrapped (`V1`) with Insert/Update/Delete variants
- `WalFile::open()` creates file and parent directories
- `WalFile::append()` writes entries with Pending status
- `WalFile::replay()` returns Neo4jCommitted entries for Kvrocks re-execution, marks Pending as RolledBack
- `WalFile::update_status()` overwrites status byte in place
- Entry IDs are monotonically increasing, recovered correctly on reopen

### Schema crate (Neo4j)

- `Neo4jConnection::connect(uri, user, password)` with `neo4rs` connection pool
- `ping()` executes `RETURN 1 AS ping` and validates response
- `ColumnType` enum with all Phase 1 supported types

### Executor crate (Kvrocks)

- `KvrocksConnection::connect(addr)` with `redis-rs` multiplexed async connection
- `ping()` sends PING, validates PONG

### Orchestrator

- Startup sequence: WAL open -> WAL replay -> Neo4j connect + ping -> Kvrocks connect + ping -> mark ready -> accept connections
- pgwire listener on `:5432` using `pgwire` crate with NoopStartupHandler
- Any SQL query returns empty result (M1 scope -- no execution)
- Health endpoint on `:8080` returns `{"status":"starting"}` (503) during startup, `{"status":"ok"}` (200) when ready
- Structured logging via `tracing` with env-filter support

### Docker

- `docker-compose.yml`: Neo4j 5, Apache Kvrocks, orchestrator with health-based dependency ordering
- `Dockerfile`: multi-stage build (rust:1.83-bookworm builder, debian:bookworm-slim runtime)
- ARM64/Apple Silicon compatible via platform tags

## Test Results

```
26 tests total, 26 passed

keys:    11 tests (base62 encoding, key format, scan patterns)
wal:     13 tests (header roundtrip, append/read, status update, replay logic, file management)
schema:   2 tests (type classification)
```

## Acceptance Criteria Status

| # | Criterion | Status |
|---|---|---|
| 1 | psql connects and shows prompt | Ready (requires Docker) |
| 2 | `\q` exits cleanly | Ready (requires Docker) |
| 3 | Logs confirm Neo4j and Kvrocks connectivity | Implemented |
| 4 | Any SQL returns empty result | Implemented |
| 5 | `curl localhost:8080/health` returns ok | Implemented |
| 6 | `docker-compose up` starts all services | Ready |
| 7 | Orchestrator restart reconnects | Implemented |

## How to Test

```bash
# Run unit tests
cargo test

# Start all services
docker-compose up --build

# In another terminal:
psql -h localhost -U anyuser -d anydb

# Health check:
curl localhost:8080/health

# Shut down:
docker-compose down
```

## Architecture Decisions

1. **Used debian:bookworm-slim instead of distroless** -- `neo4rs` requires OpenSSL/TLS at runtime which distroless does not include. bookworm-slim adds only `ca-certificates`.

2. **WAL header is 21 bytes on disk** (not 17 as in the spec) because `payload_len` is stored as 4 bytes after the 17-byte fixed portion. The spec counts 17 bytes for the "fixed" header fields (entry_id:8 + timestamp:8 + status:1) but the length field follows immediately.

3. **pgwire 0.28 uses `PgWireServerHandlers` trait** (not `PgWireHandlerFactory`), which includes an `ErrorHandler` associated type. We use `NoopErrorHandler` for M1.
