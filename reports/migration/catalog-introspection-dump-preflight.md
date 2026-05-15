# Snowdrop Migration Preflight

Generated at: `2026-05-11T03:51:04.250768720+00:00`

- Source PostgreSQL database: `catalog_introspection_dump`
- Schema filter: `all non-system schemas`

## Summary

| Metric | Value |
|---|---|
| Tables | 2 |
| Columns | 6 |
| Constraints | 5 |
| Indexes | 3 |
| Supported tables | 0 |
| Partial tables | 2 |
| Unsupported tables | 0 |

| DDL-ready tables | 0 |
| DDL-partial tables | 2 |
| DDL-blocked tables | 0 |

## Findings By Status

| Status | Count |
|---|---|
| partial | 4 |

## Table Compatibility

| Schema | Table | Status | Translation | Columns | Constraints | Indexes |
|---|---|---|---|---|---|---|
| public | accounts | partial | partial | 5 | 4 | 2 |
| public | parent_accounts | partial | partial | 1 | 1 | 1 |

## Compatibility Findings

| Status | Object | Category | Message | Recommendation |
|---|---|---|---|---|
| partial | constraint `public.accounts.accounts_balance_check` | constraint | CHECK constraint needs manual review during migration | validate this constraint against the current Snowdrop subset before applying it automatically |
| partial | index `public.accounts.accounts_pkey` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | index `public.accounts.accounts_tenant_email_key` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | index `public.parent_accounts.parent_accounts_pkey` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |

## Partial Feature Inventory

### constraint

- constraint public.accounts.accounts_balance_check -> CHECK constraint needs manual review during migration

### index

- index public.accounts.accounts_pkey -> index definition needs review during migration
- index public.accounts.accounts_tenant_email_key -> index definition needs review during migration
- index public.parent_accounts.parent_accounts_pkey -> index definition needs review during migration

## Table `public`.`accounts`

- Overall status: `partial`
- Translation preview status: `partial`
- table needs manual review for partial features before a production migration
- Translation note: index 'accounts_pkey' is omitted from the preview and should be recreated manually if still needed
- Translation note: index 'accounts_tenant_email_key' is omitted from the preview and should be recreated manually if still needed

### Columns

| Column | Source type | Snowdrop type | Nullable | Status | Notes |
|---|---|---|---|---|---|
| id | bigint | BIGINT | no | supported |  |
| tenant_id | bigint | BIGINT | yes | supported |  |
| email | varchar(255) | VARCHAR(255) | yes | supported |  |
| parent_id | bigint | BIGINT | yes | supported |  |
| balance | bigint | BIGINT | yes | supported |  |

### Constraints

| Name | Kind | Columns | Status | Reference | Notes |
|---|---|---|---|---|---|
| accounts_balance_check | CHECK |  | partial | - | CHECK constraints are supported only for the current safe row-local scalar subset |
| accounts_parent_fkey | FOREIGN KEY | parent_id | supported | public.parent_accounts(id) |  |
| accounts_pkey | PRIMARY KEY | id | supported | - |  |
| accounts_tenant_email_key | UNIQUE | tenant_id, email | supported | - | multi-column UNIQUE is supported in the current narrow Phase 4 slice |

### Indexes

| Name | Status | Definition |
|---|---|---|
| accounts_pkey | partial | CREATE UNIQUE INDEX accounts_pkey ON public.accounts USING btree (id) |
| accounts_tenant_email_key | partial | CREATE UNIQUE INDEX accounts_tenant_email_key ON public.accounts USING btree (tenant_id, email) |

### Translation Preview

```sql
-- Source table public.accounts
CREATE TABLE "accounts" (
  "id" BIGINT NOT NULL,
  "tenant_id" BIGINT,
  "email" VARCHAR(255),
  "parent_id" BIGINT,
  "balance" BIGINT,
  CHECK (((balance >= 0))),
  FOREIGN KEY ("parent_id") REFERENCES "parent_accounts" ("id"),
  PRIMARY KEY ("id"),
  UNIQUE ("tenant_id", "email")
);

-- NOTE: index 'accounts_pkey' is omitted from the preview and should be recreated manually if still needed
-- NOTE: index 'accounts_tenant_email_key' is omitted from the preview and should be recreated manually if still needed
```

## Table `public`.`parent_accounts`

- Overall status: `partial`
- Translation preview status: `partial`
- table needs manual review for partial features before a production migration
- Translation note: index 'parent_accounts_pkey' is omitted from the preview and should be recreated manually if still needed

### Columns

| Column | Source type | Snowdrop type | Nullable | Status | Notes |
|---|---|---|---|---|---|
| id | bigint | BIGINT | no | supported |  |

### Constraints

| Name | Kind | Columns | Status | Reference | Notes |
|---|---|---|---|---|---|
| parent_accounts_pkey | PRIMARY KEY | id | supported | - |  |

### Indexes

| Name | Status | Definition |
|---|---|---|
| parent_accounts_pkey | partial | CREATE UNIQUE INDEX parent_accounts_pkey ON public.parent_accounts USING btree (id) |

### Translation Preview

```sql
-- Source table public.parent_accounts
CREATE TABLE "parent_accounts" (
  "id" BIGINT NOT NULL,
  PRIMARY KEY ("id")
);

-- NOTE: index 'parent_accounts_pkey' is omitted from the preview and should be recreated manually if still needed
```

