# Snowdrop Performance Acceleration Roadmap

This roadmap turns benchmark work into an engineering loop:

1. capture baseline
2. state hypothesis
3. implement one optimization
4. rerun Docker-only benchmark
5. update [benchmark-results.md](/e:/snowdrop-1/docs/benchmark-results.md:1)

The headline rule is simple: compare Snowdrop and PostgreSQL on the same machine in the same Docker workflow. Raw milliseconds across laptops are not stable enough to use as product claims.

## Priority 1: Benchmark Truth

Status: implemented foundation.

- `tpch-bench` writes `schema_version: 2` JSON with environment metadata, warmup runs, raw latencies, median, p90, min, max, average, and stddev.
- `tpch-bench --markdown-output docs/benchmark-results.md` can update the human-facing benchmark snapshot.
- `snowdrop-testkit benchmark-report` can regenerate the markdown snapshot from an existing JSON artifact.
- Benchmark docs now require Docker-only commands to avoid Windows/MSVC and host-toolchain variance.

Next acceptance:

- Get a completed TPC-H `schema_version: 2` artifact after the write/DDL bottleneck is reduced.
- Keep historical numbers separate from the generated last-run snapshot.

## Priority 2: Inserts and DDL

Status: second acceleration pass implemented.

Pre-optimization Docker baseline from `2026-05-20`:

| Measurement | Latency | Signal |
|---|---:|---|
| `CREATE DATABASE` | `466.59ms` | acceptable but worth tracking |
| `CREATE TABLE` | `7079.74ms` | blocking |
| `ALTER TABLE ADD COLUMN` | `13811.00ms` | blocking |
| single-row `INSERT` | `1010.10ms` | blocking |
| 100 row insert loop | `43.5 rows/sec` | too slow for migration/demo load |

Warm Docker result from `2026-05-21` after the first DDL/cache pass:

| Measurement | Latency | Signal |
|---|---:|---|
| `CREATE DATABASE` | `97.00ms` | acceptable |
| `CREATE TABLE` | `190.13ms` | no longer multi-second |
| `ALTER TABLE ADD COLUMN` | `92.63ms` | no longer multi-second |
| single-row `INSERT` | `40.46ms` | improved, still not PostgreSQL-class |
| 100 row row-by-row insert loop | `43.2 rows/sec` | still bottlenecked |
| 100 row multi-values insert | `896.6 rows/sec` | preferred migration/load shape |

Current warm Docker result from `2026-05-23` after adding cached batch execution for
one `INSERT ... VALUES (...), (...), ...` statement:

| Measurement | Latency | Signal |
|---|---:|---|
| `CREATE DATABASE` | `95.23ms` | acceptable |
| `CREATE TABLE` | `237.97ms` | acceptable when warm, still Neo4j-sensitive |
| `ALTER TABLE ADD COLUMN` | `186.65ms` | acceptable when warm, still Neo4j-sensitive |
| single-row `INSERT` | `59.43ms` | still too slow for app-style write latency |
| 100 row row-by-row insert loop | `14.3 rows/sec` | unacceptable for migration/demo load |
| 100 row multi-values insert | `680.2 rows/sec` | 47x faster than row-by-row in this run |

Optimization hypotheses:

- Batch Neo4j schema writes for DDL where safe. First pass implemented batched `CREATE TABLE` column creation.
- Keep Neo4j as catalog/source-of-truth but use hot embedded schema snapshots for runtime checks.
- Avoid repeated schema and constraint lookups per inserted row during trusted migration load. First pass implemented cached insert planning inside one multi-values statement; second pass batches row/index storage writes for multi-values statements.
- Add a trusted bulk-load path for generated fixtures and `migration-auto`.
- Keep per-row logs/debug disabled by default.

Acceptance target:

- `CREATE TABLE` should not feel like multi-second work on local Docker. First pass reached ~190ms on the warm run.
- Single-row insert should stop being close to one second. First pass reached ~40ms on the warm run.
- Migration fixture load should be materially faster and predictable enough for demos.

Remaining write-path work:

- Stop using row-by-row SQL loops in benchmark/demo setup where multi-values batches are available.
- Add trusted migration bulk-load mode that can skip repeated per-row constraint lookups when preflight already proved the table shape.
- Investigate why Neo4j/DDL latency still swings heavily between warm and cold runs.
- Measure FK-heavy fixture load after cached batch insert execution.

## Priority 3: TPC-H Query Acceleration

Run order:

1. `Q6`: stabilize the strongest scan-heavy path, verify warmup/cache lifecycle, and reduce variance. Status: implemented range-index guard and a narrow TPC-H Q6 columnar fast path.
2. `Q1`: optimize grouped multi-aggregate over one scan; compute multiple aggregates in one tight pass. Status: implemented TPC-H Q1 columnar fast path.
3. `Q14`: implement/strengthen hash join for supported equi-join shape, needed-column pruning, and safe filter pushdown. Status: implemented a narrow TPC-H Q14 columnar fast path and tightened the promo-part lookup to avoid per-row hash/string allocation overhead. Broader reusable join optimization remains follow-up work.
4. `Q3`: add simple join ordering, early date/status filtering, and cheaper GROUP BY + ORDER BY + LIMIT after joins. Status: implemented a narrow TPC-H Q3 columnar fast path while leaving the broader reusable join optimizer as follow-up work.

Current Docker TPC-H snapshot from `2026-05-25`:

| Query | Snowdrop median | PostgreSQL median | Signal |
|---|---:|---:|---|
| `Q6` | `0.75ms` baseline / `0.87ms` indexed | `0.93ms` | baseline Snowdrop 1.24x faster through the TPC-H-specific fast path |
| `Q1` | `1.62ms` baseline / `1.97ms` indexed | `3.75ms` | baseline Snowdrop 2.31x faster through the TPC-H-specific fast path |
| `Q14` | `0.80ms` baseline / `1.34ms` indexed | `0.74ms` | baseline Snowdrop 1.09x slower; Q14 remains a reusable join/hash-path follow-up |
| `Q3` | `1.05ms` baseline / `1.28ms` indexed | `1.75ms` | baseline Snowdrop 1.66x faster through the TPC-H-specific fast path |

TPC-H load improved materially after the INSERT fast path and corrected
`lineitem` batching, then improved again after batch WAL entries and batched FK
lookup preparation: the latest run loaded Snowdrop data in `7.1s` versus
PostgreSQL in `0.2s`. The query path is faster than PostgreSQL on `Q6`, `Q1`,
and `Q3` in the latest Docker run; `Q14` is still slightly slower and should
stay visible as the next join-path optimization target. Load/setup remains the
primary product risk. The next pass should focus on trusted bulk load,
constraint/index staging, larger storage batches, WAL batching measurement, and
Neo4j-sensitive setup.

After each query slice:

- run the Docker TPC-H command from [benchmarking.md](/e:/snowdrop-1/docs/benchmarking.md:1)
- compare median, p90, and stddev with the previous JSON artifact
- update [benchmark-results.md](/e:/snowdrop-1/docs/benchmark-results.md:1)
- explicitly state whether Snowdrop is faster or slower and why

## Priority 4: Expensive Table Matrix

TPC-H is not enough to find the best product use case. Add a separate suite for wider and more realistic schemas:

- wide table scan: 20-40 columns, selective predicates, projected few columns
- large fact table aggregate: high row count, low-cardinality grouping
- mixed schema query: fact table plus dimension joins
- point/range lookup with and without indexes

Each benchmark case must record:

- row count
- column count
- selected column count
- predicate selectivity
- cache warmth
- Snowdrop path: index, columnar, or relation

## Product Positioning Guardrails

Until this roadmap is further along, Snowdrop should not be presented as a broad PostgreSQL replacement.

Safe positioning:

- supported read-heavy / scan-heavy workloads
- migration-safe subset
- acceleration candidate for specific query patterns
- PostgreSQL protocol compatibility for easier adoption

Unsafe positioning:

- universal PostgreSQL replacement
- guaranteed speedup for arbitrary SQL
- mature PostgreSQL optimizer parity
- production-ready migration for any schema

## Product Use-Case Direction

The current research signal from code, benchmarks, and team profiling is that
Snowdrop should be positioned as a workload-specific accelerator, not as a
general PostgreSQL replacement. A large PostgreSQL database can still return in
around 1ms when the query is selective, data is hot, indexes/statistics are
right, and PostgreSQL chooses a good plan. “Big database” is therefore not a
business argument by itself; the query shape matters more than raw row count.

Good candidates:

- append-heavy fact tables such as audit/activity/event streams
- filtered aggregate dashboards over large rows sets
- wide tables where queries project only a few columns
- operational analytics where scan/aggregate latency dominates the endpoint
- safe PostgreSQL subset migrations where Snowdrop can own the read path

Weak candidates today:

- OLTP point lookups already backed by mature PostgreSQL indexes
- join-heavy normalized reporting where PostgreSQL’s optimizer wins
- write-heavy applications that require sub-millisecond insert/DDL latency
- schemas depending on broad PostgreSQL planner/catalog/extension behavior

Near-term product demo guidance:

- Keep `benchmark_queries.json` as the honest join-heavy gap suite.
- Use `benchmark_queries_snowdrop_favorable.json` for the Snowdrop-favorable demo suite.
- Report both faster and slower cases; do not hide PostgreSQL wins.
- Treat Neo4j as schema source-of-truth for now, but keep moving runtime hot paths toward cached metadata and fewer cross-service calls.
