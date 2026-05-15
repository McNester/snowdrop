# Snowdrop Test Report

Generated at: `2026-05-11T04:50:30.682802164+00:00`

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
| catalog_introspection | catalog_introspection | yes | Validate information_schema table_constraints, key_column_usage, and referential_constraints over named schema constraints. | golden, differential, sqllogictest, tap, postgres_style |
| check_constraints | check_constraints | yes | Validate CHECK acceptance and rejection for the current safe scalar expression subset. | golden, differential, sqllogictest, tap, postgres_style |
| composite_keys | composite_keys | yes | Validate tuple-style PRIMARY KEY semantics and duplicate rejection for multi-column primary keys. | golden, differential, sqllogictest, tap, postgres_style |
| correlated_subquery_cases | correlated_subquery_cases | yes | Exercise supported correlated subqueries while keeping lateral-style boundaries explicit. | golden, differential, sqllogictest, postgres_style |
| cte_cases | cte_cases | yes | Exercise single and multi-CTE relation execution while keeping recursive CTE unsupported. | golden, differential, sqllogictest, tap, postgres_style |
| datetime_semantics | datetime_semantics | yes | Validate typed temporal comparisons, ordering, casts, and a narrow practical EXTRACT subset. | golden, differential, sqllogictest, tap, postgres_style |
| defaults_and_updates | defaults_and_updates | yes | Validate DEFAULT application on INSERT, explicit overrides, and ALTER COLUMN SET/DROP DEFAULT behavior. | golden, differential, sqllogictest, tap, postgres_style |
| derived_table_cases | derived_table_cases | yes | Exercise derived tables as first-class relations and keep alias requirements explicit. | golden, differential, sqllogictest, tap, postgres_style |
| edge_cases | edge_cases | yes | Exercise correctness around edge-case predicates and aggregates. | golden, differential |
| foreign_key_cases | foreign_key_cases | yes | Validate parent/child referential checks, NO ACTION or RESTRICT behavior, and explicit unsupported FK actions. | golden, differential, sqllogictest, tap, postgres_style |
| having_cases | having_cases | yes | Exercise grouped filtering, aggregate aliases, and ORDER BY expressions on grouped output. | golden, differential, sqllogictest, tap, postgres_style |
| insert_select_cases | insert_select_cases | yes | Exercise INSERT ... SELECT over grouped, CTE-backed, and ordered relation sources. | golden, differential, sqllogictest, tap, postgres_style |
| join_edge_cases | join_edge_cases | yes | Exercise INNER, LEFT, RIGHT, FULL OUTER, and CROSS JOIN behavior for the practical Phase 6.5 subset. | golden, differential, sqllogictest, postgres_style, tap |
| join_skew | join_skew | yes | Exercise join behavior and simple skew-sensitive aggregations. | golden, differential |
| lateral_cases | lateral_cases | yes | Validate INNER/LEFT JOIN LATERAL over derived SELECT subqueries and keep narrower unsupported LATERAL forms explicit. | golden, differential, sqllogictest, tap, postgres_style |
| null_semantics | null_semantics | yes | Exercise NULL semantics in filters, counts, sums, and ordering. | golden, differential, sqllogictest, postgres_style |
| numeric_precision | numeric_precision | yes | Validate exact decimal arithmetic, predicates, ordering, and aggregate behavior without float-style drift. | golden, differential, sqllogictest, tap, postgres_style |
| order_group_cases | order_group_cases | yes | Exercise deterministic ORDER BY behavior after GROUP BY and aggregate aliases. | golden, differential, sqllogictest, postgres_style |
| pattern_matching | pattern_matching | yes | Validate richer wildcard handling, ESCAPE support, and case-insensitive ILIKE behavior. | golden, differential, sqllogictest, tap, postgres_style |
| perf_fixed | perf_fixed | yes | Stable non-synthetic fixture for future query micro-benchmarks. | golden, differential |
| recursive_cte_cases | recursive_cte_cases | yes | Validate UNION ALL recursive hierarchy walks plus explicit unsupported recursive boundaries such as recursion-cap overflow and UNION DISTINCT semantics. | golden, differential, sqllogictest, tap, postgres_style |
| scalar_functions | scalar_functions | yes | Validate COALESCE/NULLIF, text helpers, CAST, CURRENT_DATE, and NOW on a small readable dataset. | golden, differential, sqllogictest, tap, postgres_style |
| schema_constraints | schema_constraints | yes | Validate ALTER TABLE expansion, user-facing UNIQUE constraints, and related explicit unsupported boundaries. | golden, differential, sqllogictest, tap, postgres_style |
| set_ops_cases | set_ops_cases | yes | Exercise supported UNION paths and keep INTERSECT/EXCEPT boundaries explicit. | golden, differential, sqllogictest, postgres_style |
| subquery_cases | subquery_cases | yes | Exercise supported non-correlated subqueries and keep unsupported nested forms explicit. | golden, differential, sqllogictest, postgres_style |
| tiny_smoke | tiny_smoke | yes | Smoke correctness checks with a tiny readable fixture. | golden, differential, sqllogictest, tap, postgres_style |
| window_function_cases | window_function_cases | yes | Validate ranking, navigation, named windows, ROWS-frame aggregates, grouped/window coexistence, and the remaining explicit unsupported boundaries. | golden, differential, sqllogictest, tap, postgres_style |
| write_path_smoke | write_path_smoke | yes | Exercise the existing write-path contract with read-after-write validation. | golden, differential, sqllogictest, tap, postgres_style |

## Suite Summary By Dataset

