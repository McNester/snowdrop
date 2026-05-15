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
- Phase 2 now has an initial scalar-function baseline for:
  - null helpers like `COALESCE` and `NULLIF`
  - text helpers like `LOWER`, `UPPER`, `LENGTH`, `TRIM`, `SUBSTRING`, and `CONCAT`
  - `CAST`
  - basic date/time helpers like `CURRENT_DATE` and `NOW`
- Phase 3 now has a broad relation-pipeline-driven subset:
  - the top-level `SELECT` entrypoint now dispatches between the shared relation executor and the optimized simple-table fast path, depending on query shape
  - `HAVING` runs on the shared grouped relation path
  - non-correlated scalar, `IN`, and `EXISTS` subqueries are supported
  - correlated `EXISTS`, correlated `IN`, and correlated scalar subqueries in `WHERE` / `HAVING` are supported in the current narrow subset
  - derived tables are supported with explicit alias requirements
  - non-recursive `WITH` / simple CTE chains are supported
  - `UNION ALL` and `UNION` are supported
  - `INSERT ... SELECT` now accepts the supported Phase 3 query-source forms and rejects count/type mismatch up front
  - relation-level `ORDER BY` works over subqueries, derived tables, CTEs, and compound results
  - recursive CTE, `LATERAL`, and compound-set support started Phase 3 as explicit boundaries before they moved into the narrower Phase 3.5 implementation slices
- Phase 3.5 is now implemented as a narrow advanced-query compatibility layer:
  - a narrow core window-function subset now runs on the relation pipeline
  - a narrow `JOIN LATERAL` subset now works over relation-backed derived subqueries
  - a narrow recursive CTE subset now supports `seed UNION ALL recursive_term` hierarchy walks with a deterministic iteration cap
  - distinct `INTERSECT` and `EXCEPT` now run through the relation-level compound-query pipeline
  - supported so far:
    - `ROW_NUMBER`, `RANK`, `DENSE_RANK`, `LAG`, `LEAD`, and partition aggregate windows (`COUNT`, `SUM`, `AVG`, `MIN`, `MAX`)
    - `INNER JOIN LATERAL (SELECT ...) alias ON ...`
    - `LEFT JOIN LATERAL (SELECT ...) alias ON ...`
    - one recursive CTE at a time using `UNION ALL`
    - distinct `INTERSECT`
    - distinct `EXCEPT`
    - distinct `INTERSECT`
    - distinct `EXCEPT`
  - remaining advanced-query gaps are now narrower variants beyond the current narrow window / lateral / recursive slices

The remaining work for Phase 1 and Phase 3 is expansion, not bootstrapping. The foundation now exists and should be grown deliberately.

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

### Implementation Snapshot

The repository now has a broad Phase 3 relation-pipeline slice:

- implemented in the current practical subset
  - the top-level `SELECT` entrypoint now uses the shared relation executor for Phase 3 shapes while preserving the optimized simple-table path for performance-sensitive legacy shapes
  - grouped `HAVING`
  - non-correlated scalar / `IN` / `EXISTS` subqueries
  - correlated `EXISTS`, correlated `IN`, and correlated scalar subqueries in `WHERE` / `HAVING`
  - derived tables in `FROM` / `JOIN`
  - non-recursive `WITH` / simple CTE chains
  - `UNION ALL`
  - `UNION`
  - broader `INSERT ... SELECT` with explicit count/type mismatch validation
  - broader relation-level `ORDER BY`
- still explicitly unsupported
  - recursive CTE beyond the current narrow Phase 3.5 subset
  - `INTERSECT ALL`
  - `EXCEPT ALL`
  - `LATERAL` beyond the current narrow Phase 3.5 subset
  - window functions beyond the current narrow Phase 3.5 subset
- still intentionally partial
  - grouped expressions beyond the current narrow identifier-based subset
  - `INSERT ... SELECT` execution is still materialized, not streaming
  - subqueries nested inside arbitrary scalar function arguments

