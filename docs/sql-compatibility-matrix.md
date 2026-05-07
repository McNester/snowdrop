# Snowdrop SQL Compatibility Matrix

## Scope

This matrix is derived from the current implementation, not from aspirational design goals.

Primary code references:

- statement routing and execution: [orchestrator/src/query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:156)
- predicate and expression evaluation: [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs:8)
- runtime typed values: [executor/src/typed_value.rs](/e:/snowdrop-1/executor/src/typed_value.rs:16)
- schema type support: [schema/src/types.rs](/e:/snowdrop-1/schema/src/types.rs:5)
- table DDL validation: [schema/src/ddl.rs](/e:/snowdrop-1/schema/src/ddl.rs:223)

Status meanings:

- `Supported`: implemented and intentionally usable now
- `Partial`: implemented with important restrictions or caveats
- `Not Supported`: absent, rejected, or too incomplete to rely on

## Current Validation Coverage

The current fixed-dataset foundation now covers these semantic zones:

| Dataset | Focus |
|---|---|
| `tiny_smoke` | happy-path CRUD, aggregates, and simple joins |
| `edge_cases` | null-adjacent and boundary-like correctness cases |
| `null_semantics` | `NULL` filters, grouped counts, grouped sums |
| `order_group_cases` | grouped ordering, aggregate alias ordering, tie-breaking |
| `join_skew` | skew-sensitive join aggregation |
| `join_edge_cases` | duplicate join matches, unmatched rows, left-join null filling |
| `write_path_smoke` | insert, update, delete round-trips |
| `perf_fixed` | stable non-synthetic micro-benchmark fixture |
| `scalar_functions` | scalar functions, casts, and basic date/time helper coverage |
| `having_cases` | grouped filtering, aggregate aliases, and grouped-order expression coverage |
| `insert_select_cases` | `INSERT ... SELECT` from grouped, ordered, and CTE-backed relation sources |
| `subquery_cases` | non-correlated scalar, `IN`, and boundary-error subquery coverage |
| `correlated_subquery_cases` | correlated `EXISTS`, `IN`, scalar projection, and grouped `HAVING` coverage |
| `derived_table_cases` | derived tables in `FROM` / `JOIN`, alias requirements, and `LATERAL` boundary coverage |
| `cte_cases` | non-recursive CTE chains and recursive-CTE boundary coverage |
| `set_ops_cases` | `UNION ALL`, `UNION`, and explicit `INTERSECT` / `EXCEPT` boundary coverage |

These datasets are exercised through a mix of `golden`, `differential`, `sqllogictest`, `tap`, and `postgres_style` suites depending on the feature area.

## Statement-Level Support

| Area | Feature | Status | Notes |
|---|---|---|---|
| Database DDL | `CREATE DATABASE`, `DROP DATABASE`, `USE` | Supported | Routed directly in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:166) and [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:229). |
| Table DDL | `CREATE TABLE`, `DROP TABLE` | Partial | Basic table lifecycle works, but schema features are narrow. |
| Index DDL | `CREATE INDEX`, `DROP INDEX` | Partial | Present, but practical support is aimed at the current RocksDB secondary-index path. |
| Alter DDL | `ALTER TABLE` | Partial | Only `ADD COLUMN` is supported in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:383). |
| Insert | `INSERT ... VALUES` | Supported | Implemented in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:504). |
| Insert | `INSERT ... SELECT` | Partial | Now runs through the relation pipeline in [orchestrator/src/query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs), supports grouped, ordered, subquery-backed, derived-table, CTE, and `UNION` sources inside the current Phase 3 subset, and fails explicitly on source/target column-count and source/target type-coercion mismatch. It remains partial because source relations are still materialized in memory before insert. |
| Update | `UPDATE ... SET ... WHERE ...` | Partial | Works, but write-path `WHERE` is limited to simple equality via [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:2718). |
| Delete | `DELETE ... WHERE ...` | Partial | Same simple `WHERE` restriction as `UPDATE`. |
| Select | Basic `SELECT ... FROM ...` | Supported | The top-level `SELECT` entrypoint now dispatches either to the shared relation execution API or to the optimized simple-table storage path in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs), depending on the query shape. |
| Select | `SELECT` without `FROM` | Not Supported | Rejected in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:668). |
| Compound queries | `UNION ALL`, `UNION` | Supported | Compound relation execution now flows through the internal relation pipeline in [orchestrator/src/query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs), including relation-level `ORDER BY` and `LIMIT` over compound outputs, plus explicit column-count and type-compatibility mismatch errors. |
| Compound queries | `INTERSECT`, `EXCEPT` | Not Supported | Explicit readable runtime errors are emitted from the relation pipeline when these operators appear. |
| CTEs | non-recursive `WITH ...` | Supported | Non-recursive CTEs are loaded into relation context and can be referenced anywhere the current Phase 3 subset accepts a table-like source. |
| CTEs | recursive `WITH RECURSIVE ...` | Not Supported | Explicitly rejected with a dedicated runtime error. |
| Views | `CREATE VIEW`, querying views | Not Supported | No view support path is implemented. |

