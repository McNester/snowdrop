# Snowdrop Test Report

Generated at: `2026-05-07T02:04:05.176735562+00:00`

## Environment

| Component | Host | Port |
|---|---|---|
| Snowdrop pgwire | orchestrator | 5432 |
| PostgreSQL | postgres | 5432 |
| Health | orchestrator | 8080 |
| Metrics | orchestrator | 9090 |

## Dataset Inventory

| Dataset | Database | PostgreSQL | Purpose | Suites |
|---|---|---|---|---|
| correlated_subquery_cases | correlated_subquery_cases | yes | Exercise supported correlated subqueries while keeping lateral-style boundaries explicit. | golden, differential, sqllogictest, postgres_style |
| cte_cases | cte_cases | yes | Exercise single and multi-CTE relation execution while keeping recursive CTE unsupported. | golden, differential, sqllogictest, tap, postgres_style |
| derived_table_cases | derived_table_cases | yes | Exercise derived tables as first-class relations and keep alias requirements explicit. | golden, differential, sqllogictest, tap, postgres_style |
| edge_cases | edge_cases | yes | Exercise correctness around edge-case predicates and aggregates. | golden, differential |
| having_cases | having_cases | yes | Exercise grouped filtering, aggregate aliases, and ORDER BY expressions on grouped output. | golden, differential, sqllogictest, tap, postgres_style |
| insert_select_cases | insert_select_cases | yes | Exercise INSERT ... SELECT over grouped, CTE-backed, and ordered relation sources. | golden, differential, sqllogictest, tap, postgres_style |
| join_edge_cases | join_edge_cases | yes | Exercise join edge behavior for inner and left join semantics. | golden, differential |
| join_skew | join_skew | yes | Exercise join behavior and simple skew-sensitive aggregations. | golden, differential |
| null_semantics | null_semantics | yes | Exercise NULL semantics in filters, counts, sums, and ordering. | golden, differential, sqllogictest, postgres_style |
| order_group_cases | order_group_cases | yes | Exercise deterministic ORDER BY behavior after GROUP BY and aggregate aliases. | golden, differential, sqllogictest, postgres_style |
| perf_fixed | perf_fixed | yes | Stable non-synthetic fixture for future query micro-benchmarks. | golden, differential |
| scalar_functions | scalar_functions | yes | Validate COALESCE/NULLIF, text helpers, CAST, CURRENT_DATE, and NOW on a small readable dataset. | golden, differential, sqllogictest, tap, postgres_style |
| set_ops_cases | set_ops_cases | yes | Exercise supported UNION paths and keep INTERSECT/EXCEPT boundaries explicit. | golden, differential, sqllogictest, postgres_style |
| subquery_cases | subquery_cases | yes | Exercise supported non-correlated subqueries and keep unsupported nested forms explicit. | golden, differential, sqllogictest, postgres_style |
| tiny_smoke | tiny_smoke | yes | Smoke correctness checks with a tiny readable fixture. | golden, differential, sqllogictest, tap, postgres_style |
| write_path_smoke | write_path_smoke | yes | Exercise the existing write-path contract with read-after-write validation. | golden, differential, sqllogictest, tap, postgres_style |

## Suite Summary By Dataset