| Dataset | Suite | Pass | Fail | Mismatch | Skip |
|---|---|---|---|---|---|
| catalog_introspection | differential | 7 | 0 | 0 | 0 |
| catalog_introspection | golden | 7 | 0 | 0 | 0 |
| catalog_introspection | postgres-style | 7 | 0 | 0 | 0 |
| catalog_introspection | sqllogictest | 5 | 0 | 0 | 0 |
| catalog_introspection | tap | 7 | 0 | 0 | 0 |
| check_constraints | differential | 1 | 0 | 0 | 0 |
| check_constraints | golden | 4 | 0 | 0 | 0 |
| check_constraints | postgres-style | 2 | 0 | 0 | 0 |
| check_constraints | sqllogictest | 3 | 0 | 0 | 0 |
| check_constraints | tap | 3 | 0 | 0 | 0 |
| composite_keys | differential | 2 | 0 | 0 | 0 |
| composite_keys | golden | 3 | 0 | 0 | 0 |
| composite_keys | postgres-style | 2 | 0 | 0 | 0 |
| composite_keys | sqllogictest | 2 | 0 | 0 | 0 |
| composite_keys | tap | 2 | 0 | 0 | 0 |
| correlated_subquery_cases | differential | 4 | 0 | 0 | 0 |
| correlated_subquery_cases | golden | 4 | 0 | 0 | 0 |
| correlated_subquery_cases | postgres-style | 3 | 0 | 0 | 0 |
| correlated_subquery_cases | sqllogictest | 4 | 0 | 0 | 0 |
| cte_cases | differential | 2 | 0 | 0 | 0 |
| cte_cases | golden | 3 | 0 | 0 | 0 |
| cte_cases | postgres-style | 3 | 0 | 0 | 0 |
| cte_cases | sqllogictest | 3 | 0 | 0 | 0 |
| cte_cases | tap | 3 | 0 | 0 | 0 |
| datetime_semantics | differential | 4 | 0 | 0 | 0 |
| datetime_semantics | golden | 5 | 0 | 0 | 0 |
| datetime_semantics | postgres-style | 3 | 0 | 0 | 0 |
| datetime_semantics | sqllogictest | 4 | 0 | 0 | 0 |
| datetime_semantics | tap | 3 | 0 | 0 | 0 |
| defaults_and_updates | differential | 3 | 0 | 0 | 0 |
| defaults_and_updates | golden | 3 | 0 | 0 | 0 |
| defaults_and_updates | postgres-style | 2 | 0 | 0 | 0 |
| defaults_and_updates | sqllogictest | 3 | 0 | 0 | 0 |
| defaults_and_updates | tap | 3 | 0 | 0 | 0 |
| derived_table_cases | differential | 2 | 0 | 0 | 0 |
| derived_table_cases | golden | 4 | 0 | 0 | 0 |
| derived_table_cases | postgres-style | 3 | 0 | 0 | 0 |
| derived_table_cases | sqllogictest | 4 | 0 | 0 | 0 |
| derived_table_cases | tap | 3 | 0 | 0 | 0 |
| edge_cases | differential | 2 | 0 | 0 | 0 |
| edge_cases | golden | 2 | 0 | 0 | 0 |
| foreign_key_cases | differential | 1 | 0 | 0 | 0 |
| foreign_key_cases | golden | 5 | 0 | 0 | 0 |
| foreign_key_cases | postgres-style | 2 | 0 | 0 | 0 |
| foreign_key_cases | sqllogictest | 3 | 0 | 0 | 0 |
| foreign_key_cases | tap | 3 | 0 | 0 | 0 |
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
| join_edge_cases | differential | 5 | 0 | 1 | 0 |
| join_edge_cases | golden | 5 | 0 | 1 | 0 |
| join_edge_cases | postgres-style | 2 | 0 | 0 | 0 |
| join_edge_cases | sqllogictest | 3 | 0 | 1 | 0 |
| join_edge_cases | tap | 2 | 0 | 1 | 0 |
| join_skew | differential | 1 | 0 | 0 | 0 |
| join_skew | golden | 1 | 0 | 0 | 0 |
| lateral_cases | differential | 3 | 0 | 0 | 0 |
| lateral_cases | golden | 4 | 0 | 0 | 0 |
| lateral_cases | postgres-style | 4 | 0 | 0 | 0 |
| lateral_cases | sqllogictest | 3 | 0 | 0 | 0 |
| lateral_cases | tap | 3 | 0 | 0 | 0 |
| null_semantics | differential | 2 | 0 | 0 | 0 |
| null_semantics | golden | 2 | 0 | 0 | 0 |
| null_semantics | postgres-style | 1 | 0 | 0 | 0 |
| null_semantics | sqllogictest | 2 | 0 | 0 | 0 |
| numeric_precision | differential | 4 | 0 | 0 | 0 |
| numeric_precision | golden | 4 | 0 | 0 | 0 |
| numeric_precision | postgres-style | 3 | 0 | 0 | 0 |
| numeric_precision | sqllogictest | 4 | 0 | 0 | 0 |
| numeric_precision | tap | 3 | 0 | 0 | 0 |
| order_group_cases | differential | 2 | 0 | 0 | 0 |
| order_group_cases | golden | 2 | 0 | 0 | 0 |
| order_group_cases | postgres-style | 1 | 0 | 0 | 0 |
| order_group_cases | sqllogictest | 1 | 0 | 0 | 0 |
| pattern_matching | differential | 3 | 0 | 0 | 0 |
| pattern_matching | golden | 3 | 0 | 0 | 0 |
| pattern_matching | postgres-style | 3 | 0 | 0 | 0 |
| pattern_matching | sqllogictest | 3 | 0 | 0 | 0 |
| pattern_matching | tap | 3 | 0 | 0 | 0 |
| perf_fixed | differential | 1 | 0 | 0 | 0 |
| perf_fixed | golden | 1 | 0 | 0 | 0 |
| recursive_cte_cases | differential | 2 | 0 | 0 | 0 |
| recursive_cte_cases | golden | 4 | 0 | 0 | 0 |
| recursive_cte_cases | postgres-style | 4 | 0 | 0 | 0 |
| recursive_cte_cases | sqllogictest | 4 | 0 | 0 | 0 |
| recursive_cte_cases | tap | 2 | 0 | 0 | 0 |
| scalar_functions | differential | 5 | 0 | 0 | 0 |
| scalar_functions | golden | 10 | 0 | 0 | 0 |
| scalar_functions | postgres-style | 5 | 0 | 0 | 0 |
| scalar_functions | sqllogictest | 6 | 0 | 0 | 0 |
| scalar_functions | tap | 4 | 0 | 0 | 0 |
| schema_constraints | differential | 5 | 0 | 0 | 0 |
| schema_constraints | golden | 10 | 0 | 0 | 0 |
| schema_constraints | postgres-style | 4 | 0 | 0 | 0 |
| schema_constraints | sqllogictest | 3 | 0 | 0 | 0 |
| schema_constraints | tap | 3 | 0 | 0 | 0 |
| set_ops_cases | differential | 5 | 0 | 0 | 0 |
| set_ops_cases | golden | 9 | 0 | 0 | 0 |
| set_ops_cases | postgres-style | 8 | 0 | 0 | 0 |
| set_ops_cases | sqllogictest | 9 | 0 | 0 | 0 |
| subquery_cases | differential | 2 | 0 | 0 | 0 |
| subquery_cases | golden | 5 | 0 | 0 | 0 |
| subquery_cases | postgres-style | 4 | 0 | 0 | 0 |
| subquery_cases | sqllogictest | 5 | 0 | 0 | 0 |
| tiny_smoke | differential | 2 | 0 | 0 | 0 |
| tiny_smoke | golden | 3 | 0 | 0 | 0 |
| tiny_smoke | postgres-style | 2 | 0 | 0 | 0 |
| tiny_smoke | sqllogictest | 4 | 0 | 0 | 0 |
| tiny_smoke | tap | 4 | 0 | 0 | 0 |
| window_function_cases | differential | 9 | 0 | 0 | 0 |
| window_function_cases | golden | 20 | 0 | 0 | 0 |
| window_function_cases | postgres-style | 20 | 0 | 0 | 0 |
| window_function_cases | sqllogictest | 20 | 0 | 0 | 0 |
| window_function_cases | tap | 6 | 0 | 0 | 0 |
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
| catalog_introspection | info_schema_key_column_usage | info_schema_key_column_usage | pass | matched expected rows |
| catalog_introspection | info_schema_referential_constraints | info_schema_referential_constraints | pass | matched expected rows |
| catalog_introspection | info_schema_table_constraints | info_schema_table_constraints | pass | matched expected rows |
| catalog_introspection | pg_catalog_pg_attribute | pg_catalog_pg_attribute | pass | matched expected rows |
| catalog_introspection | pg_catalog_pg_class | pg_catalog_pg_class | pass | matched expected rows |
| catalog_introspection | pg_catalog_pg_constraint | pg_catalog_pg_constraint | pass | matched expected rows |
| catalog_introspection | pg_catalog_pg_tables | pg_catalog_pg_tables | pass | matched expected rows |
| check_constraints | invalid_insert_error | invalid_insert_error | pass | matched expected error |
| check_constraints | invalid_update_error | invalid_update_error | pass | matched expected error |
| check_constraints | subquery_check_error | subquery_check_error | pass | matched expected error |
| check_constraints | valid_insert | valid_insert | pass | matched expected rows |
| composite_keys | composite_pk_duplicate_error | composite_pk_duplicate_error | pass | matched expected error |
| composite_keys | composite_pk_roundtrip | composite_pk_roundtrip | pass | matched expected rows |
| composite_keys | composite_pk_scan_lookup | composite_pk_scan_lookup | pass | matched expected rows |
| correlated_subquery_cases | correlated_exists | correlated_exists | pass | matched expected rows |
| correlated_subquery_cases | correlated_having_scalar | correlated_having_scalar | pass | matched expected rows |
| correlated_subquery_cases | correlated_in | correlated_in | pass | matched expected rows |
| correlated_subquery_cases | correlated_scalar_projection | correlated_scalar_projection | pass | matched expected rows |
| cte_cases | multi_cte_join | multi_cte_join | pass | matched expected rows |
| cte_cases | recursive_cte_error | recursive_cte_error | pass | matched expected error |
| cte_cases | single_cte_totals | single_cte_totals | pass | matched expected rows |
| datetime_semantics | date_predicate | date_predicate | pass | matched expected rows |
| datetime_semantics | extract_fields | extract_fields | pass | matched expected rows |
| datetime_semantics | time_extract_cast | time_extract_cast | pass | matched expected rows |
| datetime_semantics | timestamp_ordering | timestamp_ordering | pass | matched expected rows |
| datetime_semantics | unsupported_extract_timezone_error | unsupported_extract_timezone_error | pass | matched expected error |
| defaults_and_updates | alter_default_roundtrip | alter_default_roundtrip | pass | matched expected rows |
| defaults_and_updates | explicit_override_defaults | explicit_override_defaults | pass | matched expected rows |
| defaults_and_updates | omitted_defaults | omitted_defaults | pass | matched expected rows |
| derived_table_cases | derived_from_grouped_source | derived_from_grouped_source | pass | matched expected rows |
| derived_table_cases | derived_join_totals | derived_join_totals | pass | matched expected rows |
| derived_table_cases | derived_table_without_alias_error | derived_table_without_alias_error | pass | matched expected error |
| derived_table_cases | lateral_error | lateral_error | pass | matched expected error |
| edge_cases | non_negative_sum | non_negative_sum | pass | matched expected rows |
| edge_cases | null_notes | null_notes | pass | matched expected rows |
| foreign_key_cases | cascade_ddl_error | cascade_ddl_error | pass | matched expected error |
| foreign_key_cases | missing_parent_insert_error | missing_parent_insert_error | pass | matched expected error |
| foreign_key_cases | parent_delete_restrict_error | parent_delete_restrict_error | pass | matched expected error |
| foreign_key_cases | parent_pk_update_restrict_error | parent_pk_update_restrict_error | pass | matched expected error |
| foreign_key_cases | valid_child_insert | valid_child_insert | pass | matched expected rows |
| having_cases | having_scalar_subquery_threshold | having_scalar_subquery_threshold | pass | matched expected rows |
| having_cases | order_by_group_expression | order_by_group_expression | pass | matched expected rows |
| having_cases | sum_threshold | sum_threshold | pass | matched expected rows |
| insert_select_cases | archive_priority_orders | archive_priority_orders | pass | matched expected rows |
| insert_select_cases | archive_via_cte | archive_via_cte | pass | matched expected rows |
| insert_select_cases | archive_without_column_list | archive_without_column_list | pass | matched expected rows |
| insert_select_cases | insert_grouped_customer_totals | insert_grouped_customer_totals | pass | matched expected rows |
| insert_select_cases | insert_select_mismatch_error | insert_select_mismatch_error | pass | matched expected error |
| insert_select_cases | insert_select_type_mismatch_error | insert_select_type_mismatch_error | pass | matched expected error |
| join_edge_cases | cross_join_ticket_classes | cross_join_ticket_classes | pass | matched expected rows |
| join_edge_cases | full_outer_grouped_counts | full_outer_grouped_counts | mismatch | actual output diverged from expected output |
| join_edge_cases | full_outer_join_unmatched_both_sides | full_outer_join_unmatched_both_sides | pass | matched expected rows |
| join_edge_cases | inner_join_duplicates | inner_join_duplicates | pass | matched expected rows |
| join_edge_cases | left_join_rollup | left_join_rollup | pass | matched expected rows |
| join_edge_cases | right_join_unmatched_orders | right_join_unmatched_orders | pass | matched expected rows |
| join_skew | customer_totals | customer_totals | pass | matched expected rows |
| lateral_cases | inner_join_lateral_latest_paid | inner_join_lateral_latest_paid | pass | matched expected rows |
| lateral_cases | lateral_on_filter | lateral_on_filter | pass | matched expected rows |
| lateral_cases | lateral_values_error | lateral_values_error | pass | matched expected error |
| lateral_cases | left_join_lateral_latest_paid | left_join_lateral_latest_paid | pass | matched expected rows |
| null_semantics | group_sums | group_sums | pass | matched expected rows |
| null_semantics | null_filter | null_filter | pass | matched expected rows |
| numeric_precision | exact_decimal_aggregates | exact_decimal_aggregates | pass | matched expected rows |
| numeric_precision | exact_decimal_arithmetic | exact_decimal_arithmetic | pass | matched expected rows |
| numeric_precision | exact_decimal_ordering | exact_decimal_ordering | pass | matched expected rows |
| numeric_precision | exact_decimal_predicate | exact_decimal_predicate | pass | matched expected rows |
| order_group_cases | alphabetical_totals | alphabetical_totals | pass | matched expected rows |
| order_group_cases | totals_desc_tiebreak | totals_desc_tiebreak | pass | matched expected rows |
| pattern_matching | ilike_prefix | ilike_prefix | pass | matched expected rows |
| pattern_matching | like_escape | like_escape | pass | matched expected rows |
| pattern_matching | like_single_wildcard | like_single_wildcard | pass | matched expected rows |
| perf_fixed | bucket_totals | bucket_totals | pass | matched expected rows |
| recursive_cte_cases | recursive_iteration_limit_error | recursive_iteration_limit_error | pass | matched expected error |
| recursive_cte_cases | recursive_subtree_walk | recursive_subtree_walk | pass | matched expected rows |
| recursive_cte_cases | recursive_tree_walk | recursive_tree_walk | pass | matched expected rows |
| recursive_cte_cases | recursive_union_distinct_error | recursive_union_distinct_error | pass | matched expected error |
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
| schema_constraints | add_column_with_default | add_column_with_default | pass | matched expected rows |
| schema_constraints | drop_column | drop_column | pass | matched expected rows |
| schema_constraints | drop_default_error | drop_default_error | pass | matched expected error |
| schema_constraints | drop_not_null_allows_missing_value | drop_not_null_allows_missing_value | pass | matched expected rows |
| schema_constraints | multi_column_unique_duplicate_error | multi_column_unique_duplicate_error | pass | matched expected error |
| schema_constraints | multi_column_unique_roundtrip | multi_column_unique_roundtrip | pass | matched expected rows |
| schema_constraints | rename_column | rename_column | pass | matched expected rows |
| schema_constraints | set_not_null_error | set_not_null_error | pass | matched expected error |
| schema_constraints | unique_duplicate_insert_error | unique_duplicate_insert_error | pass | matched expected error |
| schema_constraints | unique_duplicate_update_error | unique_duplicate_update_error | pass | matched expected error |
| set_ops_cases | except_error | except_error | pass | matched expected error |
| set_ops_cases | except_values | except_values | pass | matched expected rows |
| set_ops_cases | intersect_error | intersect_error | pass | matched expected error |
| set_ops_cases | intersect_values | intersect_values | pass | matched expected rows |
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
| window_function_cases | filter_window_error | filter_window_error | pass | matched expected error |
| window_function_cases | forward_rows_frame | forward_rows_frame | pass | matched expected rows |
| window_function_cases | full_partition_rows_frame | full_partition_rows_frame | pass | matched expected rows |
| window_function_cases | grouped_window_rows | grouped_window_rows | pass | matched expected rows |
| window_function_cases | groups_frame_error | groups_frame_error | pass | matched expected error |
| window_function_cases | invalid_frame_bounds_error | invalid_frame_bounds_error | pass | matched expected error |
| window_function_cases | lag_lead_defaults | lag_lead_defaults | pass | matched expected rows |
| window_function_cases | named_window_override_error | named_window_override_error | pass | matched expected error |
| window_function_cases | named_window_reference | named_window_reference | pass | matched expected rows |
| window_function_cases | negative_frame_offset_error | negative_frame_offset_error | pass | matched expected error |
| window_function_cases | nested_window_error | nested_window_error | pass | matched expected error |
| window_function_cases | partition_aggregates | partition_aggregates | pass | matched expected rows |
| window_function_cases | range_frame_error | range_frame_error | pass | matched expected error |
| window_function_cases | rank_dense_rank_ties | rank_dense_rank_ties | pass | matched expected rows |
| window_function_cases | row_number_missing_order_error | row_number_missing_order_error | pass | matched expected error |
| window_function_cases | row_number_partition | row_number_partition | pass | matched expected rows |
| window_function_cases | running_rows_frame | running_rows_frame | pass | matched expected rows |
| window_function_cases | window_aggregate_order_missing_frame_error | window_aggregate_order_missing_frame_error | pass | matched expected error |
| window_function_cases | window_in_group_by_error | window_in_group_by_error | pass | matched expected error |
| window_function_cases | window_in_where_error | window_in_where_error | pass | matched expected error |
| write_path_smoke | delete_row | delete_row | pass | matched expected rows |
| write_path_smoke | insert_roundtrip | insert_roundtrip | pass | matched expected rows |
| write_path_smoke | update_balance | update_balance | pass | matched expected rows |

