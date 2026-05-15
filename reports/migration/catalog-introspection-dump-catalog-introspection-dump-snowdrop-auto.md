# Snowdrop Automatic Migration

Generated at: `2026-05-11T03:51:23.003826317+00:00`

- Source mode: `dump-dir`
- Source: `catalog_introspection_dump`
- Target Snowdrop database: `catalog_introspection_dump_snowdrop`

## Summary

| Metric | Value |
|---|---|
| Success | `true` |
| Supported tables | 0 |
| Partial tables | 2 |
| Blocked tables | 0 |
| Verification passed | `true` |

## Safe Omissions

- public.accounts -> index 'accounts_pkey' is omitted from the preview and should be recreated manually if still needed
- public.accounts -> index 'accounts_tenant_email_key' is omitted from the preview and should be recreated manually if still needed
- public.parent_accounts -> index 'parent_accounts_pkey' is omitted from the preview and should be recreated manually if still needed

## Workflow

# Snowdrop Migration Workflow

Generated at: `2026-05-11T03:51:22.992824685+00:00`

- Source PostgreSQL database: `catalog_introspection_dump`
- Target Snowdrop database: `catalog_introspection_dump_snowdrop`

## Summary

| Metric | Value |
|---|---|
| Planned tables | 2 |
| Blocked tables | 0 |
| DDL applied tables | 2 |
| Loaded tables | 2 |
| Verified tables | 2 |
| Row-count matches | 2 |
| Sample matches | 2 |
| Aggregate matches | 2 |

| Replay queries run | 6 |
| Replay queries matched | 6 |

## Table Runs

| Schema | Table | Translation | Load status | Source rows | Target rows | Count match | Sample match | Aggregate match |
|---|---|---|---|---|---|---|---|---|
| public | parent_accounts | partial | verified | 0 | 0 | yes | yes | yes |
| public | accounts | partial | verified | 0 | 0 | yes | yes | yes |

## Table `public`.`parent_accounts`

- Translation status: `partial`
- Load status: `verified`
- DDL applied: `yes`
- Source row count: `0`
- Target row count: `0`

### Replay Checks

| Name | Matched | Source SQL | Target SQL | Note |
|---|---|---|---|---|
| row_count | yes | SELECT COUNT(*) FROM "public"."parent_accounts"; | SELECT COUNT(*) FROM "parent_accounts"; | - |
| ordered_sample | yes | SELECT "id" FROM "public"."parent_accounts" ORDER BY "id" LIMIT 25; | SELECT "id" FROM "parent_accounts" ORDER BY "id" LIMIT 25; | - |
| numeric_aggregates | yes | SELECT COUNT(*), COUNT("id"), SUM("id") FROM "public"."parent_accounts"; | SELECT COUNT(*), COUNT("id"), SUM("id") FROM "parent_accounts"; | - |

## Table `public`.`accounts`

- Translation status: `partial`
- Load status: `verified`
- DDL applied: `yes`
- Source row count: `0`
- Target row count: `0`

### Replay Checks

| Name | Matched | Source SQL | Target SQL | Note |
|---|---|---|---|---|
| row_count | yes | SELECT COUNT(*) FROM "public"."accounts"; | SELECT COUNT(*) FROM "accounts"; | - |
| ordered_sample | yes | SELECT "id", "tenant_id", "email", "parent_id", "balance" FROM "public"."accounts" ORDER BY "id" LIMIT 25; | SELECT "id", "tenant_id", "email", "parent_id", "balance" FROM "accounts" ORDER BY "id" LIMIT 25; | - |
| numeric_aggregates | yes | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("tenant_id"), SUM("tenant_id"), COUNT("parent_id"), SUM("parent_id"), COUNT("balance"), SUM("balance") FROM "public"."accounts"; | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("tenant_id"), SUM("tenant_id"), COUNT("parent_id"), SUM("parent_id"), COUNT("balance"), SUM("balance") FROM "accounts"; | - |

