# Snowdrop Migration Preflight

Generated at: `2026-05-11T03:50:35.728413736+00:00`

- Source PostgreSQL database: `blocked_ticketing_orders`
- Schema filter: `all non-system schemas`

## Summary

| Metric | Value |
|---|---|
| Tables | 1 |
| Columns | 4 |
| Constraints | 1 |
| Indexes | 2 |
| Supported tables | 0 |
| Partial tables | 0 |
| Unsupported tables | 1 |

| DDL-ready tables | 0 |
| DDL-partial tables | 1 |
| DDL-blocked tables | 0 |

## Findings By Status

| Status | Count |
|---|---|
| partial | 5 |
| unsupported | 3 |

## Table Compatibility

| Schema | Table | Status | Translation | Columns | Constraints | Indexes |
|---|---|---|---|---|---|---|
| public | blocked_orders | unsupported | partial | 4 | 1 | 2 |

## Compatibility Findings

| Status | Object | Category | Message | Recommendation |
|---|---|---|---|---|
| partial | column `public.blocked_orders.id` | default | sequence-backed defaults need manual handling because Snowdrop does not expose PostgreSQL serial/sequence behavior directly | rewrite this default into explicit application-managed keys or a Snowdrop-compatible loading strategy |
| partial | column `public.blocked_orders.id` | type | column uses PostgreSQL type 'bigint' which maps to the Snowdrop status 'partial' | review this column during migration and confirm the translated semantics are acceptable |
| unsupported | column `public.blocked_orders.payload` | type | column uses PostgreSQL type 'jsonb' which maps to the Snowdrop status 'unsupported' | rewrite this column to a supported Snowdrop type or exclude it from automatic migration |
| unsupported | column `public.blocked_orders.tags` | type | column uses PostgreSQL type 'ARRAY' which maps to the Snowdrop status 'unsupported' | rewrite this column to a supported Snowdrop type or exclude it from automatic migration |
| partial | column `public.blocked_orders.created_at` | default | function-backed default expression needs manual review before migration | replace this default with a supported literal or add an application-side default during migration |
| partial | column `public.blocked_orders.created_at` | type | column uses PostgreSQL type 'timestamp without time zone' which maps to the Snowdrop status 'partial' | review this column during migration and confirm the translated semantics are acceptable |
| partial | index `public.blocked_orders.blocked_orders_pkey` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| unsupported | index `public.blocked_orders.idx_blocked_orders_payload` | index | index definition needs review during migration | recreate this index manually with a Snowdrop-supported index strategy |

## Unsupported Feature Inventory

### index

- index public.blocked_orders.idx_blocked_orders_payload -> index definition needs review during migration

### type

- column public.blocked_orders.payload -> column uses PostgreSQL type 'jsonb' which maps to the Snowdrop status 'unsupported'
- column public.blocked_orders.tags -> column uses PostgreSQL type 'ARRAY' which maps to the Snowdrop status 'unsupported'

## Partial Feature Inventory

### default

- column public.blocked_orders.id -> sequence-backed defaults need manual handling because Snowdrop does not expose PostgreSQL serial/sequence behavior directly
- column public.blocked_orders.created_at -> function-backed default expression needs manual review before migration

### index

- index public.blocked_orders.blocked_orders_pkey -> index definition needs review during migration

### type

- column public.blocked_orders.id -> column uses PostgreSQL type 'bigint' which maps to the Snowdrop status 'partial'
- column public.blocked_orders.created_at -> column uses PostgreSQL type 'timestamp without time zone' which maps to the Snowdrop status 'partial'

## Table `public`.`blocked_orders`

- Overall status: `unsupported`
- Translation preview status: `partial`
- table contains unsupported PostgreSQL features for the current Snowdrop roadmap
- Translation note: default for column 'id' needs manual rewrite and is omitted from preview
- Translation note: column 'payload' has no safe Snowdrop type mapping yet
- Translation note: column 'tags' has no safe Snowdrop type mapping yet
- Translation note: default for column 'created_at' needs manual rewrite and is omitted from preview
- Translation note: index 'blocked_orders_pkey' is omitted from the preview and should be recreated manually if still needed
- Translation note: index 'idx_blocked_orders_payload' is omitted from the preview and should be recreated manually if still needed

### Columns

| Column | Source type | Snowdrop type | Nullable | Status | Notes |
|---|---|---|---|---|---|
| id | bigint | BIGINT | no | partial | sequence-backed defaults need manual handling because Snowdrop does not expose PostgreSQL serial/sequence behavior directly |
| payload | jsonb | - | no | unsupported | JSON / JSONB are still unsupported in the current Snowdrop roadmap |
| tags | ARRAY | - | no | unsupported | array types are not supported by the current Snowdrop type layer |
| created_at | timestamp without time zone | TIMESTAMP | no | partial | function-backed default expression needs manual review before migration |

### Constraints

| Name | Kind | Columns | Status | Reference | Notes |
|---|---|---|---|---|---|
| blocked_orders_pkey | PRIMARY KEY | id | supported | - |  |

### Indexes

| Name | Status | Definition |
|---|---|---|
| blocked_orders_pkey | partial | CREATE UNIQUE INDEX blocked_orders_pkey ON public.blocked_orders USING btree (id) |
| idx_blocked_orders_payload | unsupported | CREATE INDEX idx_blocked_orders_payload ON public.blocked_orders USING gin (payload) |

### Translation Preview

```sql
-- Source table public.blocked_orders
CREATE TABLE "blocked_orders" (
  "id" BIGINT NOT NULL,
  "created_at" TIMESTAMP NOT NULL,
  PRIMARY KEY ("id")
);

-- NOTE: default for column 'id' needs manual rewrite and is omitted from preview
-- NOTE: column 'payload' has no safe Snowdrop type mapping yet
-- NOTE: column 'tags' has no safe Snowdrop type mapping yet
-- NOTE: default for column 'created_at' needs manual rewrite and is omitted from preview
-- NOTE: index 'blocked_orders_pkey' is omitted from the preview and should be recreated manually if still needed
-- NOTE: index 'idx_blocked_orders_payload' is omitted from the preview and should be recreated manually if still needed
```