### Required Test Coverage

Datasets to add or expand:

- `having_cases`
- `subquery_cases`
- `correlated_subquery_cases`
- `derived_table_cases`
- `cte_cases`
- `set_ops_cases`
- `insert_select_cases`

Suites:

- `golden`
  - exact rows and expected errors
- `differential`
  - parity for supported cases only
- `sqllogictest`
  - compact positive and negative coverage for each feature family
- `tap`
  - smoke workflows for `WITH`, derived-table queries, and `INSERT ... SELECT`
- `postgres_style`
  - grouped by feature family

Important rule:

- each newly supported feature must also gain an "unsupported boundary" test
- for example, even after `UNION` support lands, the remaining `INTERSECT ALL` / `EXCEPT ALL` boundary should stay explicitly covered until that wider set-op surface is implemented

### Exit Criteria

- Snowdrop supports a wider, explicitly documented query subset
- the shared relation pipeline backs subqueries, derived tables, CTEs, set operations, and broader `INSERT ... SELECT`
- failure modes for still-unsupported queries are deterministic and readable

### Migration Risk Notes

These Phase 3 boundaries do not carry the same migration risk:

- `window functions`: medium risk after the broader Phase 3.5 slice because the most common ranking and running-window patterns are now covered, while advanced PostgreSQL-only variants still remain
- recursive CTE: medium, workload-dependent risk because they matter a lot for hierarchy and traversal use cases but do not appear in every application
- `LATERAL`: medium-high risk for SQL-heavy applications, especially ones that lean on per-row derived sources
- `INTERSECT` / `EXCEPT`: medium-low risk because they are often rewritable, even though native support is still preferable

This means Phase 3 should still be considered a practical migration subset, not a claim of full PostgreSQL query compatibility. The next phases should prioritize the highest-risk remaining gaps before chasing lower-frequency syntax.

## Phase 3.5: Advanced Query Compatibility Boundaries

### Objectives

- reduce rewrite pressure for migration-heavy SQL workloads
- close the highest-risk query-surface gaps intentionally left out of Phase 3
- keep unsupported advanced forms explicit and testable until they are implemented

### Why This Phase Exists

- Phase 3 already provides a practical migration subset for a broad range of workloads
- the remaining blockers are no longer “basic query surface” tasks
- they are advanced relation/planner semantics that deserve their own focused delivery phase before schema-fidelity work begins

### Functional Priorities

Implement in this order:

1. core window functions
2. narrow `LATERAL`
3. narrow recursive CTE
4. `INTERSECT` / `EXCEPT`

Current status:

- completed:
  - broader practical window-function slice
  - narrow `LATERAL` slice
  - narrow recursive CTE slice
  - distinct `INTERSECT` / `EXCEPT` slice
- remaining:
  - broader advanced variants beyond the narrow Phase 3.5 surface

### Implementation Scope

#### Window functions first

Target a practical migration-oriented window surface:

- `ROW_NUMBER() OVER (...)`
- `RANK() OVER (...)`
- `DENSE_RANK() OVER (...)`
- `LAG(expr [, offset [, default]]) OVER (...)`
- `LEAD(expr [, offset [, default]]) OVER (...)`
- aggregate-over-window subset:
  - `COUNT`
  - `SUM`
  - `AVG`
  - `MIN`
  - `MAX`

Supported syntax in this slice:

- `PARTITION BY` optional
- `ORDER BY` required for ranking and navigation functions
- `ORDER BY` optional for aggregate-over-window subset, and required when explicit running-window semantics are requested
- inline `OVER (...)` and named `WINDOW` references
- explicit `ROWS` frames for ordered aggregate windows
- grouped/window coexistence when windows run over the post-aggregate relation result

Still unsupported after the first window slice:

- named-window inheritance / override forms
- `RANGE` frames
- `GROUPS` frames
- `FILTER (...) OVER (...)`
- ordered-set / hypothetical-set aggregates
- nested window expressions
- window functions inside unsupported grouped or write-path contexts

Implementation intent:

- evaluate windows on materialized relation rows after source relation formation and filtering
- run window evaluation before final projection output is frozen
- preserve the optimized simple-table fast path for non-window queries

#### Narrow `LATERAL`

Support only:

- `INNER JOIN LATERAL (SELECT ...) alias ON ...`
- `LEFT JOIN LATERAL (SELECT ...) alias ON ...`
- mandatory alias
- right-hand lateral subquery may reference only relations already bound on the left

Still unsupported after this slice:

- lateral function/table-valued function sources
- lateral `VALUES`
- `RIGHT/FULL JOIN LATERAL`
- nested lateral chains that require broader planner rules than the current join subset

Implementation intent:

- extend relation-source binding so right-side relation execution can receive left-row scope
- reuse existing correlated-subquery machinery instead of inventing a separate outer-scope model

#### Narrow recursive CTE

Support only:

- one recursive CTE at a time
- form: `WITH RECURSIVE cte AS (seed UNION ALL recursive_term) SELECT ...`
- recursive self-reference only inside `recursive_term`
- one self-reference per recursive CTE
- column counts must match exactly
- deterministic recursion cap with a readable overflow error

Still unsupported after this slice:

- mutual recursion
- multiple recursive CTEs depending on each other
- `UNION DISTINCT` recursive fixpoint semantics
- data-modifying CTEs
- recursive CTEs inside unsupported compound/lateral combinations

Implementation intent:

- add an iterative fixpoint executor over relation results
- evaluate `seed`, then repeatedly evaluate `recursive_term` against the growing working set until no new rows appear or the recursion cap is reached
- keep `UNION ALL` semantics only in this slice

#### `INTERSECT` / `EXCEPT`

Support:

- `INTERSECT`
- `EXCEPT`

Behavior:

- relation-level set-op execution reuses the existing compound-query pipeline
- keep current column-count validation style
- keep current type-compatibility validation style

Still unsupported after this slice:

- `INTERSECT ALL`
- `EXCEPT ALL`

Implementation intent:

- normalize both relation inputs to the same output shape
- perform distinct set semantics only
- preserve relation-level `ORDER BY` / `LIMIT` handling over the compound result

### Required Test Coverage

Add or expand fixed datasets:

- `window_function_cases`
- `lateral_cases`
- `recursive_cte_cases`
- expand `set_ops_cases`

Suites:

- `golden`
  - happy-path and explicit-error coverage for every feature family
- `differential`
  - PostgreSQL parity for supported subsets only
- `sqllogictest`
  - `window_functions.test`
  - `lateral.test`
  - `recursive_cte.test`
  - expand `union.test` or add `set_ops_advanced.test`
- `postgres_style`
  - one grouped regression family per feature family
- `tap`
  - smoke workflow for each new supported feature family

Minimum required scenarios:

- `ROW_NUMBER()` with `PARTITION BY` + `ORDER BY`
- `RANK()` / `DENSE_RANK()` tie behavior
- `LAG` / `LEAD` with default argument
- aggregate-over-window over one partition and multiple partitions
- `JOIN LATERAL` reading left-row values
- one simple recursive hierarchy traversal
- recursion-cap failure
- `INTERSECT` and `EXCEPT` happy paths
- explicit unsupported tests for:
  - named window clauses
  - explicit frame clauses
  - lateral non-subquery sources
  - mutual recursion
  - `INTERSECT ALL`
  - `EXCEPT ALL`

### Exit Criteria

- the highest-risk query boundaries from Phase 3 are either supported or narrowed to smaller explicit boundaries
- the compatibility matrix is updated from `Not Supported` to `Supported` / `Partial` where applicable
- all newly supported forms are backed by fixed datasets and differential coverage
- the optimized simple-table path remains intact for non-advanced queries

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
