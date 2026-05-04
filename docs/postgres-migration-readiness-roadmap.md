# PostgreSQL Migration Readiness Roadmap

## Goal

Move Snowdrop from a fast SQL PoC with a useful analytical subset toward a safer foundation for PostgreSQL-to-Snowdrop migration.

This roadmap assumes the near-term priority is:

1. correctness and predictability
2. clear compatibility boundaries
3. migration-readiness of schema and SQL behavior
4. performance only after the above are measurable and testable

## Current Status

- Phase 0 baseline is now implemented in the repository structure:
  - generated artifacts live under `reports/`
  - benchmark guidance is separated from generated output
  - the compatibility matrix exists as a dedicated document
- Phase 1 now has an initial fixed-dataset baseline for:
  - `NULL` semantics
  - grouped ordering
  - join edge cases
  - write-path round-trips

The remaining work for Phase 1 is expansion, not bootstrapping. The foundation now exists and should be grown deliberately.

## Guiding Principles

- Do not expand the SQL surface without expanding correctness coverage.
- Every new compatibility feature should come with fixed-dataset tests.
- Differential testing against PostgreSQL should be the default oracle whenever Snowdrop intentionally claims equivalent behavior.
- Migration features should be staged behind explicit "supported", "partial", and "unsupported" capability reporting.

## Phase 0: Reporting and Documentation Baseline

### Objectives

- Keep generated artifacts out of `docs/`
- Make the current reporting flow unambiguous
- Publish a compatibility source of truth

### Deliverables

- `reports/` becomes the home for generated artifacts
- `docs/benchmarking.md` becomes the benchmark guide
- `docs/sql-compatibility-matrix.md` becomes the feature-status source of truth
- `docs/postgres-migration-readiness-roadmap.md` becomes the delivery roadmap

### Testing Impact

- Add a smoke check that `snowdrop-testkit -- report` writes to `reports/test-report.md`
- Add a smoke check that raw JSON lands in `reports/raw/`

## Phase 1: Compatibility Baseline and Correctness Expansion

### Objectives

- Define exactly which SQL subset Snowdrop promises today
- Expand correctness suites around the existing surface before adding major new syntax

### Functional Focus

- stabilize current `SELECT`, `JOIN`, `GROUP BY`, `ORDER BY`, `LIMIT/OFFSET`
- stabilize write-path semantics already implemented today
- tighten behavior around nulls, sorting, joins, and aggregates

### Required Test Expansion

Add or expand fixed datasets for:

- `null_semantics`
  - `NULL` in filters
  - `NULL` in aggregates
  - `NULL` ordering expectations
- `order_group_cases`
  - grouped output ordering
  - qualified and unqualified `ORDER BY`
  - ties and deterministic sort expectations
- `join_edge_cases`
  - duplicate keys
  - unmatched rows
  - left join null filling
  - skewed key distributions
- `write_path_smoke`
  - insert/update/delete on simple fixtures
  - read-after-write checks

### Suite Coverage Requirements

- `golden`
  - exact result checks for each dataset
- `differential`
  - same queries against Snowdrop and PostgreSQL
- `sqllogictest`
  - compact statement and query coverage for the supported subset
- `postgres_style`
  - human-reviewable regression pairs for the same supported subset

### Exit Criteria

- correctness suite size is materially larger than the benchmark suite
- existing supported features have dataset-backed regression coverage
- the compatibility matrix reflects the tested supported subset, not assumptions

## Phase 2: Scalar Function and Expression Foundation

### Objectives

- Build the minimum scalar expression layer needed for realistic SQL workloads
- Reduce the gap between "basic query works" and "ordinary application SQL works"

### Functional Priorities

Implement and validate:

- null helpers
  - `COALESCE`
  - `NULLIF`
- string helpers
  - `LOWER`
  - `UPPER`
  - `LENGTH`
  - `TRIM`
  - `SUBSTRING`
  - `CONCAT`
- conversion helpers
  - `CAST`
- date/time basics
  - `CURRENT_DATE`
  - `NOW`

### Why This Matters for Migration

Without these functions, even modest PostgreSQL query sets will fail or require heavy rewriting before migration.

### Required Test Coverage

Add fixed datasets and cases for:

- text normalization and trimming
- nullable text and nullable numeric expressions
- date-based predicates
- cast behavior across text, integer, float, and boolean

Coverage style:

- `golden`
  - function-level expected rows
- `differential`
  - PostgreSQL parity for supported function signatures
- `sqllogictest`
  - compact function smoke tests
- negative tests
  - explicit unsupported signatures should fail predictably, not silently miscompute

### Exit Criteria

- a first "practical SQL function pack" is supported
- expression behavior is documented in the compatibility matrix
- all new function claims are backed by differential tests

## Phase 3: Query Surface Needed Before Migration

### Objectives

- Expand Snowdrop beyond the current simple-select subset
- Support the SQL constructs that appear early in real PostgreSQL workloads

### Functional Priorities

Implement in this order:

1. `HAVING`
2. subqueries in safe, narrow forms
3. derived tables
4. `WITH` / simple CTEs
5. `UNION ALL`, then `UNION`
6. `INSERT ... SELECT`
7. broader `ORDER BY` on expressions