| Dataset | Suite | Pass | Fail | Mismatch | Skip |
|---|---|---|---|---|---|
| correlated_subquery_cases | differential | 4 | 0 | 0 | 0 |
| correlated_subquery_cases | golden | 4 | 0 | 0 | 0 |
| correlated_subquery_cases | postgres-style | 3 | 0 | 0 | 0 |
| correlated_subquery_cases | sqllogictest | 4 | 0 | 0 | 0 |
| cte_cases | differential | 2 | 0 | 0 | 0 |
| cte_cases | golden | 3 | 0 | 0 | 0 |
| cte_cases | postgres-style | 3 | 0 | 0 | 0 |
| cte_cases | sqllogictest | 3 | 0 | 0 | 0 |
| cte_cases | tap | 3 | 0 | 0 | 0 |
| derived_table_cases | differential | 2 | 0 | 0 | 0 |
| derived_table_cases | golden | 4 | 0 | 0 | 0 |
| derived_table_cases | postgres-style | 3 | 0 | 0 | 0 |
| derived_table_cases | sqllogictest | 4 | 0 | 0 | 0 |
| derived_table_cases | tap | 3 | 0 | 0 | 0 |
| edge_cases | differential | 2 | 0 | 0 | 0 |
| edge_cases | golden | 2 | 0 | 0 | 0 |
| having_cases | differential | 3 | 0 | 0 | 0 |
| having_cases | golden | 3 | 0 | 0 | 0 |
| having_cases | postgres-style | 3 | 0 | 0 | 0 |
| having_cases | sqllogictest | 2 | 0 | 0 | 0 |
| having_cases | tap | 2 | 0 | 0 | 0 |
| insert_select_cases | differential | 4 | 0 | 0 | 0 |
| insert_select_cases | golden | 6 | 0 | 0 | 0 |
| insert_select_cases | postgres-style | 6 | 0 | 0 | 0 |
| insert_select_cases | sqllogictest | 6 | 0 | 0 | 0 |
| insert_select_cases | tap | 5 | 0 | 0 | 0 |
| join_edge_cases | differential | 2 | 0 | 0 | 0 |
| join_edge_cases | golden | 2 | 0 | 0 | 0 |
| join_skew | differential | 1 | 0 | 0 | 0 |
| join_skew | golden | 1 | 0 | 0 | 0 |
| null_semantics | differential | 2 | 0 | 0 | 0 |
| null_semantics | golden | 2 | 0 | 0 | 0 |
| null_semantics | postgres-style | 1 | 0 | 0 | 0 |
| null_semantics | sqllogictest | 2 | 0 | 0 | 0 |
| order_group_cases | differential | 2 | 0 | 0 | 0 |
| order_group_cases | golden | 2 | 0 | 0 | 0 |
| order_group_cases | postgres-style | 1 | 0 | 0 | 0 |
| order_group_cases | sqllogictest | 1 | 0 | 0 | 0 |
| perf_fixed | differential | 1 | 0 | 0 | 0 |
| perf_fixed | golden | 1 | 0 | 0 | 0 |
| scalar_functions | differential | 5 | 0 | 0 | 0 |
| scalar_functions | golden | 10 | 0 | 0 | 0 |
| scalar_functions | postgres-style | 4 | 0 | 0 | 0 |
| scalar_functions | sqllogictest | 5 | 0 | 0 | 0 |
| scalar_functions | tap | 4 | 0 | 0 | 0 |
| set_ops_cases | differential | 3 | 0 | 0 | 0 |
| set_ops_cases | golden | 7 | 0 | 0 | 0 |
| set_ops_cases | postgres-style | 6 | 0 | 0 | 0 |
| set_ops_cases | sqllogictest | 7 | 0 | 0 | 0 |
| subquery_cases | differential | 2 | 0 | 0 | 0 |
| subquery_cases | golden | 5 | 0 | 0 | 0 |
| subquery_cases | postgres-style | 4 | 0 | 0 | 0 |
| subquery_cases | sqllogictest | 5 | 0 | 0 | 0 |
| tiny_smoke | differential | 2 | 0 | 0 | 0 |
| tiny_smoke | golden | 3 | 0 | 0 | 0 |
| tiny_smoke | postgres-style | 2 | 0 | 0 | 0 |
| tiny_smoke | sqllogictest | 4 | 0 | 0 | 0 |
| tiny_smoke | tap | 4 | 0 | 0 | 0 |
| write_path_smoke | differential | 3 | 0 | 0 | 0 |
| write_path_smoke | golden | 3 | 0 | 0 | 0 |
| write_path_smoke | postgres-style | 1 | 0 | 0 | 0 |
| write_path_smoke | sqllogictest | 4 | 0 | 0 | 0 |
| write_path_smoke | tap | 3 | 0 | 0 | 0 |

## Phase 3 Feature Summary