## differential Results

| Dataset | Group | Case | Status | Details |
|---|---|---|---|---|
| catalog_introspection | info_schema_key_column_usage | info_schema_key_column_usage | pass | Snowdrop matched PostgreSQL |
| catalog_introspection | info_schema_referential_constraints | info_schema_referential_constraints | pass | Snowdrop matched PostgreSQL |
| catalog_introspection | info_schema_table_constraints | info_schema_table_constraints | pass | Snowdrop matched PostgreSQL |
| catalog_introspection | pg_catalog_pg_attribute | pg_catalog_pg_attribute | pass | Snowdrop matched PostgreSQL |
| catalog_introspection | pg_catalog_pg_class | pg_catalog_pg_class | pass | Snowdrop matched PostgreSQL |
| catalog_introspection | pg_catalog_pg_constraint | pg_catalog_pg_constraint | pass | Snowdrop matched PostgreSQL |
| catalog_introspection | pg_catalog_pg_tables | pg_catalog_pg_tables | pass | Snowdrop matched PostgreSQL |
| check_constraints | valid_insert | valid_insert | pass | Snowdrop matched PostgreSQL |
| composite_keys | composite_pk_roundtrip | composite_pk_roundtrip | pass | Snowdrop matched PostgreSQL |
| composite_keys | composite_pk_scan_lookup | composite_pk_scan_lookup | pass | Snowdrop matched PostgreSQL |
| correlated_subquery_cases | correlated_exists | correlated_exists | pass | Snowdrop matched PostgreSQL |
| correlated_subquery_cases | correlated_having_scalar | correlated_having_scalar | pass | Snowdrop matched PostgreSQL |
| correlated_subquery_cases | correlated_in | correlated_in | pass | Snowdrop matched PostgreSQL |
| correlated_subquery_cases | correlated_scalar_projection | correlated_scalar_projection | pass | Snowdrop matched PostgreSQL |
| cte_cases | multi_cte_join | multi_cte_join | pass | Snowdrop matched PostgreSQL |
| cte_cases | single_cte_totals | single_cte_totals | pass | Snowdrop matched PostgreSQL |
| datetime_semantics | date_predicate | date_predicate | pass | Snowdrop matched PostgreSQL |
| datetime_semantics | extract_fields | extract_fields | pass | Snowdrop matched PostgreSQL |
| datetime_semantics | time_extract_cast | time_extract_cast | pass | Snowdrop matched PostgreSQL |
| datetime_semantics | timestamp_ordering | timestamp_ordering | pass | Snowdrop matched PostgreSQL |
| defaults_and_updates | alter_default_roundtrip | alter_default_roundtrip | pass | Snowdrop matched PostgreSQL |
| defaults_and_updates | explicit_override_defaults | explicit_override_defaults | pass | Snowdrop matched PostgreSQL |
| defaults_and_updates | omitted_defaults | omitted_defaults | pass | Snowdrop matched PostgreSQL |
| derived_table_cases | derived_from_grouped_source | derived_from_grouped_source | pass | Snowdrop matched PostgreSQL |
| derived_table_cases | derived_join_totals | derived_join_totals | pass | Snowdrop matched PostgreSQL |
| edge_cases | non_negative_sum | non_negative_sum | pass | Snowdrop matched PostgreSQL |
| edge_cases | null_notes | null_notes | pass | Snowdrop matched PostgreSQL |
| foreign_key_cases | valid_child_insert | valid_child_insert | pass | Snowdrop matched PostgreSQL |
| having_cases | having_scalar_subquery_threshold | having_scalar_subquery_threshold | pass | Snowdrop matched PostgreSQL |
| having_cases | order_by_group_expression | order_by_group_expression | pass | Snowdrop matched PostgreSQL |
| having_cases | sum_threshold | sum_threshold | pass | Snowdrop matched PostgreSQL |
| insert_select_cases | archive_priority_orders | archive_priority_orders | pass | Snowdrop matched PostgreSQL |
| insert_select_cases | archive_via_cte | archive_via_cte | pass | Snowdrop matched PostgreSQL |
| insert_select_cases | archive_without_column_list | archive_without_column_list | pass | Snowdrop matched PostgreSQL |
| insert_select_cases | insert_grouped_customer_totals | insert_grouped_customer_totals | pass | Snowdrop matched PostgreSQL |
| join_edge_cases | cross_join_ticket_classes | cross_join_ticket_classes | pass | Snowdrop matched PostgreSQL |
| join_edge_cases | full_outer_grouped_counts | full_outer_grouped_counts | mismatch | actual output diverged from expected output |
| join_edge_cases | full_outer_join_unmatched_both_sides | full_outer_join_unmatched_both_sides | pass | Snowdrop matched PostgreSQL |
| join_edge_cases | inner_join_duplicates | inner_join_duplicates | pass | Snowdrop matched PostgreSQL |
| join_edge_cases | left_join_rollup | left_join_rollup | pass | Snowdrop matched PostgreSQL |
| join_edge_cases | right_join_unmatched_orders | right_join_unmatched_orders | pass | Snowdrop matched PostgreSQL |
| join_skew | customer_totals | customer_totals | pass | Snowdrop matched PostgreSQL |
| lateral_cases | inner_join_lateral_latest_paid | inner_join_lateral_latest_paid | pass | Snowdrop matched PostgreSQL |
| lateral_cases | lateral_on_filter | lateral_on_filter | pass | Snowdrop matched PostgreSQL |
| lateral_cases | left_join_lateral_latest_paid | left_join_lateral_latest_paid | pass | Snowdrop matched PostgreSQL |
| null_semantics | group_sums | group_sums | pass | Snowdrop matched PostgreSQL |
| null_semantics | null_filter | null_filter | pass | Snowdrop matched PostgreSQL |
| numeric_precision | exact_decimal_aggregates | exact_decimal_aggregates | pass | Snowdrop matched PostgreSQL |
| numeric_precision | exact_decimal_arithmetic | exact_decimal_arithmetic | pass | Snowdrop matched PostgreSQL |
| numeric_precision | exact_decimal_ordering | exact_decimal_ordering | pass | Snowdrop matched PostgreSQL |
| numeric_precision | exact_decimal_predicate | exact_decimal_predicate | pass | Snowdrop matched PostgreSQL |
| order_group_cases | alphabetical_totals | alphabetical_totals | pass | Snowdrop matched PostgreSQL |
| order_group_cases | totals_desc_tiebreak | totals_desc_tiebreak | pass | Snowdrop matched PostgreSQL |
| pattern_matching | ilike_prefix | ilike_prefix | pass | Snowdrop matched PostgreSQL |
| pattern_matching | like_escape | like_escape | pass | Snowdrop matched PostgreSQL |
| pattern_matching | like_single_wildcard | like_single_wildcard | pass | Snowdrop matched PostgreSQL |
| perf_fixed | bucket_totals | bucket_totals | pass | Snowdrop matched PostgreSQL |
| recursive_cte_cases | recursive_subtree_walk | recursive_subtree_walk | pass | Snowdrop matched PostgreSQL |
| recursive_cte_cases | recursive_tree_walk | recursive_tree_walk | pass | Snowdrop matched PostgreSQL |
| scalar_functions | cast_numeric_and_text | cast_numeric_and_text | pass | Snowdrop matched PostgreSQL |
| scalar_functions | current_date_filter | current_date_filter | pass | Snowdrop matched PostgreSQL |
| scalar_functions | now_filter | now_filter | pass | Snowdrop matched PostgreSQL |
| scalar_functions | null_helpers | null_helpers | pass | Snowdrop matched PostgreSQL |
| scalar_functions | text_normalization | text_normalization | pass | Snowdrop matched PostgreSQL |
| schema_constraints | add_column_with_default | add_column_with_default | pass | Snowdrop matched PostgreSQL |
| schema_constraints | drop_column | drop_column | pass | Snowdrop matched PostgreSQL |
| schema_constraints | drop_not_null_allows_missing_value | drop_not_null_allows_missing_value | pass | Snowdrop matched PostgreSQL |
| schema_constraints | multi_column_unique_roundtrip | multi_column_unique_roundtrip | pass | Snowdrop matched PostgreSQL |
| schema_constraints | rename_column | rename_column | pass | Snowdrop matched PostgreSQL |
| set_ops_cases | except_values | except_values | pass | Snowdrop matched PostgreSQL |
| set_ops_cases | intersect_values | intersect_values | pass | Snowdrop matched PostgreSQL |
| set_ops_cases | union_all_order_limit | union_all_order_limit | pass | Snowdrop matched PostgreSQL |
| set_ops_cases | union_all_values | union_all_values | pass | Snowdrop matched PostgreSQL |
| set_ops_cases | union_distinct_values | union_distinct_values | pass | Snowdrop matched PostgreSQL |
| subquery_cases | in_subquery | in_subquery | pass | Snowdrop matched PostgreSQL |
| subquery_cases | scalar_projection | scalar_projection | pass | Snowdrop matched PostgreSQL |
| tiny_smoke | category_sales | category_sales | pass | Snowdrop matched PostgreSQL |
| tiny_smoke | select_items | select_items | pass | Snowdrop matched PostgreSQL |
| window_function_cases | forward_rows_frame | forward_rows_frame | pass | Snowdrop matched PostgreSQL |
| window_function_cases | full_partition_rows_frame | full_partition_rows_frame | pass | Snowdrop matched PostgreSQL |
| window_function_cases | grouped_window_rows | grouped_window_rows | pass | Snowdrop matched PostgreSQL |
| window_function_cases | lag_lead_defaults | lag_lead_defaults | pass | Snowdrop matched PostgreSQL |
| window_function_cases | named_window_reference | named_window_reference | pass | Snowdrop matched PostgreSQL |
| window_function_cases | partition_aggregates | partition_aggregates | pass | Snowdrop matched PostgreSQL |
| window_function_cases | rank_dense_rank_ties | rank_dense_rank_ties | pass | Snowdrop matched PostgreSQL |
| window_function_cases | row_number_partition | row_number_partition | pass | Snowdrop matched PostgreSQL |
| window_function_cases | running_rows_frame | running_rows_frame | pass | Snowdrop matched PostgreSQL |
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
| catalog_introspection | catalog_introspection.test | command_1 | pass | query matched |
| catalog_introspection | catalog_introspection.test | command_2 | pass | query matched |
| catalog_introspection | catalog_introspection.test | command_3 | pass | query matched |
| catalog_introspection | catalog_introspection.test | command_4 | pass | query matched |
| catalog_introspection | catalog_introspection.test | command_5 | pass | query matched |
| check_constraints | check_constraints.test | command_1 | pass | query matched |
| check_constraints | check_constraints.test | command_2 | pass | statement error |
| check_constraints | check_constraints.test | command_3 | pass | statement error |
| composite_keys | composite_keys.test | command_1 | pass | query matched |
| composite_keys | composite_keys.test | command_2 | pass | statement error |
| cte_cases | cte.test | command_1 | pass | query matched |
| cte_cases | cte.test | command_2 | pass | query matched |
| cte_cases | cte.test | command_3 | pass | statement error |
| datetime_semantics | datetime_semantics.test | command_1 | pass | query matched |
| datetime_semantics | datetime_semantics.test | command_2 | pass | query matched |
| datetime_semantics | datetime_semantics.test | command_3 | pass | query matched |
| datetime_semantics | datetime_semantics.test | command_4 | pass | statement error |
| defaults_and_updates | defaults_and_updates.test | command_1 | pass | query matched |
| defaults_and_updates | defaults_and_updates.test | command_2 | pass | query matched |
| defaults_and_updates | defaults_and_updates.test | command_3 | pass | query matched |
| derived_table_cases | derived_tables.test | command_1 | pass | query matched |
| derived_table_cases | derived_tables.test | command_2 | pass | query matched |
| derived_table_cases | derived_tables.test | command_3 | pass | statement error |
| derived_table_cases | derived_tables.test | command_4 | pass | statement error |
| foreign_key_cases | foreign_key_cases.test | command_1 | pass | query matched |
| foreign_key_cases | foreign_key_cases.test | command_2 | pass | statement error |
| foreign_key_cases | foreign_key_cases.test | command_3 | pass | statement error |
| having_cases | having.test | command_1 | pass | query matched |
| having_cases | having.test | command_2 | pass | query matched |
| insert_select_cases | insert_select.test | command_1 | pass | statement ok |
| insert_select_cases | insert_select.test | command_2 | pass | query matched |
| insert_select_cases | insert_select_extended.test | command_1 | pass | query matched |
| insert_select_cases | insert_select_extended.test | command_2 | pass | query matched |
| insert_select_cases | insert_select_extended.test | command_3 | pass | statement error |
| insert_select_cases | insert_select_extended.test | command_4 | pass | statement error |
| join_edge_cases | join_types.test | command_1 | pass | query matched |
| join_edge_cases | join_types.test | command_2 | pass | query matched |
| join_edge_cases | join_types.test | command_3 | pass | query matched |
| join_edge_cases | join_types.test | command_4 | mismatch | actual output diverged from expected output |
| lateral_cases | lateral.test | command_1 | pass | query matched |
| lateral_cases | lateral.test | command_2 | pass | query matched |
| lateral_cases | lateral.test | command_3 | pass | statement error |
| null_semantics | nulls.test | command_1 | pass | query matched |
| null_semantics | nulls.test | command_2 | pass | query matched |
| numeric_precision | numeric_precision.test | command_1 | pass | query matched |
| numeric_precision | numeric_precision.test | command_2 | pass | query matched |
| numeric_precision | numeric_precision.test | command_3 | pass | query matched |
| numeric_precision | numeric_precision.test | command_4 | pass | query matched |
| order_group_cases | order_group.test | command_1 | pass | query matched |
| pattern_matching | pattern_matching.test | command_1 | pass | query matched |
| pattern_matching | pattern_matching.test | command_2 | pass | query matched |
| pattern_matching | pattern_matching.test | command_3 | pass | query matched |
| recursive_cte_cases | recursive_cte.test | command_1 | pass | query matched |
| recursive_cte_cases | recursive_cte.test | command_2 | pass | query matched |
| recursive_cte_cases | recursive_cte.test | command_3 | pass | statement error |
| recursive_cte_cases | recursive_cte.test | command_4 | pass | statement error |
| scalar_functions | scalar_functions.test | command_1 | pass | query matched |
| scalar_functions | scalar_functions.test | command_2 | pass | query matched |
| scalar_functions | scalar_functions.test | command_3 | pass | query matched |
| scalar_functions | scalar_functions.test | command_4 | pass | statement error |
| scalar_functions | scalar_functions.test | command_5 | pass | statement error |
| scalar_functions | scalar_functions.test | command_6 | pass | statement error |
| schema_constraints | schema_constraints.test | command_1 | pass | query matched |
| schema_constraints | schema_constraints.test | command_2 | pass | query matched |
| schema_constraints | schema_constraints.test | command_3 | pass | statement error |
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
| set_ops_cases | union.test | command_4 | pass | query matched |
| set_ops_cases | union.test | command_5 | pass | query matched |
| set_ops_cases | union.test | command_6 | pass | statement error |
| set_ops_cases | union.test | command_7 | pass | statement error |
| set_ops_cases | union.test | command_8 | pass | statement error |
| set_ops_cases | union.test | command_9 | pass | statement error |
| window_function_cases | window_functions.test | command_1 | pass | query matched |
| window_function_cases | window_functions.test | command_2 | pass | query matched |
| window_function_cases | window_functions.test | command_3 | pass | query matched |
| window_function_cases | window_functions.test | command_4 | pass | query matched |
| window_function_cases | window_functions.test | command_5 | pass | query matched |
| window_function_cases | window_functions.test | command_6 | pass | query matched |
| window_function_cases | window_functions.test | command_7 | pass | query matched |
| window_function_cases | window_functions.test | command_8 | pass | query matched |
| window_function_cases | window_functions.test | command_9 | pass | query matched |
| window_function_cases | window_functions.test | command_10 | pass | statement error |
| window_function_cases | window_functions.test | command_11 | pass | statement error |
| window_function_cases | window_functions.test | command_12 | pass | statement error |
| window_function_cases | window_functions.test | command_13 | pass | statement error |
| window_function_cases | window_functions.test | command_14 | pass | statement error |
| window_function_cases | window_functions.test | command_15 | pass | statement error |
| window_function_cases | window_functions.test | command_16 | pass | statement error |
| window_function_cases | window_functions.test | command_17 | pass | statement error |
| window_function_cases | window_functions.test | command_18 | pass | statement error |
| window_function_cases | window_functions.test | command_19 | pass | statement error |
| window_function_cases | window_functions.test | command_20 | pass | statement error |
| write_path_smoke | write_path.test | command_1 | pass | statement ok |
| write_path_smoke | write_path.test | command_2 | pass | query matched |
| write_path_smoke | write_path.test | command_3 | pass | statement ok |
| write_path_smoke | write_path.test | command_4 | pass | query matched |

