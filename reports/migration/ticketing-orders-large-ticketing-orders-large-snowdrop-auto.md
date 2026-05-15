# Snowdrop Automatic Migration

Generated at: `2026-05-11T03:52:25.456643137+00:00`

- Source mode: `dump-dir`
- Source: `ticketing_orders_large`
- Target Snowdrop database: `ticketing_orders_large_snowdrop`

## Summary

| Metric | Value |
|---|---|
| Success | `false` |
| Supported tables | 0 |
| Partial tables | 13 |
| Blocked tables | 0 |
| Verification passed | `false` |

## Safe Omissions

- public.accounts -> index 'accounts_customer_id_key' is omitted from the preview and should be recreated manually if still needed
- public.accounts -> index 'accounts_pkey' is omitted from the preview and should be recreated manually if still needed
- public.audit_events -> index 'audit_events_pkey' is omitted from the preview and should be recreated manually if still needed
- public.audit_events -> index 'idx_audit_events_account_day' is omitted from the preview and should be recreated manually if still needed
- public.customers -> index 'customers_email_key' is omitted from the preview and should be recreated manually if still needed
- public.customers -> index 'customers_pkey' is omitted from the preview and should be recreated manually if still needed
- public.events -> index 'events_event_code_key' is omitted from the preview and should be recreated manually if still needed
- public.events -> index 'events_pkey' is omitted from the preview and should be recreated manually if still needed
- public.events -> index 'idx_events_starts_on' is omitted from the preview and should be recreated manually if still needed
- public.order_items -> index 'idx_order_items_show_id' is omitted from the preview and should be recreated manually if still needed
- public.order_items -> index 'order_items_order_id_line_no_key' is omitted from the preview and should be recreated manually if still needed
- public.order_items -> index 'order_items_pkey' is omitted from the preview and should be recreated manually if still needed
- public.orders -> index 'idx_orders_account_ordered_at' is omitted from the preview and should be recreated manually if still needed
- public.orders -> index 'orders_pkey' is omitted from the preview and should be recreated manually if still needed
- public.payments -> index 'idx_payments_order_id' is omitted from the preview and should be recreated manually if still needed
- public.payments -> index 'payments_pkey' is omitted from the preview and should be recreated manually if still needed
- public.payments -> index 'payments_provider_ref_key' is omitted from the preview and should be recreated manually if still needed
- public.promotions -> index 'promotions_pkey' is omitted from the preview and should be recreated manually if still needed
- public.promotions -> index 'promotions_promo_code_key' is omitted from the preview and should be recreated manually if still needed
- public.refunds -> index 'refunds_pkey' is omitted from the preview and should be recreated manually if still needed
- public.seat_holds -> index 'seat_holds_pkey' is omitted from the preview and should be recreated manually if still needed
- public.seat_holds -> index 'seat_holds_show_id_seat_label_key' is omitted from the preview and should be recreated manually if still needed
- public.shows -> index 'idx_shows_event_day' is omitted from the preview and should be recreated manually if still needed
- public.shows -> index 'shows_pkey' is omitted from the preview and should be recreated manually if still needed
- public.shows -> index 'shows_show_code_key' is omitted from the preview and should be recreated manually if still needed
- public.tickets -> index 'tickets_pkey' is omitted from the preview and should be recreated manually if still needed
- public.tickets -> index 'tickets_ticket_code_key' is omitted from the preview and should be recreated manually if still needed
- public.venues -> index 'venues_pkey' is omitted from the preview and should be recreated manually if still needed
- public.venues -> index 'venues_venue_code_key' is omitted from the preview and should be recreated manually if still needed

## Workflow

# Snowdrop Migration Workflow

Generated at: `2026-05-11T03:52:25.443295138+00:00`

- Source PostgreSQL database: `ticketing_orders_large`
- Target Snowdrop database: `ticketing_orders_large_snowdrop`

## Summary

| Metric | Value |
|---|---|
| Planned tables | 13 |
| Blocked tables | 0 |
| DDL applied tables | 13 |
| Loaded tables | 5 |
| Verified tables | 5 |
| Row-count matches | 5 |
| Sample matches | 3 |
| Aggregate matches | 3 |

