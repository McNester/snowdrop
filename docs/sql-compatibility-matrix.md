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
| `recursive_cte_cases` | narrow recursive `WITH RECURSIVE` hierarchy traversal, recursion-cap overflow, and `UNION DISTINCT` boundary coverage |
| `set_ops_cases` | `UNION ALL`, `UNION`, and explicit `INTERSECT` / `EXCEPT` boundary coverage |

These datasets are exercised through a mix of `golden`, `differential`, `sqllogictest`, `tap`, and `postgres_style` suites depending on the feature area.

## Statement-Level Support

| Area | Feature | Status | Notes |
|---|---|---|---|
| Database DDL | `CREATE DATABASE`, `DROP DATABASE`, `USE` | Supported | Routed directly in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:166) and [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:229). |
| Table DDL | `CREATE TABLE`, `DROP TABLE` | Partial | Basic table lifecycle works, but schema features are narrow. |
| Index DDL | `CREATE INDEX`, `DROP INDEX` | Partial | Present, but practical support is aimed at the current RocksDB secondary-index path. |
| Alter DDL | `ALTER TABLE` | Partial | Phase 4 now supports `ADD COLUMN`, `RENAME COLUMN`, `DROP COLUMN`, `ALTER COLUMN SET DEFAULT`, `ALTER COLUMN DROP DEFAULT`, `ALTER COLUMN SET NOT NULL`, and `ALTER COLUMN DROP NOT NULL` in the current narrow schema-fidelity subset. Existing-row backfill/validation semantics still remain narrower than PostgreSQL for `ADD COLUMN ... DEFAULT` and `SET NOT NULL`. Type rewrites, broader PostgreSQL-only `ALTER` forms, and multi-action atomic migration behavior remain out of scope. |
| Insert | `INSERT ... VALUES` | Supported | Implemented in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:504). |
| Insert | `INSERT ... SELECT` | Partial | Now runs through the relation pipeline in [orchestrator/src/query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs), supports grouped, ordered, subquery-backed, derived-table, CTE, and `UNION` sources inside the current Phase 3 subset, and fails explicitly on source/target column-count and source/target type-coercion mismatch. It remains partial because source relations are still materialized in memory before insert. |
| Update | `UPDATE ... SET ... WHERE ...` | Partial | Works, but write-path `WHERE` is limited to simple equality via [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:2718). |
| Delete | `DELETE ... WHERE ...` | Partial | Same simple `WHERE` restriction as `UPDATE`. |
| Select | Basic `SELECT ... FROM ...` | Supported | The top-level `SELECT` entrypoint now dispatches either to the shared relation execution API or to the optimized simple-table storage path in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs), depending on the query shape. |
| Select | `SELECT` without `FROM` | Partial | Phase 6.5 now supports a narrow scalar subset through the relation projection path, including constant projections such as `SELECT 1`. Broader expression coverage remains intentionally conservative until more no-source relation cases are regression-tested. |
| Compound queries | `UNION ALL`, `UNION` | Supported | Compound relation execution now flows through the internal relation pipeline in [orchestrator/src/query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs), including relation-level `ORDER BY` and `LIMIT` over compound outputs, plus explicit column-count and type-compatibility mismatch errors. |
| Compound queries | `INTERSECT`, `EXCEPT` | Partial | Phase 3.5 now supports distinct `INTERSECT` and `EXCEPT` through the relation-level compound-query pipeline. `INTERSECT ALL` and `EXCEPT ALL` remain explicitly unsupported. |
| CTEs | non-recursive `WITH ...` | Supported | Non-recursive CTEs are loaded into relation context and can be referenced anywhere the current Phase 3 subset accepts a table-like source. |
| CTEs | recursive `WITH RECURSIVE ...` | Partial | Phase 3.5 now includes a narrow recursive subset: one recursive CTE at a time, `seed UNION ALL recursive_term`, exactly one self-reference in the recursive term, exact column-count matching, and a deterministic iteration cap. Mutual recursion, multiple recursive CTEs, `UNION DISTINCT` recursion semantics, and data-modifying recursive CTEs remain unsupported. |
| Views | `CREATE VIEW`, querying views | Not Supported | No view support path is implemented. |

## Query Features