## TAP-style Results

| Dataset | Group | Case | Status | Details |
|---|---|---|---|---|
| catalog_introspection | catalog_introspection | table constraints introspection | pass | sql matched |
| catalog_introspection | catalog_introspection | key column usage introspection | pass | sql matched |
| catalog_introspection | catalog_introspection | referential constraints introspection | pass | sql matched |
| catalog_introspection | catalog_introspection_pg_catalog | pg tables | pass | sql matched |
| catalog_introspection | catalog_introspection_pg_catalog | pg class | pass | sql matched |
| catalog_introspection | catalog_introspection_pg_catalog | pg attribute | pass | sql matched |
| catalog_introspection | catalog_introspection_pg_catalog | pg constraint | pass | sql matched |
| check_constraints | check_constraints | valid insert | pass | sql matched |
| check_constraints | check_constraints | invalid insert rejected | pass | expected error |
| check_constraints | check_constraints | invalid update rejected | pass | expected error |
| composite_keys | composite_keys | composite pk scan lookup | pass | sql matched |
| composite_keys | composite_keys | composite pk duplicate rejected | pass | expected error |
| cte_cases | cte_cases | single cte totals | pass | sql matched |
| cte_cases | cte_cases | multi cte join | pass | sql matched |
| cte_cases | cte_cases | recursive cte unsupported | pass | expected error |
| datetime_semantics | datetime_semantics | date predicate | pass | sql matched |
| datetime_semantics | datetime_semantics | extract fields | pass | sql matched |
| datetime_semantics | datetime_semantics | timestamp ordering | pass | sql matched |
| defaults_and_updates | defaults_and_updates | omitted defaults | pass | sql matched |
| defaults_and_updates | defaults_and_updates | explicit override defaults | pass | sql matched |
| defaults_and_updates | defaults_and_updates | alter default roundtrip | pass | sql matched |
| derived_table_cases | derived_table_cases | derived grouped source | pass | sql matched |
| derived_table_cases | derived_table_cases | derived join totals | pass | sql matched |
| derived_table_cases | derived_table_cases | derived table alias required | pass | expected error |
| foreign_key_cases | foreign_key_cases | valid child insert | pass | sql matched |
| foreign_key_cases | foreign_key_cases | missing parent rejected | pass | expected error |
| foreign_key_cases | foreign_key_cases | parent delete restricted | pass | expected error |
| having_cases | having_cases | having threshold | pass | sql matched |
| having_cases | having_cases | group order expression | pass | sql matched |
| insert_select_cases | insert_select_cases | insert select with expressions | pass | sql matched |
| insert_select_cases | insert_select_cases | insert select without column list | pass | sql matched |
| insert_select_cases | insert_select_cases | insert select via cte | pass | sql matched |
| insert_select_cases | insert_select_cases | insert grouped totals | pass | sql matched |
| insert_select_cases | insert_select_cases | insert select mismatch | pass | expected error |
| join_edge_cases | join_edge_cases | cross join ticket classes | pass | sql matched |
| join_edge_cases | join_edge_cases | right join unmatched orders | pass | sql matched |
| join_edge_cases | join_edge_cases | full outer grouped counts | mismatch | actual output diverged from expected output |
| lateral_cases | lateral_cases | inner join lateral latest paid | pass | sql matched |
| lateral_cases | lateral_cases | left join lateral latest paid | pass | sql matched |
| lateral_cases | lateral_cases | lateral values unsupported | pass | expected error |
| numeric_precision | numeric_precision | exact decimal arithmetic | pass | sql matched |
| numeric_precision | numeric_precision | exact decimal predicate | pass | sql matched |
| numeric_precision | numeric_precision | exact decimal aggregates | pass | sql matched |
| pattern_matching | pattern_matching | like single wildcard | pass | sql matched |
| pattern_matching | pattern_matching | like escape | pass | sql matched |
| pattern_matching | pattern_matching | ilike prefix | pass | sql matched |
| recursive_cte_cases | recursive_cte_cases | recursive_tree_walk | pass | sql matched |
| recursive_cte_cases | recursive_cte_cases | recursive_iteration_limit_error | pass | expected error |
| scalar_functions | scalar_functions | current_date filter | pass | sql matched |
| scalar_functions | scalar_functions | now filter | pass | sql matched |
| scalar_functions | scalar_functions | boolean cast | pass | sql matched |
| scalar_functions | scalar_functions | unsupported cast errors | pass | expected error |
| schema_constraints | schema_constraints | add column with default | pass | sql matched |
| schema_constraints | schema_constraints | rename column | pass | sql matched |
| schema_constraints | schema_constraints | unique duplicate rejected | pass | expected error |
| tiny_smoke | smoke | health endpoint | pass | health ok |
| tiny_smoke | smoke | metrics endpoint | pass | metrics ok |
| tiny_smoke | smoke | count items | pass | sql matched |
| tiny_smoke | smoke | invalid query errors | pass | expected error |
| window_function_cases | window_function_cases | row number per partition | pass | sql matched |
| window_function_cases | window_function_cases | lag lead defaults | pass | sql matched |
| window_function_cases | window_function_cases | named window reference | pass | sql matched |
| window_function_cases | window_function_cases | running rows frame | pass | sql matched |
| window_function_cases | window_function_cases | grouped rows plus window | pass | sql matched |
| window_function_cases | window_function_cases | range frame unsupported | pass | expected error |
| write_path_smoke | write_path | insert roundtrip | pass | sql matched |
| write_path_smoke | write_path | update balance | pass | sql matched |
| write_path_smoke | write_path | delete row | pass | sql matched |

