# Snowdrop Benchmark Results

This document is the compact benchmark snapshot for presentations, PR review, and thesis updates. It is generated from the latest `tpch-bench` JSON artifact so old historical numbers do not get mixed with the current run.

## Last Benchmark Run

| Field | Value |
|---|---|
| Timestamp | `2026-05-25T20:22:21.025649480+00:00` |
| Git branch | `phase6-postgresql-migration-workflow` |
| Git commit | `44a8fa3` |
| Host OS / arch | `linux` / `x86_64` |
| CPU count | `12` |
| Container mode | `docker` |
| Docker version | `not detected` |
| Memory limit | `unlimited / unknown` |
| Endpoints | Snowdrop `snowdrop-1-orchestrator-1:5432` / PostgreSQL `snowdrop-1-postgres-1:5432` |
| Scale / seed | `1` / `42` |
| Runs / warmup | `10` measured / `2` warmup |
| Preparation | Snowdrop indexes `true` / PostgreSQL indexes `true` / PostgreSQL ANALYZE `true` |
| Load batch size | `1000` rows per INSERT statement |
| Output artifact | `reports/benchmarks/tpch-results.json` |

## Current TPC-H Snapshot

Headline values are measured-run medians; warmup runs are excluded. Compare Snowdrop and PostgreSQL only inside the same Docker run because absolute milliseconds vary by host CPU, Docker backend, filesystem, and thermal state.

| Query | Snowdrop baseline median | Snowdrop indexed median | PostgreSQL median | Snowdrop result | p90 / stddev signal |
|---|---:|---:|---:|---|---|
| Q6 | `0.79ms` | `0.74ms` | `1.15ms` | Snowdrop is `1.57x faster` | SD p90 `0.99ms` / stddev `0.15ms`; PG p90 `1.25ms` / stddev `0.10ms` |
| Q1 | `1.57ms` | `1.55ms` | `3.91ms` | Snowdrop is `2.52x faster` | SD p90 `1.94ms` / stddev `0.26ms`; PG p90 `4.26ms` / stddev `0.38ms` |
| Q14 | `0.73ms` | `0.71ms` | `0.88ms` | Snowdrop is `1.24x faster` | SD p90 `0.73ms` / stddev `0.06ms`; PG p90 `0.93ms` / stddev `0.11ms` |
| Q3 | `1.06ms` | `1.05ms` | `1.62ms` | Snowdrop is `1.55x faster` | SD p90 `1.22ms` / stddev `0.22ms`; PG p90 `1.67ms` / stddev `0.13ms` |

## Query Meaning

- `Q6`: scan-heavy filtered aggregate; currently accelerated by a narrow TPC-H columnar fast path.
- `Q1`: grouped multi-aggregate over one large table; currently accelerated by a narrow TPC-H columnar fast path, with reusable aggregate planning still pending.
- `Q14`: two-table join with CASE/filter; currently accelerated by a narrow TPC-H fast path, with reusable hash join and pruning still pending.
- `Q3`: three-table join with GROUP BY/ORDER/LIMIT; currently accelerated by a narrow TPC-H fast path, with reusable join ordering still pending.

## Historical Snapshot

The old warm-cache snapshot below is retained as historical context only. New claims should use the generated `Last Benchmark Run` block above.

| Query | Snowdrop (cold) | Snowdrop (warm) | PostgreSQL 16 | Warm vs PG |
|---|---:|---:|---:|---|
| Q6 (SUM + date range) | 8.2ms | **0.8ms** | 1.5ms | **1.9x faster** |
| Q1 (GROUP BY + 6 aggregates) | 15.9ms | **10.9ms** | 4.6ms | 2.4x slower |
| Q14 (2-table JOIN + CASE) | 42.2ms | **26.1ms** | 1.5ms | 17x slower |
| Q3 (3-table JOIN + GROUP BY) | 62.6ms | **44.3ms** | 1.6ms | 28x slower |

## Interpretation

- Snowdrop performance should be described as workload-specific, not as a universal PostgreSQL replacement claim.
- The stable headline is the same-run ratio and median latency, not raw milliseconds copied between laptops.
- Current acceleration work should proceed in this order: stabilize migration/load, reduce write/DDL round-trips, then generalize the Q1/Q14/Q3 fast paths into reusable planner rules.

## Write/DDL Companion Result

The current write/DDL baseline is tracked separately because setup/load time can dominate benchmark usability before query execution starts. See [write-ddl-results.md](/e:/snowdrop-1/reports/benchmarks/write-ddl-results.md:1) for the latest `CREATE TABLE`, `ALTER TABLE`, single insert, row-by-row insert, and multi-values insert numbers.

## Refresh Command

Use the Docker-only command in [benchmarking.md](/e:/snowdrop-1/docs/benchmarking.md:1) and include `--markdown-output docs/benchmark-results.md`, or regenerate from an existing JSON artifact with `snowdrop-testkit benchmark-report`.
