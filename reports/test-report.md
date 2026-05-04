# Snowdrop Test Report

Generated at: `2026-05-04T21:44:43.605066221+00:00`

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
| write_path_smoke | write_path.test | command_1 | pass | statement ok |
| write_path_smoke | write_path.test | command_2 | pass | query matched |
| write_path_smoke | write_path.test | command_3 | pass | statement ok |
| write_path_smoke | write_path.test | command_4 | pass | query matched |

## TAP-style Results

| Dataset | Group | Case | Status | Details |
|---|---|---|---|---|
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
| write_path_smoke | write_path_smoke | insert_then_select | pass | matched expected output |

Runs: `3`. Seed: `42`. Indexed phase: `enabled`.

## TPC-H Benchmark

Scale factor: 1 (~6000 lineitem rows)

| Query | Snowdrop (warm) | SD Indexed (warm) | PostgreSQL 16 | vs PG (baseline) |
|---|---|---|---|---|
| Q6 | 0.7ms | 4.5ms | 35.7ms | 48.1x faster |
| Q1 | 13.8ms | 17.3ms | 77.8ms | 5.6x faster |
| Q14 | 8.3ms | 8.3ms | 40.8ms | 4.9x faster |
| Q3 | 19.3ms | 15.9ms | 44.5ms | 2.3x faster |

### Query Descriptions

- **Q6**: Revenue from discounted shipments (SUM + 5-predicate WHERE)
- **Q1**: Pricing summary (GROUP BY + 7 aggregates)
- **Q14**: Promotion effect (2-table JOIN + CASE WHEN)
- **Q3**: Shipping priority (3-table JOIN + GROUP BY + ORDER BY + LIMIT)

### Index Configuration

- `idx_shipdate` on `lineitem(l_shipdate)`
- `idx_partkey` on `lineitem(l_partkey)`


## Conclusions

- Total cases: `54`
- Passed: `54`
- Failed: `0`
- Mismatched: `0`
- Skipped: `0`