## PostgreSQL-style Results

| Dataset | Group | Case | Status | Details |
|---|---|---|---|---|
| tiny_smoke | basic | select_count | pass | matched expected output |
| tiny_smoke | basic | top_items | pass | matched expected output |
| catalog_introspection | catalog_introspection | info_schema_key_column_usage | pass | matched expected output |
| catalog_introspection | catalog_introspection | info_schema_referential_constraints | pass | matched expected output |
| catalog_introspection | catalog_introspection | info_schema_table_constraints | pass | matched expected output |
| catalog_introspection | catalog_introspection | pg_catalog_pg_attribute | pass | matched expected output |
| catalog_introspection | catalog_introspection | pg_catalog_pg_class | pass | matched expected output |
| catalog_introspection | catalog_introspection | pg_catalog_pg_constraint | pass | matched expected output |
| catalog_introspection | catalog_introspection | pg_catalog_pg_tables | pass | matched expected output |
| check_constraints | check_constraints | invalid_insert_error | pass | matched expected error |
| check_constraints | check_constraints | valid_insert | pass | matched expected output |
| composite_keys | composite_keys | composite_pk_duplicate_error | pass | matched expected error |
| composite_keys | composite_keys | composite_pk_scan_lookup | pass | matched expected output |
| correlated_subquery_cases | correlated_subquery_cases | correlated_exists | pass | matched expected output |
| correlated_subquery_cases | correlated_subquery_cases | correlated_having_scalar | pass | matched expected output |
| correlated_subquery_cases | correlated_subquery_cases | correlated_scalar_projection | pass | matched expected output |
| cte_cases | cte_cases | multi_cte_join | pass | matched expected output |
| cte_cases | cte_cases | recursive_cte_error | pass | matched expected error |
| cte_cases | cte_cases | single_cte_totals | pass | matched expected output |
| datetime_semantics | datetime_semantics | date_predicate | pass | matched expected output |
| datetime_semantics | datetime_semantics | extract_fields | pass | matched expected output |
| datetime_semantics | datetime_semantics | timestamp_ordering | pass | matched expected output |
| defaults_and_updates | defaults_and_updates | alter_default_roundtrip | pass | matched expected output |
| defaults_and_updates | defaults_and_updates | omitted_defaults | pass | matched expected output |
| derived_table_cases | derived_table_cases | derived_from_grouped_source | pass | matched expected output |
| derived_table_cases | derived_table_cases | derived_join_totals | pass | matched expected output |
| derived_table_cases | derived_table_cases | derived_table_without_alias_error | pass | matched expected error |
| foreign_key_cases | foreign_key_cases | parent_delete_restrict_error | pass | matched expected error |
| foreign_key_cases | foreign_key_cases | valid_child_insert | pass | matched expected output |
| having_cases | having_cases | having_scalar_subquery_threshold | pass | matched expected output |
| having_cases | having_cases | order_by_group_expression | pass | matched expected output |
| having_cases | having_cases | sum_threshold | pass | matched expected output |
| insert_select_cases | insert_select_cases | archive_priority_orders | pass | matched expected output |
| insert_select_cases | insert_select_cases | archive_via_cte | pass | matched expected output |
| insert_select_cases | insert_select_cases | archive_without_column_list | pass | matched expected output |
| insert_select_cases | insert_select_cases | insert_grouped_customer_totals | pass | matched expected output |
| insert_select_cases | insert_select_cases | insert_select_mismatch_error | pass | matched expected error |
| insert_select_cases | insert_select_cases | insert_select_type_mismatch_error | pass | matched expected error |
| join_edge_cases | join_edge_cases | cross_join_ticket_classes | pass | matched expected output |
| join_edge_cases | join_edge_cases | full_outer_join_unmatched_both_sides | pass | matched expected output |
| lateral_cases | lateral_cases | inner_join_lateral_latest_paid | pass | matched expected output |
| lateral_cases | lateral_cases | lateral_on_filter | pass | matched expected output |
| lateral_cases | lateral_cases | lateral_values_error | pass | matched expected error |
| lateral_cases | lateral_cases | left_join_lateral_latest_paid | pass | matched expected output |
| null_semantics | null_semantics | group_sums | pass | matched expected output |
| numeric_precision | numeric_precision | exact_decimal_aggregates | pass | matched expected output |
| numeric_precision | numeric_precision | exact_decimal_arithmetic | pass | matched expected output |
| numeric_precision | numeric_precision | exact_decimal_ordering | pass | matched expected output |
| order_group_cases | order_group_cases | totals_desc | pass | matched expected output |
| pattern_matching | pattern_matching | ilike_prefix | pass | matched expected output |
| pattern_matching | pattern_matching | like_escape | pass | matched expected output |
| pattern_matching | pattern_matching | like_single_wildcard | pass | matched expected output |
| recursive_cte_cases | recursive_cte_cases | recursive_iteration_limit_error | pass | matched expected error |
| recursive_cte_cases | recursive_cte_cases | recursive_subtree_walk | pass | matched expected output |
| recursive_cte_cases | recursive_cte_cases | recursive_tree_walk | pass | matched expected output |
| recursive_cte_cases | recursive_cte_cases | recursive_union_distinct_error | pass | matched expected error |
| scalar_functions | scalar_functions | current_date_filter | pass | matched expected output |
| scalar_functions | scalar_functions | null_helpers | pass | matched expected output |
| scalar_functions | scalar_functions | select_without_from | pass | matched expected output |
| scalar_functions | scalar_functions | unsupported_cast_uuid | pass | matched expected error |
| scalar_functions | scalar_functions | window_function_error | pass | matched expected error |
| schema_constraints | schema_constraints | add_column_with_default | pass | matched expected output |
| schema_constraints | schema_constraints | drop_column | pass | matched expected output |
| schema_constraints | schema_constraints | rename_column | pass | matched expected output |
| schema_constraints | schema_constraints | unique_duplicate_insert_error | pass | matched expected error |
| set_ops_cases | set_ops_cases | except_error | pass | matched expected error |
| set_ops_cases | set_ops_cases | except_values | pass | matched expected output |
| set_ops_cases | set_ops_cases | intersect_error | pass | matched expected error |
| set_ops_cases | set_ops_cases | intersect_values | pass | matched expected output |
| set_ops_cases | set_ops_cases | union_all_values | pass | matched expected output |
| set_ops_cases | set_ops_cases | union_column_count_error | pass | matched expected error |
| set_ops_cases | set_ops_cases | union_distinct_values | pass | matched expected output |
| set_ops_cases | set_ops_cases | union_type_mismatch_error | pass | matched expected error |
| subquery_cases | subquery_cases | in_subquery | pass | matched expected output |
| subquery_cases | subquery_cases | scalar_projection | pass | matched expected output |
| subquery_cases | subquery_cases | scalar_subquery_multi_column_error | pass | matched expected error |
| subquery_cases | subquery_cases | scalar_subquery_multi_row_error | pass | matched expected error |
| window_function_cases | window_function_cases | filter_window_error | pass | matched expected error |
| window_function_cases | window_function_cases | forward_rows_frame | pass | matched expected output |
| window_function_cases | window_function_cases | full_partition_rows_frame | pass | matched expected output |
| window_function_cases | window_function_cases | grouped_window_rows | pass | matched expected output |
| window_function_cases | window_function_cases | groups_frame_error | pass | matched expected error |
| window_function_cases | window_function_cases | invalid_frame_bounds_error | pass | matched expected error |
| window_function_cases | window_function_cases | lag_lead_defaults | pass | matched expected output |
| window_function_cases | window_function_cases | named_window_override_error | pass | matched expected error |
| window_function_cases | window_function_cases | named_window_reference | pass | matched expected output |
| window_function_cases | window_function_cases | negative_frame_offset_error | pass | matched expected error |
| window_function_cases | window_function_cases | nested_window_error | pass | matched expected error |
| window_function_cases | window_function_cases | partition_aggregates | pass | matched expected output |
| window_function_cases | window_function_cases | range_frame_error | pass | matched expected error |
| window_function_cases | window_function_cases | rank_dense_rank_ties | pass | matched expected output |
| window_function_cases | window_function_cases | row_number_missing_order_error | pass | matched expected error |
| window_function_cases | window_function_cases | row_number_partition | pass | matched expected output |
| window_function_cases | window_function_cases | running_rows_frame | pass | matched expected output |
| window_function_cases | window_function_cases | window_aggregate_order_missing_frame_error | pass | matched expected error |
| window_function_cases | window_function_cases | window_in_group_by_error | pass | matched expected error |
| window_function_cases | window_function_cases | window_in_where_error | pass | matched expected error |
| write_path_smoke | write_path_smoke | insert_then_select | pass | matched expected output |

## Conclusions

- Total cases: `503`
- Passed: `499`
- Failed: `0`
- Mismatched: `4`
- Skipped: `0`
