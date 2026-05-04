# Snowdrop Test Report

Generated at: `2026-05-04T23:22:28.579376399+00:00`

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
| edge_cases | edge_cases | yes | Exercise correctness around edge-case predicates and aggregates. | golden, differential |
| join_edge_cases | join_edge_cases | yes | Exercise join edge behavior for inner and left join semantics. | golden, differential |
| join_skew | join_skew | yes | Exercise join behavior and simple skew-sensitive aggregations. | golden, differential |
| null_semantics | null_semantics | yes | Exercise NULL semantics in filters, counts, sums, and ordering. | golden, differential, sqllogictest, postgres_style |
| order_group_cases | order_group_cases | yes | Exercise deterministic ORDER BY behavior after GROUP BY and aggregate aliases. | golden, differential, sqllogictest, postgres_style |
| perf_fixed | perf_fixed | yes | Stable non-synthetic fixture for future query micro-benchmarks. | golden, differential |
| scalar_functions | scalar_functions | yes | Validate COALESCE/NULLIF, text helpers, CAST, CURRENT_DATE, and NOW on a small readable dataset. | golden, differential, sqllogictest, tap, postgres_style |
| tiny_smoke | tiny_smoke | yes | Smoke correctness checks with a tiny readable fixture. | golden, differential, sqllogictest, tap, postgres_style |
| write_path_smoke | write_path_smoke | yes | Exercise the existing write-path contract with read-after-write validation. | golden, differential, sqllogictest, tap, postgres_style |

## Suite Summary By Dataset

| Dataset | Suite | Pass | Fail | Mismatch | Skip |
|---|---|---|---|---|---|
| edge_cases | differential | 2 | 0 | 0 | 0 |
| edge_cases | golden | 2 | 0 | 0 | 0 |
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
| scalar_functions | golden | 9 | 0 | 0 | 0 |
| scalar_functions | postgres-style | 3 | 0 | 0 | 0 |
| scalar_functions | sqllogictest | 4 | 0 | 0 | 0 |
| scalar_functions | tap | 4 | 0 | 0 | 0 |
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

## golden Results

| Dataset | Group | Case | Status | Details |
|---|---|---|---|---|
| edge_cases | non_negative_sum | non_negative_sum | pass | matched expected rows |
| edge_cases | null_notes | null_notes | pass | matched expected rows |
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
| tiny_smoke | category_sales | category_sales | pass | matched expected rows |
| tiny_smoke | expected_error | expected_error | pass | matched expected error |
| tiny_smoke | select_items | select_items | pass | matched expected rows |
| write_path_smoke | delete_row | delete_row | pass | matched expected rows |
| write_path_smoke | insert_roundtrip | insert_roundtrip | pass | matched expected rows |
| write_path_smoke | update_balance | update_balance | pass | matched expected rows |

## differential Results

| Dataset | Group | Case | Status | Details |
|---|---|---|---|---|
| edge_cases | non_negative_sum | non_negative_sum | pass | Snowdrop matched PostgreSQL |
| edge_cases | null_notes | null_notes | pass | Snowdrop matched PostgreSQL |
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
| null_semantics | nulls.test | command_1 | pass | query matched |
| null_semantics | nulls.test | command_2 | pass | query matched |
| order_group_cases | order_group.test | command_1 | pass | query matched |
| scalar_functions | scalar_functions.test | command_1 | pass | query matched |
| scalar_functions | scalar_functions.test | command_2 | pass | query matched |
| scalar_functions | scalar_functions.test | command_3 | pass | statement error |
| scalar_functions | scalar_functions.test | command_4 | pass | statement error |
| write_path_smoke | write_path.test | command_1 | pass | statement ok |
| write_path_smoke | write_path.test | command_2 | pass | query matched |
| write_path_smoke | write_path.test | command_3 | pass | statement ok |
| write_path_smoke | write_path.test | command_4 | pass | query matched |

## TAP-style Results

| Dataset | Group | Case | Status | Details |
|---|---|---|---|---|
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
| null_semantics | null_semantics | group_sums | pass | matched expected output |
| order_group_cases | order_group_cases | totals_desc | pass | matched expected output |
| scalar_functions | scalar_functions | current_date_filter | pass | matched expected output |
| scalar_functions | scalar_functions | null_helpers | pass | matched expected output |
| scalar_functions | scalar_functions | unsupported_cast_uuid | pass | matched expected error |
| write_path_smoke | write_path_smoke | insert_then_select | pass | matched expected output |

Runs: `3`. Seed: `42`. Indexed phase: `enabled`.

## TPC-H Benchmark

Scale factor: 1 (~6000 lineitem rows)

| Query | Snowdrop (warm) | SD Indexed (warm) | PostgreSQL 16 | vs PG (baseline) |
|---|---|---|---|---|
| Q6 | 0.8ms | 4.3ms | 35.6ms | 46.7x faster |
| Q1 | 14.0ms | 17.5ms | 77.5ms | 5.5x faster |
| Q14 | 8.6ms | 8.2ms | 35.7ms | 4.1x faster |
| Q3 | 17.1ms | 16.1ms | 56.2ms | 3.3x faster |

### Query Descriptions

- **Q6**: Revenue from discounted shipments (SUM + 5-predicate WHERE)
- **Q1**: Pricing summary (GROUP BY + 7 aggregates)
- **Q14**: Promotion effect (2-table JOIN + CASE WHEN)
- **Q3**: Shipping priority (3-table JOIN + GROUP BY + ORDER BY + LIMIT)

### Index Configuration

- `idx_shipdate` on `lineitem(l_shipdate)`
- `idx_partkey` on `lineitem(l_partkey)`


## Conclusions

- Total cases: `79`
- Passed: `79`
- Failed: `0`
- Mismatched: `0`
- Skipped: `0`
