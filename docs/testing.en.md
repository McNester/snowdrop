# Snowdrop Testing Guide

## Overview

Snowdrop now uses a dedicated testing foundation built around:

- fixed SQL datasets in `tests/datasets/`
- suite definitions in `tests/golden/`, `tests/differential/`, `tests/sqllogictest/`, `tests/tap/`, and `tests/postgres_style/`
- a Docker-oriented runner crate: `snowdrop-testkit`
- one unified generated report: `reports/test-report.md`

This testing system is intentionally Docker-first. After query-engine or SQL-surface changes, rebuild and restart Snowdrop with `docker compose up -d --build orchestrator`, then run the harness commands from the repository root.

When a suite needs PostgreSQL access, the runner reads the password from `PGPASSWORD` first and falls back to `POSTGRES_PASSWORD`.

## Folder Layout

```text
tests/
  datasets/
    <dataset>/
      manifest.json
      schema.sql
      seed.sql
  golden/
  differential/
  sqllogictest/
  tap/
  postgres_style/

reports/
  test-report.md
  raw/
    snowdrop-testkit-results.json
```

## Dataset Format

Each dataset lives in `tests/datasets/<dataset>/` and contains:

- `manifest.json`: dataset metadata, target database name, drop order, supported suites
- `schema.sql`: DDL for the dataset
- `seed.sql`: fixed DML inserts

Rules:

- use readable, reviewable SQL fixtures
- keep datasets intentionally small in v1
- prefer unqualified table names inside fixture SQL
- treat these files as the shared source of truth for Snowdrop and PostgreSQL differential runs

## Supported Suites

### Golden

- Runs predefined SQL against Snowdrop on a named fixed dataset
- Compares exact expected rows or expected errors
- Primary correctness suite

### Differential

- Runs the same SQL against Snowdrop and PostgreSQL
- Compares normalized result sets
- Marks unsupported Snowdrop behavior as `skip` when the Snowdrop error is clearly about unsupported or unimplemented behavior

### SQLLogicTest

- Uses a focused subset of `.test` files
- Supports:
  - `statement ok`
  - `statement error`
  - `query ... ----`
- Requires a `# dataset: <name>` header in each file

### TAP-style

- Covers readiness and integration-style smoke checks
- Uses JSON suite definitions in `tests/tap/`
- Produces TAP-like output files under `reports/tap/`

### PostgreSQL-style subset

- Uses `sql/` and `expected/` pairs under `tests/postgres_style/`
- Mimics PostgreSQL regression organization
- Does not attempt literal upstream PostgreSQL compatibility

## Commands

Canonical workflow:

```powershell
docker compose up -d --build orchestrator
$env:PGPASSWORD = "<your-postgres-password>"
cargo run -p snowdrop-testkit -- list-datasets
cargo run -p snowdrop-testkit -- run-all --with-tpch
```

Other commands:

```powershell
cargo run -p snowdrop-testkit -- load --dataset tiny_smoke
cargo run -p snowdrop-testkit -- run --dataset tiny_smoke --suite golden
cargo run -p snowdrop-testkit -- run --dataset edge_cases --suite differential
cargo run -p snowdrop-testkit -- run --suite sqllogictest
cargo run -p snowdrop-testkit -- run --suite tap
cargo run -p snowdrop-testkit -- run --suite postgres-style
cargo run -p snowdrop-testkit -- report
```

Notes:

- `golden`, `sqllogictest`, and `tap` suites can run without PostgreSQL credentials.
- `differential`, `postgres-style`, and `run-all --with-tpch` need `PGPASSWORD` or `POSTGRES_PASSWORD` to be set before launch.
- `run` and `run-all` perform a small Phase 3 runtime preflight. If the harness reports an outdated Snowdrop runtime, rebuild the orchestrator and rerun. Bypass this guard only intentionally with `SNOWDROP_SKIP_RUNTIME_PROBE=1`.

## Generated Artifacts

The main human-facing artifact is:

- `reports/test-report.md`

It includes:

- environment metadata
- dataset inventory
- a short Phase 3 feature summary showing supported, partial, and explicitly unsupported families
- per-dataset suite summary
- case tables for golden, differential, sqllogictest, TAP-style, and PostgreSQL-style suites
- skipped or unsupported cases
- TPC-H benchmark summary when `--with-tpch` is used

The raw machine-readable run artifact is stored at:

- `reports/raw/snowdrop-testkit-results.json`

## Status Meanings

- `pass`: behavior matched the expectation
- `fail`: query or workflow failed unexpectedly
- `mismatch`: query succeeded but returned unexpected results
- `skip`: intentionally not counted as a failure, usually because Snowdrop does not support that behavior yet

## Related Docs

- [benchmarking.md](/e:/snowdrop-1/docs/benchmarking.md:1)
- [sql-compatibility-matrix.md](/e:/snowdrop-1/docs/sql-compatibility-matrix.md:1)
- [postgres-migration-readiness-roadmap.md](/e:/snowdrop-1/docs/postgres-migration-readiness-roadmap.md:1)
- [reports/README.md](/e:/snowdrop-1/reports/README.md:1)

## Limitations

- Docker-only runtime in v1
- no literal execution of upstream PostgreSQL regression or TAP files
- `sqllogictest` support is intentionally partial
- large production-like datasets are out of scope for this first testing foundation