| Replay queries run | 15 |
| Replay queries matched | 11 |

## Table Runs

| Schema | Table | Translation | Load status | Source rows | Target rows | Count match | Sample match | Aggregate match |
|---|---|---|---|---|---|---|---|---|
| public | customers | partial | verified | 4000 | 4000 | yes | no | yes |
| public | promotions | partial | verified | 64 | 64 | yes | no | yes |
| public | venues | partial | verified | 100 | 100 | yes | yes | yes |
| public | accounts | partial | verified | 4000 | 4000 | yes | yes | no |
| public | events | partial | verified | 1600 | 1600 | yes | yes | no |
| public | audit_events | partial | load_failed | - | - | - | - | - |
| public | orders | partial | load_failed | - | - | - | - | - |
| public | shows | partial | load_failed | - | - | - | - | - |
| public | order_items | partial | load_failed | - | - | - | - | - |
| public | payments | partial | load_failed | - | - | - | - | - |
| public | seat_holds | partial | load_failed | - | - | - | - | - |
| public | refunds | partial | load_failed | - | - | - | - | - |
| public | tickets | partial | load_failed | - | - | - | - | - |

## Table `public`.`customers`

- Translation status: `partial`
- Load status: `verified`
- DDL applied: `yes`
- Source row count: `4000`
- Target row count: `4000`
- sample row verification diverged on the ordered first 25 rows

### Replay Checks

| Name | Matched | Source SQL | Target SQL | Note |
|---|---|---|---|---|
| row_count | yes | SELECT COUNT(*) FROM "public"."customers"; | SELECT COUNT(*) FROM "customers"; | - |
| ordered_sample | no | SELECT "id", "email", "full_name", "tier", "marketing_opt_in", "created_on" FROM "public"."customers" ORDER BY "id" LIMIT 25; | SELECT "id", "email", "full_name", "tier", "marketing_opt_in", "created_on" FROM "customers" ORDER BY "id" LIMIT 25; | - |
| numeric_aggregates | yes | SELECT COUNT(*), COUNT("id"), SUM("id") FROM "public"."customers"; | SELECT COUNT(*), COUNT("id"), SUM("id") FROM "customers"; | - |

## Table `public`.`promotions`

- Translation status: `partial`
- Load status: `verified`
- DDL applied: `yes`
- Source row count: `64`
- Target row count: `64`
- sample row verification diverged on the ordered first 25 rows

### Replay Checks

| Name | Matched | Source SQL | Target SQL | Note |
|---|---|---|---|---|
| row_count | yes | SELECT COUNT(*) FROM "public"."promotions"; | SELECT COUNT(*) FROM "promotions"; | - |
| ordered_sample | no | SELECT "id", "promo_code", "discount_percent", "active" FROM "public"."promotions" ORDER BY "id" LIMIT 25; | SELECT "id", "promo_code", "discount_percent", "active" FROM "promotions" ORDER BY "id" LIMIT 25; | - |
| numeric_aggregates | yes | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("discount_percent"), SUM("discount_percent") FROM "public"."promotions"; | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("discount_percent"), SUM("discount_percent") FROM "promotions"; | - |

## Table `public`.`venues`

- Translation status: `partial`
- Load status: `verified`
- DDL applied: `yes`
- Source row count: `100`
- Target row count: `100`

### Replay Checks

| Name | Matched | Source SQL | Target SQL | Note |
|---|---|---|---|---|
| row_count | yes | SELECT COUNT(*) FROM "public"."venues"; | SELECT COUNT(*) FROM "venues"; | - |
| ordered_sample | yes | SELECT "id", "venue_code", "name", "city", "capacity" FROM "public"."venues" ORDER BY "id" LIMIT 25; | SELECT "id", "venue_code", "name", "city", "capacity" FROM "venues" ORDER BY "id" LIMIT 25; | - |
| numeric_aggregates | yes | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("capacity"), SUM("capacity") FROM "public"."venues"; | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("capacity"), SUM("capacity") FROM "venues"; | - |