| Status | Feature family |
|---|---|
| Supported | HAVING over the grouped aggregate subset |
| Supported | non-correlated subqueries in scalar, IN, and EXISTS forms |
| Supported | correlated EXISTS, IN, and scalar subqueries in WHERE / HAVING |
| Supported | derived tables in FROM and JOIN with required aliases |
| Supported | non-recursive WITH / CTE chains |
| Supported | UNION ALL and UNION |
| Supported | broader INSERT ... SELECT over relation-producing query sources |
| Supported | relation-level ORDER BY after subqueries, CTEs, derived tables, and UNION outputs |
| Partial | GROUP BY expressions beyond plain identifiers / qualified identifiers |
| Partial | INSERT ... SELECT currently materializes source relations in memory before insert |
| Partial | subqueries nested inside complex scalar function arguments remain intentionally narrow |
| Explicitly unsupported | recursive CTE |
| Explicitly unsupported | INTERSECT |
| Explicitly unsupported | EXCEPT |
| Explicitly unsupported | LATERAL |
| Explicitly unsupported | window functions |

## golden Results

| Dataset | Group | Case | Status | Details |
|---|---|---|---|---|
| correlated_subquery_cases | correlated_exists | correlated_exists | pass | matched expected rows |
| correlated_subquery_cases | correlated_having_scalar | correlated_having_scalar | pass | matched expected rows |
| correlated_subquery_cases | correlated_in | correlated_in | pass | matched expected rows |
| correlated_subquery_cases | correlated_scalar_projection | correlated_scalar_projection | pass | matched expected rows |
| cte_cases | multi_cte_join | multi_cte_join | pass | matched expected rows |
| cte_cases | recursive_cte_error | recursive_cte_error | pass | matched expected error |
| cte_cases | single_cte_totals | single_cte_totals | pass | matched expected rows |
| derived_table_cases | derived_from_grouped_source | derived_from_grouped_source | pass | matched expected rows |
| derived_table_cases | derived_join_totals | derived_join_totals | pass | matched expected rows |
| derived_table_cases | derived_table_without_alias_error | derived_table_without_alias_error | pass | matched expected error |
| derived_table_cases | lateral_error | lateral_error | pass | matched expected error |
| edge_cases | non_negative_sum | non_negative_sum | pass | matched expected rows |
| edge_cases | null_notes | null_notes | pass | matched expected rows |
| having_cases | having_scalar_subquery_threshold | having_scalar_subquery_threshold | pass | matched expected rows |
| having_cases | order_by_group_expression | order_by_group_expression | pass | matched expected rows |
| having_cases | sum_threshold | sum_threshold | pass | matched expected rows |
| insert_select_cases | archive_priority_orders | archive_priority_orders | pass | matched expected rows |
| insert_select_cases | archive_via_cte | archive_via_cte | pass | matched expected rows |
| insert_select_cases | archive_without_column_list | archive_without_column_list | pass | matched expected rows |
| insert_select_cases | insert_grouped_customer_totals | insert_grouped_customer_totals | pass | matched expected rows |
| insert_select_cases | insert_select_mismatch_error | insert_select_mismatch_error | pass | matched expected error |
| insert_select_cases | insert_select_type_mismatch_error | insert_select_type_mismatch_error | pass | matched expected error |
| join_edge_cases | inner_join_duplicates | inner_join_duplicates | pass | matched expected rows |
| join_edge_cases | left_join_rollup | left_join_rollup | pass | matched expected rows |
| join_skew | customer_totals | customer_totals | pass | matched expected rows |
| null_semantics | group_sums | group_sums | pass | matched expected rows |
| null_semantics | null_filter | null_filter | pass | matched expected rows |
| order_group_cases | alphabetical_totals | alphabetical_totals | pass | matched expected rows |
| order_group_cases | totals_desc_tiebreak | totals_desc_tiebreak | pass | matched expected rows |
| perf_fixed | bucket_totals | bucket_totals | pass | matched expected rows |
| scalar_functions | cast_boolean | cast_boolean | pass | matched expected rows |
| scalar_functions | cast_numeric_and_text | cast_numeric_and_text | pass | matched expected rows |
| scalar_functions | current_date_filter | current_date_filter | pass | matched expected rows |
| scalar_functions | now_filter | now_filter | pass | matched expected rows |
| scalar_functions | null_helpers | null_helpers | pass | matched expected rows |
| scalar_functions | text_normalization | text_normalization | pass | matched expected rows |
| scalar_functions | unsupported_cast_uuid | unsupported_cast_uuid | pass | matched expected error |
| scalar_functions | unsupported_coalesce_arity | unsupported_coalesce_arity | pass | matched expected error |
| scalar_functions | unsupported_now_args | unsupported_now_args | pass | matched expected error |
| scalar_functions | window_function_error | window_function_error | pass | matched expected error |
| set_ops_cases | except_error | except_error | pass | matched expected error |
| set_ops_cases | intersect_error | intersect_error | pass | matched expected error |
| set_ops_cases | union_all_order_limit | union_all_order_limit | pass | matched expected rows |
| set_ops_cases | union_all_values | union_all_values | pass | matched expected rows |
| set_ops_cases | union_column_count_error | union_column_count_error | pass | matched expected error |
| set_ops_cases | union_distinct_values | union_distinct_values | pass | matched expected rows |
| set_ops_cases | union_type_mismatch_error | union_type_mismatch_error | pass | matched expected error |
| subquery_cases | in_subquery | in_subquery | pass | matched expected rows |
| subquery_cases | scalar_projection | scalar_projection | pass | matched expected rows |
| subquery_cases | scalar_subquery_multi_column_error | scalar_subquery_multi_column_error | pass | matched expected error |
| subquery_cases | scalar_subquery_multi_row_error | scalar_subquery_multi_row_error | pass | matched expected error |
| subquery_cases | subquery_in_function_error | subquery_in_function_error | pass | matched expected error |
| tiny_smoke | category_sales | category_sales | pass | matched expected rows |
| tiny_smoke | expected_error | expected_error | pass | matched expected error |
| tiny_smoke | select_items | select_items | pass | matched expected rows |
| write_path_smoke | delete_row | delete_row | pass | matched expected rows |
| write_path_smoke | insert_roundtrip | insert_roundtrip | pass | matched expected rows |
| write_path_smoke | update_balance | update_balance | pass | matched expected rows |

