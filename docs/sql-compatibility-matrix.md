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

These datasets are exercised through a mix of `golden`, `differential`, `sqllogictest`, `tap`, and `postgres_style` suites depending on the feature area.

## Statement-Level Support

| Area | Feature | Status | Notes |
|---|---|---|---|
| Database DDL | `CREATE DATABASE`, `DROP DATABASE`, `USE` | Supported | Routed directly in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:166) and [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:229). |
| Table DDL | `CREATE TABLE`, `DROP TABLE` | Partial | Basic table lifecycle works, but schema features are narrow. |
| Index DDL | `CREATE INDEX`, `DROP INDEX` | Partial | Present, but practical support is aimed at the current RocksDB secondary-index path. |
| Alter DDL | `ALTER TABLE` | Partial | Only `ADD COLUMN` is supported in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:383). |
| Insert | `INSERT ... VALUES` | Supported | Implemented in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:504). |
| Insert | `INSERT ... SELECT` | Not Supported | Non-`VALUES` insert sources are rejected in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:525). |
| Update | `UPDATE ... SET ... WHERE ...` | Partial | Works, but write-path `WHERE` is limited to simple equality via [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:2718). |
| Delete | `DELETE ... WHERE ...` | Partial | Same simple `WHERE` restriction as `UPDATE`. |
| Select | Basic `SELECT ... FROM ...` | Supported | Core path is in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:659). |
| Select | `SELECT` without `FROM` | Not Supported | Rejected in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:668). |
| Compound queries | `UNION`, `INTERSECT`, `EXCEPT` | Not Supported | Only `SetExpr::Select` is accepted in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:662). |
| CTEs | `WITH ...` | Not Supported | No dedicated support path in the router. |
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
| Ordering | `ORDER BY expression` | Partial | `sort_rows()` only handles identifiers and compound identifiers in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:2234). |
| Limiting | `LIMIT`, `OFFSET` | Supported | Numeric extraction is implemented in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:2718). |
| Aggregation | `COUNT`, `SUM`, `AVG`, `MIN`, `MAX` | Supported | Aggregate extraction and execution are in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:2022) and [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:2131). |
| Aggregation | `GROUP BY column` | Supported | Plain identifier and qualified identifier grouping are supported in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:2045). |
| Aggregation | `GROUP BY expression` | Partial | Complex expressions are not first-class in `extract_group_by_cols()` and related paths. |
| Aggregation | `HAVING` | Not Supported | No explicit handling is present in the current router. |

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
| Scalar functions | Generic functions like `LOWER`, `UPPER`, `LENGTH`, `TRIM`, `SUBSTRING`, `CONCAT` | Not Supported | `resolve_value()` does not implement generic `Expr::Function` evaluation in [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs:88). |
| Null helpers | `COALESCE`, `NULLIF` | Not Supported | No dedicated function evaluation path exists yet. |
| Conversion | `CAST` and PostgreSQL-style casts | Not Supported | No cast execution layer is implemented. |
| Date/time functions | `NOW()`, `CURRENT_DATE`, truncation/extract helpers | Not Supported | No dedicated scalar date/time function layer exists yet. |
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

The biggest functional gaps before migration work are:

- scalar function coverage
- richer schema fidelity
- wider SQL surface (`HAVING`, subqueries, CTEs, compound queries)
- better write-path semantics
- better type semantics and catalog compatibility