## Table `public`.`accounts`

- Translation status: `partial`
- Load status: `verified`
- DDL applied: `yes`
- Source row count: `4000`
- Target row count: `4000`
- aggregate verification diverged for at least one numeric column

### Replay Checks

| Name | Matched | Source SQL | Target SQL | Note |
|---|---|---|---|---|
| row_count | yes | SELECT COUNT(*) FROM "public"."accounts"; | SELECT COUNT(*) FROM "accounts"; | - |
| ordered_sample | yes | SELECT "id", "customer_id", "status", "credit_limit", "opened_on" FROM "public"."accounts" ORDER BY "id" LIMIT 25; | SELECT "id", "customer_id", "status", "credit_limit", "opened_on" FROM "accounts" ORDER BY "id" LIMIT 25; | - |
| numeric_aggregates | no | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("customer_id"), SUM("customer_id"), COUNT("credit_limit"), SUM("credit_limit") FROM "public"."accounts"; | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("customer_id"), SUM("customer_id"), COUNT("credit_limit"), SUM("credit_limit") FROM "accounts"; | - |

## Table `public`.`events`

- Translation status: `partial`
- Load status: `verified`
- DDL applied: `yes`
- Source row count: `1600`
- Target row count: `1600`
- aggregate verification diverged for at least one numeric column

### Replay Checks

| Name | Matched | Source SQL | Target SQL | Note |
|---|---|---|---|---|
| row_count | yes | SELECT COUNT(*) FROM "public"."events"; | SELECT COUNT(*) FROM "events"; | - |
| ordered_sample | yes | SELECT "id", "venue_id", "event_code", "title", "category", "starts_on", "base_price" FROM "public"."events" ORDER BY "id" LIMIT 25; | SELECT "id", "venue_id", "event_code", "title", "category", "starts_on", "base_price" FROM "events" ORDER BY "id" LIMIT 25; | - |
| numeric_aggregates | no | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("venue_id"), SUM("venue_id"), COUNT("base_price"), SUM("base_price") FROM "public"."events"; | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("venue_id"), SUM("venue_id"), COUNT("base_price"), SUM("base_price") FROM "events"; | - |

## Table `public`.`audit_events`

- Translation status: `partial`
- Load status: `load_failed`
- DDL applied: `yes`
- data load failed: ERROR: table 'audit_events' does not exist in database 'catalog_introspection_dump_snowdrop'

## Table `public`.`orders`

- Translation status: `partial`
- Load status: `load_failed`
- DDL applied: `yes`
- data load failed: ERROR: table 'promotions' does not exist in database 'catalog_introspection_dump_snowdrop'

## Table `public`.`shows`

- Translation status: `partial`
- Load status: `load_failed`
- DDL applied: `yes`
- data load failed: ERROR: table 'events' does not exist in database 'catalog_introspection_dump_snowdrop'

## Table `public`.`order_items`

- Translation status: `partial`
- Load status: `load_failed`
- DDL applied: `yes`
- data load failed: ERROR: FOREIGN KEY 'fk_order_items_1' violation: referenced row is missing

## Table `public`.`payments`

- Translation status: `partial`
- Load status: `load_failed`
- DDL applied: `yes`
- data load failed: ERROR: FOREIGN KEY 'fk_payments_1' violation: referenced row is missing

## Table `public`.`seat_holds`

- Translation status: `partial`
- Load status: `load_failed`
- DDL applied: `yes`
- data load failed: ERROR: table 'customers' does not exist in database 'catalog_introspection_dump_snowdrop'

## Table `public`.`refunds`

- Translation status: `partial`
- Load status: `load_failed`
- DDL applied: `yes`
- data load failed: ERROR: FOREIGN KEY 'fk_refunds_1' violation: referenced row is missing

## Table `public`.`tickets`

- Translation status: `partial`
- Load status: `load_failed`
- DDL applied: `yes`
- data load failed: ERROR: FOREIGN KEY 'fk_tickets_0' violation: referenced row is missing

