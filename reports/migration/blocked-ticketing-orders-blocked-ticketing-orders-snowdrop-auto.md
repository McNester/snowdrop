# Snowdrop Automatic Migration

Generated at: `2026-05-11T03:50:35.748169905+00:00`

- Source mode: `dump-dir`
- Source: `blocked_ticketing_orders`
- Target Snowdrop database: `blocked_ticketing_orders_snowdrop`

## Summary

| Metric | Value |
|---|---|
| Success | `false` |
| Supported tables | 0 |
| Partial tables | 0 |
| Blocked tables | 1 |
| Verification passed | `false` |

## Blockers

- `unsupported` public.blocked_orders.payload: column uses PostgreSQL type 'jsonb' which maps to the Snowdrop status 'unsupported'. Recommendation: rewrite this column to a supported Snowdrop type or exclude it from automatic migration
- `unsupported` public.blocked_orders.tags: column uses PostgreSQL type 'ARRAY' which maps to the Snowdrop status 'unsupported'. Recommendation: rewrite this column to a supported Snowdrop type or exclude it from automatic migration
- `unsupported` public.blocked_orders.idx_blocked_orders_payload: index definition needs review during migration. Recommendation: recreate this index manually with a Snowdrop-supported index strategy
- `unsupported` public.blocked_orders: default for column 'id' needs manual rewrite and is omitted from preview. Recommendation: remove this manual follow-up before relying on the automatic migration path
- `unsupported` public.blocked_orders: column 'payload' has no safe Snowdrop type mapping yet. Recommendation: remove this manual follow-up before relying on the automatic migration path
- `unsupported` public.blocked_orders: column 'tags' has no safe Snowdrop type mapping yet. Recommendation: remove this manual follow-up before relying on the automatic migration path
- `unsupported` public.blocked_orders: default for column 'created_at' needs manual rewrite and is omitted from preview. Recommendation: remove this manual follow-up before relying on the automatic migration path

## Safe Omissions

- public.blocked_orders -> index 'blocked_orders_pkey' is omitted from the preview and should be recreated manually if still needed
- public.blocked_orders -> index 'idx_blocked_orders_payload' is omitted from the preview and should be recreated manually if still needed