## differential Results

| Dataset | Group | Case | Status | Details |
|---|---|---|---|---|
| correlated_subquery_cases | correlated_exists | correlated_exists | pass | Snowdrop matched PostgreSQL |
| correlated_subquery_cases | correlated_having_scalar | correlated_having_scalar | pass | Snowdrop matched PostgreSQL |
| correlated_subquery_cases | correlated_in | correlated_in | pass | Snowdrop matched PostgreSQL |
| correlated_subquery_cases | correlated_scalar_projection | correlated_scalar_projection | pass | Snowdrop matched PostgreSQL |
| cte_cases | multi_cte_join | multi_cte_join | pass | Snowdrop matched PostgreSQL |
| cte_cases | single_cte_totals | single_cte_totals | pass | Snowdrop matched PostgreSQL |
| derived_table_cases | derived_from_grouped_source | derived_from_grouped_source | pass | Snowdrop matched PostgreSQL |
| derived_table_cases | derived_join_totals | derived_join_totals | pass | Snowdrop matched PostgreSQL |
| edge_cases | non_negative_sum | non_negative_sum | pass | Snowdrop matched PostgreSQL |
| edge_cases | null_notes | null_notes | pass | Snowdrop matched PostgreSQL |
| having_cases | having_scalar_subquery_threshold | having_scalar_subquery_threshold | pass | Snowdrop matched PostgreSQL |
| having_cases | order_by_group_expression | order_by_group_expression | pass | Snowdrop matched PostgreSQL |
| having_cases | sum_threshold | sum_threshold | pass | Snowdrop matched PostgreSQL |
| insert_select_cases | archive_priority_orders | archive_priority_orders | pass | Snowdrop matched PostgreSQL |
| insert_select_cases | archive_via_cte | archive_via_cte | pass | Snowdrop matched PostgreSQL |
| insert_select_cases | archive_without_column_list | archive_without_column_list | pass | Snowdrop matched PostgreSQL |
| insert_select_cases | insert_grouped_customer_totals | insert_grouped_customer_totals | pass | Snowdrop matched PostgreSQL |
| join_edge_cases | inner_join_duplicates | inner_join_duplicates | pass | Snowdrop matched PostgreSQL |
| join_edge_cases | left_join_rollup | left_join_rollup | pass | Snowdrop matched PostgreSQL |
| join_skew | customer_totals | customer_totals | pass | Snowdrop matched PostgreSQL |
| null_semantics | group_sums | group_sums | pass | Snowdrop matched PostgreSQL |
| null_semantics | null_filter | null_filter | pass | Snowdrop matched PostgreSQL |
| order_group_cases | alphabetical_totals | alphabetical_totals | pass | Snowdrop matched PostgreSQL |
| order_group_cases | totals_desc_tiebreak | totals_desc_tiebreak | pass | Snowdrop matched PostgreSQL |
| perf_fixed | bucket_totals | bucket_totals | pass | Snowdrop matched PostgreSQL |
| scalar_functions | cast_numeric_and_text | cast_numeric_and_text | pass | Snowdrop matched PostgreSQL |
| scalar_functions | current_date_filter | current_date_filter | pass | Snowdrop matched PostgreSQL |
| scalar_functions | now_filter | now_filter | pass | Snowdrop matched PostgreSQL |
| scalar_functions | null_helpers | null_helpers | pass | Snowdrop matched PostgreSQL |
| scalar_functions | text_normalization | text_normalization | pass | Snowdrop matched PostgreSQL |
| set_ops_cases | union_all_order_limit | union_all_order_limit | pass | Snowdrop matched PostgreSQL |
| set_ops_cases | union_all_values | union_all_values | pass | Snowdrop matched PostgreSQL |
| set_ops_cases | union_distinct_values | union_distinct_values | pass | Snowdrop matched PostgreSQL |
| subquery_cases | in_subquery | in_subquery | pass | Snowdrop matched PostgreSQL |
| subquery_cases | scalar_projection | scalar_projection | pass | Snowdrop matched PostgreSQL |
| tiny_smoke | category_sales | category_sales | pass | Snowdrop matched PostgreSQL |
| tiny_smoke | select_items | select_items | pass | Snowdrop matched PostgreSQL |
| write_path_smoke | delete_row | delete_row | pass | Snowdrop matched PostgreSQL |
| write_path_smoke | insert_roundtrip | insert_roundtrip | pass | Snowdrop matched PostgreSQL |
| write_path_smoke | update_balance | update_balance | pass | Snowdrop matched PostgreSQL |