| Area | Feature | Status | Notes |
|---|---|---|---|
| Filtering | `WHERE` comparisons (`=`, `!=`, `<`, `<=`, `>`, `>=`) | Supported | Implemented in [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs:51). |
| Filtering | `AND`, `OR`, `NOT` | Supported | Implemented in [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs:53). |
| Filtering | `IS NULL`, `IS NOT NULL` | Supported | Implemented in [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs:16). |
| Filtering | `BETWEEN`, `IN (...)`, `LIKE` | Supported | Implemented in [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs:23), [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs:31), and [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs:39). |
| Filtering | `ILIKE` and practical `LIKE` wildcards / `ESCAPE` | Partial | Phase 5 now supports `%`, `_`, `ESCAPE`, and case-insensitive `ILIKE` in [executor/src/typed_value.rs](/e:/snowdrop-1/executor/src/typed_value.rs). Regex operators and broader PostgreSQL text-pattern semantics remain unsupported. |
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
| Derived tables | `LATERAL` | Partial | Phase 3.5 now supports narrow `INNER JOIN LATERAL (SELECT ...) alias ON ...` and `LEFT JOIN LATERAL (SELECT ...) alias ON ...` forms. `LATERAL VALUES`, right/full lateral joins, and broader lateral source shapes remain unsupported. |

## JOIN Support

| Area | Feature | Status | Notes |
|---|---|---|---|
| Join type | `INNER JOIN ... ON` | Partial | Supported in the current table/derived-table relation subset through the shared join execution paths in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs). The `ON` predicate surface is still the practical subset listed below. |
| Join type | `LEFT JOIN ... ON` | Partial | Supported in the same practical relation subset as `INNER JOIN`, including null-padding over the unmatched right side. |
| Join type | `RIGHT JOIN ... ON` | Partial | Phase 6.5 now supports right-side null-padding and unmatched-right preservation in the shared join execution paths. `LATERAL` right joins and broader PostgreSQL planner semantics remain out of scope. |
| Join type | `FULL OUTER JOIN ... ON` | Partial | Phase 6.5 now preserves unmatched rows from both sides with deterministic null-padding in the shared relation-backed join executor. Broader PostgreSQL join-expression parity remains out of scope. |
| Join type | `CROSS JOIN` | Partial | Phase 6.5 now supports table and supported derived-relation cross products through the shared join execution paths. `NATURAL JOIN` / `USING` are still unsupported. |
| Join predicate | Equi-join (`=`) | Partial | Only column-to-column equality predicates are supported, optionally combined with `AND`, in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:8224). |
| Join predicate | Non-equality join predicates | Not Supported | Broader `ON` expressions are rejected in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:8224). |
| Join optimization | Predicate pushdown and needed-column pruning | Partial | Needed-column pruning remains in place across the current join subset. Predicate pushdown is still intentionally conservative and does not claim full PostgreSQL outer-join rewrite safety. |

## Expressions and Functions

| Area | Feature | Status | Notes |
|---|---|---|---|
| Scalar expressions | Arithmetic `+ - * / %` | Partial | Implemented in [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs:127). Phase 5 has started tightening `DECIMAL` / `NUMERIC` arithmetic so exact-decimal paths no longer immediately degrade into loose float behavior, but the wider type-semantics and fast-path surface still needs follow-up hardening. |
| Scalar expressions | `CASE WHEN` | Supported | Implemented in [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs:183). |
| Scalar expressions | window functions | Partial | Phase 3.5 now includes a broader practical relation-level window slice: `ROW_NUMBER`, `RANK`, `DENSE_RANK`, `LAG`, `LEAD`, partition aggregate windows (`COUNT`, `SUM`, `AVG`, `MIN`, `MAX`), named `WINDOW` references, ordered aggregate windows with explicit `ROWS` frames, and grouped/window coexistence over post-aggregate relation results. `RANGE` / `GROUPS` frames, `FILTER (...) OVER (...)`, nested windows, named-window inheritance/override, and the wider PostgreSQL window surface remain explicitly unsupported. |
| Scalar functions | `LOWER`, `UPPER`, `LENGTH`, `TRIM`, `SUBSTRING`, `CONCAT` | Supported | Implemented in the row and `RowView` expression evaluators in [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs) and covered by the `scalar_functions` dataset. |
| Null helpers | `COALESCE`, `NULLIF` | Supported | Implemented in [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs) with fixed-dataset coverage in `tests/datasets/scalar_functions/`. |
| Conversion | `CAST` and PostgreSQL-style casts | Partial | `CAST(...)` and `::` work for the current Phase 2 target set, and Phase 5 now preserves exact `DECIMAL` / `NUMERIC` values plus practical `DATE` / `TIMESTAMP` / `TIME` casts as typed runtime values instead of immediately collapsing them to float-only or text-only behavior. Unsupported target types are still rejected up front in [orchestrator/src/query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs) and runtime conversion still lives in [executor/src/typed_value.rs](/e:/snowdrop-1/executor/src/typed_value.rs). |
| Date/time functions | `NOW()`, `CURRENT_DATE`, `CURRENT_TIME`, `CURRENT_TIMESTAMP`, `LOCALTIME`, `LOCALTIMESTAMP` | Partial | Phase 5 now evaluates the basic helpers as typed runtime `DATE` / `TIMESTAMP` / `TIME` values in [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs), but truncation helpers and interval arithmetic are still absent. |
| Date/time functions | `EXTRACT` | Partial | Phase 5 now supports a practical subset in [executor/src/filter.rs](/e:/snowdrop-1/executor/src/filter.rs): `YEAR`, `MONTH`, `DAY`, `HOUR`, `MINUTE`, `SECOND`, `DOW`, `ISODOW`, `DOY`, `QUARTER`, and ISO-style `WEEK`. Timezone-oriented fields, `WEEK(<weekday>)`, truncation helpers, and interval-aware scalar logic remain unsupported. |
| Date/time functions | truncation helpers, interval-aware scalar logic | Not Supported | No dedicated scalar truncation or interval operator layer exists yet. |
| Columnar fast path | Arithmetic-only scalar expressions | Partial | Fast columnar evaluation supports a restricted expression subset in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:2570). |

