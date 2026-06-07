# Hybrid Embedded Engine Roadmap

Snowdrop's next architecture target is a PostgreSQL-compatible read accelerator,
not a broad PostgreSQL replacement. The goal is to remove network/control-plane
bottlenecks and the current query-shape cliff while preserving the existing
pgwire surface and the safe migration subset.

## Target Shape

```text
pgwire / PostgreSQL-compatible surface
  -> parser + planner + workload metrics
  -> embedded catalog
  -> vectorized execution layer
  -> RocksDB row-delta/index store
  -> persisted columnar read store
```

RocksDB remains the durable row/delta/index substrate. Neo4j is demoted from
required runtime catalog to legacy/mirror tooling. The in-memory columnar cache
evolves into persisted immutable segments with row-delta overlay. SELECT-heavy
workloads move toward Arrow/DataFusion-style vectorized execution.

## Phase A: Architecture Foundation

Status: started.

- Added `CatalogStore` plus `SQLiteCatalogStore` foundation in the `schema`
  crate.
- Added `SNOWDROP_CATALOG_BACKEND=neo4j|sqlite|mirror` as the catalog rollout
  contract.
- Added `snowdrop-testkit catalog-init --path ...` to initialize the embedded
  catalog without requiring a running Snowdrop server.
- Added `QueryStageTimings` so query and migration reports can attribute time to
  catalog, parse, planning, execution, WAL, storage, columnar, and pgwire stages.
- Added a persisted columnar segment manifest/store foundation for future
  immutable segment builds.
- Wired the embedded catalog into the live pgwire `QueryRouter` startup path.
  `SHOW DATABASES`, `SHOW TABLES`, `DESCRIBE`, cached database/table/column
  lookups, and cached table constraints can now read through `CatalogStore`.
- `CREATE/DROP DATABASE` and `CREATE/DROP TABLE` now use SQLite as the
  authoritative catalog when `SNOWDROP_CATALOG_BACKEND=sqlite`; `mirror` mode
  keeps Neo4j authoritative and mirrors core metadata into SQLite.
- Supported schema-evolution metadata now has SQLite catalog implementations:
  `ALTER TABLE ADD COLUMN`, `DROP COLUMN`, `RENAME COLUMN`,
  `ALTER COLUMN SET/DROP DEFAULT`, `ALTER COLUMN SET/DROP NOT NULL`, and
  `CREATE INDEX` metadata.
- SQLite catalog mode can now warm an empty or populated embedded catalog without
  a Neo4j connection. If legacy WAL replay is pending, startup still blocks
  because the current replay path needs the legacy schema connection.
- `information_schema` handlers and the table-backed `pg_catalog` relation
  builders now use `CatalogStore`/schema-cache-aware metadata instead of calling
  Neo4j-specific introspection helpers directly.
- Plan-based `INSERT ... VALUES` now has a Neo4j-free execution path. In SQLite
  catalog mode, `QueryRouter` resolves the `InsertPlan` from `CatalogStore` and
  uses `WriteEngine::new_plan_only(...)`, so batched INSERT can write WAL,
  constraint keys, rows, and index entries without taking a legacy Neo4j
  dependency.
- Non-key `UPDATE` now has the same first embedded-catalog path. In SQLite
  catalog mode, `QueryRouter` resolves table metadata from `CatalogStore` and
  calls `update_with_metadata(...)` through `WriteEngine::new_plan_only(...)`
  for updates that do not need legacy cross-table referential checks.
- Key-changing `UPDATE` now also has a SQLite catalog path for the supported FK
  subset. `QueryRouter` resolves referenced FK targets and referencing-table
  checks from `CatalogStore`, so parent PK updates and child FK updates can be
  validated without calling Neo4j.
- `DELETE` now has the first embedded-catalog path as well. SQLite catalog mode
  resolves table metadata and referencing foreign-key checks from `CatalogStore`,
  then calls `delete_with_metadata(...)` without requiring a Neo4j connection.
- WAL storage replay now has an embedded-catalog metadata path. Startup in
  SQLite catalog mode can replay `Neo4jCommitted` storage mutations using
  `CatalogStore` metadata instead of requiring a Neo4j connection.

Important boundary: some unsupported DDL and catalog-level recovery semantics
still require the legacy path. SQLite catalog mode now owns basic database/table
lifecycle metadata plus the first plan-based INSERT, UPDATE, DELETE, and storage
WAL replay paths, but Snowdrop is not yet fully Neo4j-free.

### SQLite Catalog Docker Smoke Workflow

The embedded-catalog runtime can be started without Neo4j through the dedicated
compose file:

```powershell
docker compose -f docker-compose.sqlite.yml up -d --build orchestrator-sqlite
```

It exposes Snowdrop on host port `15432` by default and persists the local
catalog at `/data/catalog/catalog.sqlite` inside the container volume. A minimal
smoke check:

```powershell
docker run --rm --platform linux/amd64 --network snowdrop-1_default -e PGPASSWORD=postgres postgres:16 psql -h snowdrop-1-orchestrator-sqlite-1 -p 5432 -U postgres -d postgres -c "CREATE DATABASE sqlite_smoke;"
docker run --rm --platform linux/amd64 --network snowdrop-1_default -e PGPASSWORD=postgres postgres:16 psql -h snowdrop-1-orchestrator-sqlite-1 -p 5432 -U postgres -d sqlite_smoke -c "CREATE TABLE users (id INT PRIMARY KEY, email VARCHAR(120) UNIQUE, role VARCHAR(30) DEFAULT 'user'); INSERT INTO users (id, email) VALUES (1, 'demo@snowdrop.local'); SELECT id, email, role FROM users ORDER BY id;"
```

This workflow is intentionally separate from the legacy `docker-compose.yml`
path. The old path remains the compatibility baseline; the SQLite path is the
measuring ground for removing Neo4j from hot runtime and DML paths.

### Measured Migration Signal

The Test Flight fixture was migrated through both the legacy Neo4j-backed
runtime and the new SQLite catalog runtime. The source fixture is the same safe
8-table demo database; timings below are from the generated migration workflow
artifacts and focus on the Snowdrop insert stage.

| Runtime path | Tables | Rows verified | Snowdrop insert stage | Total table load time | Result |
|---|---:|---:|---:|---:|---|
| Neo4j catalog runtime | 8 | 80,680 | `123.4s` | `127.1s` | baseline |
| SQLite catalog runtime | 8 | 80,680 | `2.32s` | `5.74s` | `~53x faster insert stage` |

This is strong evidence that moving catalog-sensitive DML and migration load
off the Neo4j runtime path is the correct architecture direction. The remaining
time in the SQLite run is now mostly PostgreSQL export and SQL literal build,
not Snowdrop catalog round-trips. The next migration-load optimization should
therefore target binary/bulk input and less expensive SQL literal construction,
not another Neo4j workaround.

## Phase B: Catalog Migration

Move schema operations behind `CatalogStore` one group at a time:

- startup/init schema: SQLite mode can start without Neo4j when WAL replay is
  empty
- database/table/column lookup: partially wired
- DDL writes: database/table lifecycle wired for SQLite catalog mode
- constraints and indexes: supported column/default/nullability/index metadata
  mutations wired for SQLite catalog mode
- information_schema / pg_catalog introspection: partially wired
- plan-based INSERT VALUES: wired for SQLite catalog mode
- non-key UPDATE: wired for SQLite catalog mode
- key-changing UPDATE: wired for SQLite catalog mode with catalog-derived FK
  target and referencing-table checks
- write replay: storage replay wired for SQLite catalog mode; broader
  catalog-level recovery semantics still need a dedicated pass
- DELETE: wired for SQLite catalog mode with catalog-derived referencing FK
  checks
- migration artifact recording

Acceptance:

- Snowdrop starts without Neo4j when `SNOWDROP_CATALOG_BACKEND=sqlite` and no
  legacy WAL replay is pending.
- Existing schema and migration tests pass in both `neo4j` and `sqlite` modes.
- `mirror` mode can copy SQLite catalog changes to Neo4j for graph/debug tooling
  without blocking query execution.

## Phase C: Persisted Columnar Store

Replace whole-table cache invalidation with durable segment freshness:

- build immutable segment manifests and data files from RocksDB snapshots
- record min/max/null-count statistics for zone-map pruning
- keep fresh writes in the row-delta layer
- read from segments plus row-delta overlay
- compact segments in the background

Acceptance:

- mixed read/write workloads no longer rebuild the full table after every write
- Test Flight read-heavy actions keep warm-read latency after bookings/cancels
- segment metadata is crash-safe and schema-version aware

## Phase D: Vectorized Execution

Integrate Arrow/DataFusion as the supported read-query execution core:

- table provider over persisted segments and delta overlay
- projection pruning
- filter pushdown
- grouped aggregate execution
- supported equi-hash joins
- simple join ordering for two- and three-table plans

The current executor remains fallback for unsupported SQL, DDL, and write paths.

Acceptance:

- TPC-H Q6/Q1/Q14/Q3 no longer depend on narrow hardcoded fast paths
- Test Flight search/dashboard queries run through the reusable vectorized path
- benchmark reports show execution path and stage timings

## Phase E: Write And Migration Load

Keep write correctness first, then reduce overhead:

- batch row, index, and constraint writes through RocksDB write batches
- reduce FK/UNIQUE validation to grouped local lookups
- keep trusted migration bulk-load internal and gated by green preflight
- measure Snowdrop WAL plus RocksDB WAL double-write cost before simplifying
  durability

Acceptance:

- migration fixture load is at least 2x faster than the current baseline
- booking/cancel write latency is no longer dominated by catalog/schema round
  trips
- WAL replay tests pass before and after any durability boundary change

## Product Positioning

Good candidates:

- event streams and audit/activity tables
- wide fact tables with selective filters
- dashboards over aggregates
- read-heavy operational analytics inside the supported SQL subset

Weak candidates:

- write-heavy OLTP needing sub-millisecond inserts/updates
- PostgreSQL-optimized point lookups where PostgreSQL is already ~1ms
- heavily normalized join reporting outside the supported join subset
- workloads requiring broad PostgreSQL planner, extension, trigger, or catalog
  parity
