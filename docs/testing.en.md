# Snowdrop Testing Guide

## Overview

Snowdrop now uses a dedicated testing foundation built around:

- fixed SQL datasets in `tests/datasets/`
- suite definitions in `tests/golden/`, `tests/differential/`, `tests/sqllogictest/`, `tests/tap/`, and `tests/postgres_style/`
- a Docker-oriented runner crate: `snowdrop-testkit`
- one unified generated report: `docs/test-report.md`

This v1 testing system is intentionally Docker-first. Start the services with `docker compose up -d`, then run the harness commands from the repository root.

When a suite needs PostgreSQL access, the runner now reads the password from `PGPASSWORD` first and falls back to `POSTGRES_PASSWORD`.

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
- Produces TAP-like output files under `target/snowdrop-testkit/tap/`

### PostgreSQL-style subset

- Uses `sql/` and `expected/` pairs under `tests/postgres_style/`
- Mimics PostgreSQL regression organization
- Does not attempt literal upstream PostgreSQL compatibility

## Commands

Canonical workflow:

```powershell
docker compose up -d
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

## Report

The main human-facing artifact is:

- `docs/test-report.md`

It includes:

- environment metadata
- dataset inventory
- per-dataset suite summary
- case tables for golden, differential, sqllogictest, TAP-style, and PostgreSQL-style suites
- skipped/unsupported cases
- TPC-H benchmark summary when `--with-tpch` is used

The raw machine-readable run artifact is stored at:

- `target/snowdrop-testkit/results.json`

## Status Meanings

- `pass`: behavior matched the expectation
- `fail`: query or workflow failed unexpectedly
- `mismatch`: query succeeded but returned unexpected results
- `skip`: intentionally not counted as a failure, usually because Snowdrop does not support that behavior yet

## Limitations

- Docker-only runtime in v1
- no literal execution of upstream PostgreSQL regression or TAP files
- `sqllogictest` support is intentionally partial
- large production-like datasets are out of scope for this first testing foundation