## SQLLogicTest Results

| Dataset | Group | Case | Status | Details |
|---|---|---|---|---|
| tiny_smoke | basic.test | command_1 | pass | statement ok |
| tiny_smoke | basic.test | command_2 | pass | statement ok |
| tiny_smoke | basic.test | command_3 | pass | statement error |
| tiny_smoke | basic.test | command_4 | pass | query matched |
| cte_cases | cte.test | command_1 | pass | query matched |
| cte_cases | cte.test | command_2 | pass | query matched |
| cte_cases | cte.test | command_3 | pass | statement error |
| derived_table_cases | derived_tables.test | command_1 | pass | query matched |
| derived_table_cases | derived_tables.test | command_2 | pass | query matched |
| derived_table_cases | derived_tables.test | command_3 | pass | statement error |
| derived_table_cases | derived_tables.test | command_4 | pass | statement error |
| having_cases | having.test | command_1 | pass | query matched |
| having_cases | having.test | command_2 | pass | query matched |
| insert_select_cases | insert_select.test | command_1 | pass | statement ok |
| insert_select_cases | insert_select.test | command_2 | pass | query matched |
| insert_select_cases | insert_select_extended.test | command_1 | pass | query matched |
| insert_select_cases | insert_select_extended.test | command_2 | pass | query matched |
| insert_select_cases | insert_select_extended.test | command_3 | pass | statement error |
| insert_select_cases | insert_select_extended.test | command_4 | pass | statement error |
| null_semantics | nulls.test | command_1 | pass | query matched |
| null_semantics | nulls.test | command_2 | pass | query matched |
| order_group_cases | order_group.test | command_1 | pass | query matched |
| scalar_functions | scalar_functions.test | command_1 | pass | query matched |
| scalar_functions | scalar_functions.test | command_2 | pass | query matched |
| scalar_functions | scalar_functions.test | command_3 | pass | statement error |
| scalar_functions | scalar_functions.test | command_4 | pass | statement error |
| scalar_functions | scalar_functions.test | command_5 | pass | statement error |
| subquery_cases | subqueries_basic.test | command_1 | pass | query matched |
| subquery_cases | subqueries_basic.test | command_2 | pass | query matched |
| subquery_cases | subqueries_basic.test | command_3 | pass | statement error |
| subquery_cases | subqueries_basic.test | command_4 | pass | statement error |
| subquery_cases | subqueries_basic.test | command_5 | pass | statement error |
| correlated_subquery_cases | subqueries_correlated.test | command_1 | pass | query matched |
| correlated_subquery_cases | subqueries_correlated.test | command_2 | pass | query matched |
| correlated_subquery_cases | subqueries_correlated.test | command_3 | pass | query matched |
| correlated_subquery_cases | subqueries_correlated.test | command_4 | pass | query matched |
| set_ops_cases | union.test | command_1 | pass | query matched |
| set_ops_cases | union.test | command_2 | pass | query matched |
| set_ops_cases | union.test | command_3 | pass | query matched |
| set_ops_cases | union.test | command_4 | pass | statement error |
| set_ops_cases | union.test | command_5 | pass | statement error |
| set_ops_cases | union.test | command_6 | pass | statement error |
| set_ops_cases | union.test | command_7 | pass | statement error |
| write_path_smoke | write_path.test | command_1 | pass | statement ok |
| write_path_smoke | write_path.test | command_2 | pass | query matched |
| write_path_smoke | write_path.test | command_3 | pass | statement ok |
| write_path_smoke | write_path.test | command_4 | pass | query matched |