### Required Test Coverage

Datasets to add:

- `having_cases`
- `subquery_cases`
- `set_ops_cases`
- `insert_select_cases`

Suites:

- `golden`
  - exact rows and expected errors
- `differential`
  - parity for supported cases only
- `postgres_style`
  - grouped by feature family

Important rule:

- each newly supported feature must also gain an "unsupported boundary" test
- for example, if simple subqueries are supported, nested correlated subqueries should still be explicitly tested as unsupported until implemented

### Exit Criteria

- Snowdrop supports a wider, explicitly documented query subset
- failure modes for still-unsupported queries are deterministic and readable

## Phase 4: Schema Fidelity for Migration

### Objectives

- Make migrated schemas survivable, not just loadable
- Move beyond the current single-PK and basic-nullability model

### Functional Priorities

Implement in stages:

1. better `ALTER TABLE` support
2. `DEFAULT` values
3. user-facing `UNIQUE`
4. composite primary keys
5. `FOREIGN KEY` metadata, then enforcement
6. `CHECK` constraints where practical

### Required Test Coverage

Datasets:

- `schema_constraints`
- `composite_keys`
- `defaults_and_updates`

Cases to cover:

- accepted schema
- rejected schema
- constraint violation behavior
- metadata visibility via introspection paths

Test style:

- `golden`
  - expected success/failure on DDL and DML
- `postgres_style`
  - schema regression pairs
- `tap`
  - schema lifecycle smoke workflows

### Exit Criteria

- Snowdrop can load and preserve a more realistic subset of PostgreSQL schemas
- schema limitations are explicit and reportable

## Phase 5: Type Semantics and Catalog Compatibility

### Objectives

- Reduce semantic mismatches that break clients and migration tooling
- Prepare for introspection-heavy workflows

### Functional Priorities

- improve `DECIMAL` and `NUMERIC` handling so they do not degrade into loose float behavior
- strengthen date/time semantics
- improve `LIKE` behavior and decide whether `ILIKE` belongs in the near-term subset
- expand `information_schema`
- add a minimal `pg_catalog` compatibility layer for common introspection queries

### Required Test Coverage

Datasets:

- `numeric_precision`
- `date_time_semantics`
- `catalog_introspection`

Cases to cover:

- numeric rounding and equality edge cases
- date comparisons and ordering
- introspection queries used by clients and migration tooling

### Exit Criteria

- type behavior is less surprising and better documented
- client and tool introspection friction is reduced

## Phase 6: PostgreSQL Migration Workflow

### Objectives

- Turn compatibility work into an actual migration path

### Functional Priorities

Build a staged migration workflow:

1. PostgreSQL schema introspection
2. Snowdrop compatibility analysis
3. dry-run report with `supported / partial / unsupported`
4. schema translation where safe
5. data load pipeline
6. post-load verification queries

### Recommended Deliverables

- migration preflight command
- schema compatibility report
- unsupported feature inventory
- post-migration validation suite

### Required Test Coverage

Use exported PostgreSQL fixtures for:

- simple OLTP-like schema
- analytics-oriented schema
- schema with unsupported features to validate reporting quality

Validation layers:

- structural validation
  - tables, columns, types, indexes
- data validation
  - row counts
  - sample checksums
  - targeted aggregate comparisons
- query validation
  - replay a supported subset of application queries against both systems

### Exit Criteria

- Snowdrop can explain what it can migrate
- Snowdrop can reject unsupported features clearly
- migration success is measured by repeatable validation, not by anecdotal loading success

## Cross-Cutting Testing Strategy

Across all phases, testing should be expanded along these axes:

### 1. Correctness Before Speed

- every new SQL feature needs `golden` coverage
- every PostgreSQL-equivalent claim needs `differential` coverage
- every bugfix needs a dedicated regression case on a fixed dataset

### 2. Small Reviewable Fixtures First

Prefer many small, readable datasets over one giant synthetic fixture.

Keep TPC-H as a benchmark workload, but do not use it as the main correctness oracle.

### 3. Explicit Unsupported Coverage

Unsupported SQL should be tested too.

Why:

- to confirm Snowdrop fails clearly
- to avoid silent wrong answers
- to make the compatibility matrix trustworthy

### 4. Report Everything in One Place

The unified report should remain the main human-facing artifact.

It should continue to show:

- correctness status
- skipped and unsupported cases
- benchmark summary
- eventually, migration validation results

## Recommended Next Three Iterations

### Iteration 1

- finish the `reports/` cleanup
- expand correctness datasets around nulls, ordering, join edges, and write-path behavior
- keep the compatibility matrix up to date with tested reality

### Iteration 2

- implement the first scalar function pack
- add differential tests for every supported function
- add negative tests for unsupported function calls

### Iteration 3

- implement `HAVING`, narrow subqueries, and `INSERT ... SELECT`
- expand PostgreSQL-style regression coverage around those features
- start drafting the migration preflight format

## Practical Summary

Before Snowdrop grows migration tooling, it should first become:

- more explicit
- more testable
- more correct
- and more honest about its SQL boundary

That foundation is what will make future PostgreSQL migration work credible instead of fragile.