## Schema and Type Support

| Area | Feature | Status | Notes |
|---|---|---|---|
| Types | `INT`, `BIGINT`, `FLOAT`, `DOUBLE`, `DECIMAL`, `NUMERIC`, `CHAR`, `VARCHAR`, `TEXT`, `DATE`, `DATETIME`, `TIMESTAMP`, `BOOLEAN` | Partial | Declared in [schema/src/types.rs](/e:/snowdrop-1/schema/src/types.rs:5). Phase 5 now preserves exact `DECIMAL` / `NUMERIC` semantics and a practical typed `DATE` / `TIMESTAMP` / `TIME` runtime slice, but schema-level `TIME`, timezone-aware temporal types, and broader temporal behavior are still in progress. |
| Types | `JSON`, `ENUM`, `SET`, `BLOB`, `BINARY`, `VARBINARY`, `BYTEA` | Not Supported | Excluded in [schema/src/types.rs](/e:/snowdrop-1/schema/src/types.rs:27). |
| Primary key | Single-column PK | Supported | Supported throughout table creation and write-path enforcement. |
| Primary key | Composite PK | Partial | Phase 4 now supports multi-column primary keys at DDL time and in write-path uniqueness enforcement. Broader downstream semantics and richer referencing behavior still remain narrower than PostgreSQL. |
| Constraints | `NOT NULL` | Supported | Parsed in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:339). |
| Constraints | `DEFAULT` | Partial | Phase 4 now stores normalized default expressions in schema metadata and applies them during `INSERT` for the current safe scalar subset. Existing-row backfill during `ALTER TABLE ... ADD COLUMN ... DEFAULT` is still narrower than PostgreSQL. Volatile PostgreSQL-specific defaults remain out of scope. |
| Constraints | user-facing `UNIQUE` | Partial | Phase 4 now supports single-column and multi-column `UNIQUE` constraints with insert/update enforcement and dedicated duplicate-constraint failures. |
| Constraints | `FOREIGN KEY` | Partial | Phase 4 now supports immediate enforcement for the narrow subset: single-column references and primary-key-shaped references, with `NO ACTION` / `RESTRICT` semantics only. `CASCADE`, `SET NULL`, `SET DEFAULT`, deferred constraints, and broader planner/cycle semantics remain unsupported. |
| Constraints | `CHECK` | Partial | Phase 4 now supports row-local boolean checks over the current safe scalar subset and rejects unsupported expressions such as subqueries at DDL time. |
| Alter DDL | Column add | Supported | Implemented in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:383). |
| Alter DDL | Rename/drop/default/nullability changes | Partial | Supported in the current Phase 4 subset. Existing-row validation for nullability changes is still narrower than PostgreSQL. Type rewrites and broader table rewrite semantics remain unsupported. |

## Catalog and Compatibility Surface