## TAP-style Results

| Dataset | Group | Case | Status | Details |
|---|---|---|---|---|
| cte_cases | cte_cases | single cte totals | pass | sql matched |
| cte_cases | cte_cases | multi cte join | pass | sql matched |
| cte_cases | cte_cases | recursive cte unsupported | pass | expected error |
| derived_table_cases | derived_table_cases | derived grouped source | pass | sql matched |
| derived_table_cases | derived_table_cases | derived join totals | pass | sql matched |
| derived_table_cases | derived_table_cases | derived table alias required | pass | expected error |
| having_cases | having_cases | having threshold | pass | sql matched |
| having_cases | having_cases | group order expression | pass | sql matched |
| insert_select_cases | insert_select_cases | insert select with expressions | pass | sql matched |
| insert_select_cases | insert_select_cases | insert select without column list | pass | sql matched |
| insert_select_cases | insert_select_cases | insert select via cte | pass | sql matched |
| insert_select_cases | insert_select_cases | insert grouped totals | pass | sql matched |
| insert_select_cases | insert_select_cases | insert select mismatch | pass | expected error |
| scalar_functions | scalar_functions | current_date filter | pass | sql matched |
| scalar_functions | scalar_functions | now filter | pass | sql matched |
| scalar_functions | scalar_functions | boolean cast | pass | sql matched |
| scalar_functions | scalar_functions | unsupported cast errors | pass | expected error |
| tiny_smoke | smoke | health endpoint | pass | health ok |
| tiny_smoke | smoke | metrics endpoint | pass | metrics ok |
| tiny_smoke | smoke | count items | pass | sql matched |
| tiny_smoke | smoke | invalid query errors | pass | expected error |
| write_path_smoke | write_path | insert roundtrip | pass | sql matched |
| write_path_smoke | write_path | update balance | pass | sql matched |
| write_path_smoke | write_path | delete row | pass | sql matched |

## PostgreSQL-style Results