## Query Features

| Area | Feature | Status | Notes |
|---|---|---|---|
| Filtering | `WHERE` comparisons (`=`, `!=`, `<`, `<=`, `>`, `>=`) | Supported | Implemented in [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs:51). |
| Filtering | `AND`, `OR`, `NOT` | Supported | Implemented in [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs:53). |
| Filtering | `IS NULL`, `IS NOT NULL` | Supported | Implemented in [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs:16). |
| Filtering | `BETWEEN`, `IN (...)`, `LIKE` | Supported | Implemented in [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs:23), [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs:31), and [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs:39). |
| Filtering | `ILIKE`, regex, advanced pattern matching | Not Supported | `LIKE` is intentionally simplified in [typed_value.rs](/e:/snowdrop-1/executor/src/typed_value.rs:116). |
| Ordering | `ORDER BY column` | Supported | Core row sort logic is in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:2228). |
| Ordering | `ORDER BY expression` | Partial | Relation-level sorting now evaluates general expressions after subqueries, CTEs, derived tables, and `UNION` results through the relation pipeline, but it is still a materialized-row path rather than a planner/optimizer feature. |
| Limiting | `LIMIT`, `OFFSET` | Supported | Numeric extraction is implemented in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:2718). |
| Aggregation | `COUNT`, `SUM`, `AVG`, `MIN`, `MAX` | Supported | Aggregate extraction and execution are in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:2022) and [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:2131). |
| Aggregation | `GROUP BY column` | Supported | Plain identifier and qualified identifier grouping are supported in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:2045). |
| Aggregation | `GROUP BY expression` | Partial | Complex expressions are not first-class in `extract_group_by_cols()` and related paths. |
| Aggregation | `HAVING` | Partial | `HAVING` now runs on the shared relation pipeline, including scalar/`EXISTS`/`IN` subqueries in the supported grouped subset. It remains partial because grouped expressions are still narrower than PostgreSQL as a whole. |
| Subqueries | non-correlated scalar / `IN` / `EXISTS` | Supported | Supported through relation-aware predicate and projection evaluation in [orchestrator/src/query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs). |
| Subqueries | correlated `EXISTS`, correlated `IN`, correlated scalar subqueries in `WHERE` / `HAVING` | Partial | Supported in the current Phase 3 subset through outer-row scoping, but nested/lateral-like forms and subqueries inside arbitrary scalar functions remain intentionally out of scope. |
| Derived tables | subqueries in `FROM` / `JOIN` with aliases | Supported | Derived tables are materialized as relation sources and can participate in the current join subset. |
| Derived tables | `LATERAL` | Not Supported | Explicitly rejected with a dedicated runtime error. |

## JOIN Support

| Area | Feature | Status | Notes |
|---|---|---|---|
| Join type | `INNER JOIN ... ON` | Supported | Implemented in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:859). |
| Join type | `LEFT JOIN ... ON` | Supported | Implemented in the same join handler. |
| Join type | `RIGHT JOIN`, `FULL OUTER JOIN`, `CROSS JOIN` | Not Supported | Only `INNER ... ON` and `LEFT ... ON` are accepted in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:921). |
| Join predicate | Equi-join (`=`) | Supported | Extracted in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:2803). |
| Join predicate | Non-equality join predicates | Not Supported | Rejected in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:2815). |
| Join optimization | Predicate pushdown and needed-column pruning | Supported | Implemented in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:884), [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:927), and [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:3129). |

## Expressions and Functions

| Area | Feature | Status | Notes |
|---|---|---|---|
| Scalar expressions | Arithmetic `+ - * / %` | Supported | Implemented in [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs:127). |
| Scalar expressions | `CASE WHEN` | Supported | Implemented in [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs:183). |
| Scalar expressions | window functions | Not Supported | No window-function execution layer exists in the current relation pipeline or expression engine, and `OVER (...)` forms are rejected up front with a dedicated runtime error. |
| Scalar functions | `LOWER`, `UPPER`, `LENGTH`, `TRIM`, `SUBSTRING`, `CONCAT` | Supported | Implemented in the row and `RowView` expression evaluators in [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs) and covered by the `scalar_functions` dataset. |
| Null helpers | `COALESCE`, `NULLIF` | Supported | Implemented in [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs) with fixed-dataset coverage in `tests/datasets/scalar_functions/`. |
| Conversion | `CAST` and PostgreSQL-style casts | Partial | `CAST(...)` and `::` work for the current Phase 2 target set: text, integer, float, boolean, and string-backed date/time targets, with unsupported target types rejected up front in [orchestrator/src/query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs) and runtime conversion in [executor/src/typed_value.rs](/e:/snowdrop-1/executor/src/typed_value.rs). |
| Date/time functions | `NOW()`, `CURRENT_DATE`, `CURRENT_TIME`, `CURRENT_TIMESTAMP`, `LOCALTIME`, `LOCALTIMESTAMP` | Partial | Basic helpers now evaluate as text-returning scalar expressions in [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs), but richer date/time operators like `EXTRACT`, truncation, and interval arithmetic are still absent. |
| Date/time functions | `EXTRACT`, truncation helpers, interval-aware scalar logic | Not Supported | No dedicated scalar date/time operator layer exists beyond the basic Phase 2 helpers. |
| Columnar fast path | Arithmetic-only scalar expressions | Partial | Fast columnar evaluation supports a restricted expression subset in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:2570). |