| Area | Feature | Status | Notes |
|---|---|---|---|
| Introspection | `SHOW DATABASES`, `SHOW TABLES`, `DESCRIBE`, `\\d` | Supported | Routed manually in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:166). |
| Introspection | `INFORMATION_SCHEMA.TABLES`, `INFORMATION_SCHEMA.COLUMNS`, `INFORMATION_SCHEMA.TABLE_CONSTRAINTS`, `INFORMATION_SCHEMA.KEY_COLUMN_USAGE`, `INFORMATION_SCHEMA.REFERENTIAL_CONSTRAINTS` | Partial | Targeted handlers now cover the core table/column/constraint introspection paths in [query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs:271) and [schema/src/ddl.rs](/e:/snowdrop-1/schema/src/ddl.rs:894), but broader `information_schema` parity and general query-shape support are still pending. |
| PostgreSQL catalogs | `pg_catalog` compatibility | Partial | Phase 5 now exposes a narrow practical compatibility layer for common introspection queries over `pg_tables`, `pg_class`, `pg_attribute`, and table-backed `pg_constraint` rows in [orchestrator/src/query_router.rs](/e:/snowdrop-1/orchestrator/src/query_router.rs). Broader `pg_catalog` parity remains out of scope. |
| Migration workflow | `migration-preflight` / `migration-run` | Partial | Phase 6 provides PostgreSQL schema introspection, unsupported/partial feature inventory, safe Snowdrop DDL preview, practical table-by-table loading, and replay of a basic verification-query subset (row counts, ordered samples, numeric aggregates) in [snowdrop-testkit/src/main.rs](/e:/snowdrop-1/snowdrop-testkit/src/main.rs). Broader application-query replay and more advanced orchestration remain pending. |
| Migration workflow | `migration-auto` | Partial | Phase 6.5 adds a one-command automatic migration path over the safe subset. It can work from a live PostgreSQL database or a checked-in dump directory, fails early on unsafe blockers, writes structured preflight/workflow/automatic-migration artifacts, and only proceeds when the translated subset is considered safe. |
| Protocol | PostgreSQL wire protocol (`pgwire`) | Supported | Snowdrop is reachable through the pgwire server in [orchestrator/src/pgwire_handler.rs](/e:/snowdrop-1/orchestrator/src/pgwire_handler.rs:87). |

## Practical Conclusion

Snowdrop already has a meaningful analytical SQL subset:

- core DDL and DML
- basic `SELECT`
- filtered scans
- `GROUP BY`
- `ORDER BY`
- `LIMIT/OFFSET`
- `INNER`, `LEFT`, `RIGHT`, `FULL OUTER`, and `CROSS` joins inside the current equi-join subset
- the five core aggregate functions

But it is not yet a PostgreSQL-compatible application substrate.

The biggest functional gaps before migration work are now:

- broader schema fidelity beyond the narrow Phase 4 slice
- richer write-path semantics beyond the current equality-based update/delete subset
- deeper grouped/query semantics such as complex `GROUP BY` expressions
- remaining out-of-scope query forms: `INTERSECT ALL`, `EXCEPT ALL`, and the broader advanced variants beyond the current narrow window/LATERAL/recursive CTE slices
- better type semantics and catalog compatibility

## Migration Risk Notes

These remaining boundaries do not block every migration equally:

- `window functions`: medium migration risk after the broader Phase 3.5 slice; the highest-frequency reporting, ranking, and running-total patterns are now covered, while exotic framing and advanced PostgreSQL-only variants remain deferred
- recursive CTE: medium migration risk; important for hierarchy, graph-like traversal, and tree-walk workloads, but absent from many CRUD-oriented applications
- `LATERAL`: medium-high migration risk for SQL-heavy applications that rely on row-wise derived subqueries or function-like table expansion
- `INTERSECT` / `EXCEPT`: medium-low migration risk; important for some analytical query sets, but often rewritable through `JOIN`, `NOT EXISTS`, or `UNION`-based patterns

`Phase 3.5` now has its full planned slice set in place:

1. practical window-function surface
2. narrow `LATERAL`
3. narrow recursive CTE
4. distinct `INTERSECT` / `EXCEPT`

In practical terms, Snowdrop's current Phase 3 + Phase 3.5 subset is strong enough for a meaningful migration-oriented SQL baseline, but it is not yet sufficient for arbitrary PostgreSQL SQL with zero rewrite expectations. The remaining gaps are now narrower advanced variants and the broader schema-fidelity backlog rather than top-level missing query families.

Phase 4 has now materially reduced schema-level migration risk for practical workloads by landing a narrow but real schema-fidelity slice:

1. broader `ALTER TABLE`
2. `DEFAULT` values
3. user-facing `UNIQUE`
4. composite primary keys
5. immediate narrow `FOREIGN KEY` enforcement
6. practical `CHECK` constraints

What still remains for later phases is the broader PostgreSQL schema surface, not the complete absence of schema contracts.