| Dataset | Group | Case | Status | Details |
|---|---|---|---|---|
| tiny_smoke | basic | select_count | pass | matched expected output |
| tiny_smoke | basic | top_items | pass | matched expected output |
| correlated_subquery_cases | correlated_subquery_cases | correlated_exists | pass | matched expected output |
| correlated_subquery_cases | correlated_subquery_cases | correlated_having_scalar | pass | matched expected output |
| correlated_subquery_cases | correlated_subquery_cases | correlated_scalar_projection | pass | matched expected output |
| cte_cases | cte_cases | multi_cte_join | pass | matched expected output |
| cte_cases | cte_cases | recursive_cte_error | pass | matched expected error |
| cte_cases | cte_cases | single_cte_totals | pass | matched expected output |
| derived_table_cases | derived_table_cases | derived_from_grouped_source | pass | matched expected output |
| derived_table_cases | derived_table_cases | derived_join_totals | pass | matched expected output |
| derived_table_cases | derived_table_cases | derived_table_without_alias_error | pass | matched expected error |
| having_cases | having_cases | having_scalar_subquery_threshold | pass | matched expected output |
| having_cases | having_cases | order_by_group_expression | pass | matched expected output |
| having_cases | having_cases | sum_threshold | pass | matched expected output |
| insert_select_cases | insert_select_cases | archive_priority_orders | pass | matched expected output |
| insert_select_cases | insert_select_cases | archive_via_cte | pass | matched expected output |
| insert_select_cases | insert_select_cases | archive_without_column_list | pass | matched expected output |
| insert_select_cases | insert_select_cases | insert_grouped_customer_totals | pass | matched expected output |
| insert_select_cases | insert_select_cases | insert_select_mismatch_error | pass | matched expected error |
| insert_select_cases | insert_select_cases | insert_select_type_mismatch_error | pass | matched expected error |
| null_semantics | null_semantics | group_sums | pass | matched expected output |
| order_group_cases | order_group_cases | totals_desc | pass | matched expected output |
| scalar_functions | scalar_functions | current_date_filter | pass | matched expected output |
| scalar_functions | scalar_functions | null_helpers | pass | matched expected output |
| scalar_functions | scalar_functions | unsupported_cast_uuid | pass | matched expected error |
| scalar_functions | scalar_functions | window_function_error | pass | matched expected error |
| set_ops_cases | set_ops_cases | except_error | pass | matched expected error |
| set_ops_cases | set_ops_cases | intersect_error | pass | matched expected error |
| set_ops_cases | set_ops_cases | union_all_values | pass | matched expected output |
| set_ops_cases | set_ops_cases | union_column_count_error | pass | matched expected error |
| set_ops_cases | set_ops_cases | union_distinct_values | pass | matched expected output |
| set_ops_cases | set_ops_cases | union_type_mismatch_error | pass | matched expected error |
| subquery_cases | subquery_cases | in_subquery | pass | matched expected output |
| subquery_cases | subquery_cases | scalar_projection | pass | matched expected output |
| subquery_cases | subquery_cases | scalar_subquery_multi_column_error | pass | matched expected error |
| subquery_cases | subquery_cases | scalar_subquery_multi_row_error | pass | matched expected error |
| write_path_smoke | write_path_smoke | insert_then_select | pass | matched expected output |

Runs: `3`. Seed: `42`. Indexed phase: `enabled`.

## TPC-H Benchmark

Scale factor: 1 (~6000 lineitem rows)

| Query | Snowdrop (warm) | SD Indexed (warm) | PostgreSQL 16 | vs PG (baseline) |
|---|---|---|---|---|
| Q6 | 0.8ms | 4.2ms | 42.8ms | 55.7x faster |
| Q1 | 14.0ms | 17.9ms | 78.2ms | 5.6x faster |
| Q14 | 8.4ms | 7.9ms | 35.8ms | 4.3x faster |
| Q3 | 15.3ms | 15.3ms | 44.4ms | 2.9x faster |

### Query Descriptions

- **Q6**: Revenue from discounted shipments (SUM + 5-predicate WHERE)
- **Q1**: Pricing summary (GROUP BY + 7 aggregates)
- **Q14**: Promotion effect (2-table JOIN + CASE WHEN)
- **Q3**: Shipping priority (3-table JOIN + GROUP BY + ORDER BY + LIMIT)

### Index Configuration

- `idx_shipdate` on `lineitem(l_shipdate)`
- `idx_partkey` on `lineitem(l_partkey)`


## Conclusions

- Total cases: `206`
- Passed: `206`
- Failed: `0`
- Mismatched: `0`
- Skipped: `0`
