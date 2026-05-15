# Snowdrop Test Report

Generated at: `2026-05-03T01:00:03.518000781+00:00`

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
| join_skew | join_skew | yes | Exercise join behavior and simple skew-sensitive aggregations. | golden, differential |
| perf_fixed | perf_fixed | yes | Stable non-synthetic fixture for future query micro-benchmarks. | golden, differential |
| tiny_smoke | tiny_smoke | yes | Smoke correctness checks with a tiny readable fixture. | golden, differential, sqllogictest, tap, postgres_style |

## Suite Summary By Dataset

| Dataset | Suite | Pass | Fail | Mismatch | Skip |
|---|---|---|---|---|---|
| edge_cases | differential | 2 | 0 | 0 | 0 |
| edge_cases | golden | 2 | 0 | 0 | 0 |
| join_skew | differential | 1 | 0 | 0 | 0 |
| join_skew | golden | 1 | 0 | 0 | 0 |
| perf_fixed | differential | 1 | 0 | 0 | 0 |
| perf_fixed | golden | 1 | 0 | 0 | 0 |
| tiny_smoke | differential | 2 | 0 | 0 | 0 |
| tiny_smoke | golden | 3 | 0 | 0 | 0 |
| tiny_smoke | postgres-style | 2 | 0 | 0 | 0 |
| tiny_smoke | sqllogictest | 4 | 0 | 0 | 0 |
| tiny_smoke | tap | 4 | 0 | 0 | 0 |

## golden Results

| Dataset | Group | Case | Status | Details |
|---|---|---|---|---|
| edge_cases | non_negative_sum | non_negative_sum | pass | matched expected rows |
| edge_cases | null_notes | null_notes | pass | matched expected rows |
| join_skew | customer_totals | customer_totals | pass | matched expected rows |
| perf_fixed | bucket_totals | bucket_totals | pass | matched expected rows |
| tiny_smoke | category_sales | category_sales | pass | matched expected rows |
| tiny_smoke | expected_error | expected_error | pass | matched expected error |
| tiny_smoke | select_items | select_items | pass | matched expected rows |

## differential Results

| Dataset | Group | Case | Status | Details |
|---|---|---|---|---|
| edge_cases | non_negative_sum | non_negative_sum | pass | Snowdrop matched PostgreSQL |
| edge_cases | null_notes | null_notes | pass | Snowdrop matched PostgreSQL |
| join_skew | customer_totals | customer_totals | pass | Snowdrop matched PostgreSQL |
| perf_fixed | bucket_totals | bucket_totals | pass | Snowdrop matched PostgreSQL |
| tiny_smoke | category_sales | category_sales | pass | Snowdrop matched PostgreSQL |
| tiny_smoke | select_items | select_items | pass | Snowdrop matched PostgreSQL |

## SQLLogicTest Results

| Dataset | Group | Case | Status | Details |
|---|---|---|---|---|
| tiny_smoke | basic.test | command_1 | pass | statement ok |
| tiny_smoke | basic.test | command_2 | pass | statement ok |
| tiny_smoke | basic.test | command_3 | pass | statement error |
| tiny_smoke | basic.test | command_4 | pass | query matched |

## TAP-style Results

| Dataset | Group | Case | Status | Details |
|---|---|---|---|---|
| tiny_smoke | smoke | health endpoint | pass | health ok |
| tiny_smoke | smoke | metrics endpoint | pass | metrics ok |
| tiny_smoke | smoke | count items | pass | sql matched |
| tiny_smoke | smoke | invalid query errors | pass | expected error |

## PostgreSQL-style Results

| Dataset | Group | Case | Status | Details |
|---|---|---|---|---|
| tiny_smoke | basic | select_count | pass | matched expected output |
| tiny_smoke | basic | top_items | pass | matched expected output |

## Conclusions

- Total cases: `23`
- Passed: `23`
- Failed: `0`
- Mismatched: `0`
- Skipped: `0`