## Schema and Type Support

| Area | Feature | Status | Notes |
|---|---|---|---|
| Types | `INT`, `BIGINT`, `FLOAT`, `DOUBLE`, `DECIMAL`, `NUMERIC`, `CHAR`, `VARCHAR`, `TEXT`, `DATE`, `DATETIME`, `TIMESTAMP`, `BOOLEAN` | Supported | Declared in [schema/src/types.rs](/e:/snowdrop-1/schema/src/types.rs:5). |
| Types | `JSON`, `ENUM`, `SET`, `BLOB`, `BINARY`, `VARBINARY`, `BYTEA` | Not Supported | Excluded in [schema/src/types.rs](/e:/snowdrop-1/schema/src/types.rs:27). |
| Primary key | Single-column PK | Supported | Required by [schema/src/ddl.rs](/e:/snowdrop-1/schema/src/ddl.rs:223). |
| Primary key | Composite PK | Not Supported | Exactly one primary key column is enforced in [schema/src/ddl.rs](/e:/snowdrop-1/schema/src/ddl.rs:223). |
| Constraints | `NOT NULL` | Supported | Parsed in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:339). |
| Constraints | `DEFAULT`, `CHECK`, `FOREIGN KEY`, user-facing `UNIQUE` beyond PK semantics | Not Supported | No dedicated implementation path exists in current table DDL handling. |
| Alter DDL | Column add | Supported | Implemented in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:383). |
| Alter DDL | Rename/drop/alter type/default/constraint changes | Not Supported | Rejected by the narrow `ALTER TABLE` implementation. |

## Catalog and Compatibility Surface

| Area | Feature | Status | Notes |
|---|---|---|---|
| Introspection | `SHOW DATABASES`, `SHOW TABLES`, `DESCRIBE`, `\\d` | Supported | Routed manually in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:166). |
| Introspection | `INFORMATION_SCHEMA.TABLES`, `INFORMATION_SCHEMA.COLUMNS` | Partial | Only targeted handlers are implemented in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:271). |
| PostgreSQL catalogs | `pg_catalog` compatibility | Not Supported | There is no real pg_catalog emulation layer yet. |
| Protocol | PostgreSQL wire protocol (`pgwire`) | Supported | Snowdrop is reachable through the pgwire server in [orchestrator/src/pgwire_handler.rs](/e:/snowdrop-1/orchestrator/src/pgwire_handler.rs:87). |

## Practical Conclusion

Snowdrop already has a meaningful analytical SQL subset:

- core DDL and DML
- basic `SELECT`
- filtered scans
- `GROUP BY`
- `ORDER BY`
- `LIMIT/OFFSET`
- `INNER` and `LEFT` equi-joins
- the five core aggregate functions

But it is not yet a PostgreSQL-compatible application substrate.

The biggest functional gaps before migration work are now:

- broader schema fidelity
- richer write-path semantics beyond the current equality-based update/delete subset
- deeper grouped/query semantics such as complex `GROUP BY` expressions
- remaining out-of-scope query forms: recursive CTE, `INTERSECT`, `EXCEPT`, `LATERAL`, and window functions
- better type semantics and catalog compatibility

## Migration Risk Notes

These unsupported query forms do not block every migration equally:

- `window functions`: high migration risk; common in reporting, ranking, deduplication, top-N-per-group, and running-total workloads
- recursive CTE: medium migration risk; important for hierarchy, graph-like traversal, and tree-walk workloads, but absent from many CRUD-oriented applications
- `LATERAL`: medium-high migration risk for SQL-heavy applications that rely on row-wise derived subqueries or function-like table expansion
- `INTERSECT` / `EXCEPT`: medium-low migration risk; important for some analytical query sets, but often rewritable through `JOIN`, `NOT EXISTS`, or `UNION`-based patterns

In practical terms, Snowdrop's current Phase 3 subset is strong enough for a meaningful migration-oriented SQL baseline, but it is not yet sufficient for arbitrary PostgreSQL SQL with zero rewrite expectations.
