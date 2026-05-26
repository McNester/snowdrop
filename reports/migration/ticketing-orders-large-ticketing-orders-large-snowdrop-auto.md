# Snowdrop Automatic Migration

Generated at: `2026-05-25T20:35:04.930248784+00:00`

- Source mode: `dump-dir`
- Source: `ticketing_orders_large`
- Target Snowdrop database: `ticketing_orders_large_snowdrop`

## Summary

| Metric | Value |
|---|---|
| Success | `true` |
| Supported tables | 13 |
| Partial tables | 0 |
| Blocked tables | 0 |
| Verification passed | `true` |

## Workflow

# Snowdrop Migration Workflow

Generated at: `2026-05-25T20:35:04.918259242+00:00`

- Source PostgreSQL database: `ticketing_orders_large`
- Target Snowdrop database: `ticketing_orders_large_snowdrop`

## Summary

| Metric | Value |
|---|---|
| Planned tables | 13 |
| Blocked tables | 0 |
| DDL applied tables | 13 |
| Loaded tables | 13 |
| Verified tables | 13 |
| Row-count matches | 13 |
| Sample matches | 13 |
| Aggregate matches | 13 |

| Replay queries run | 39 |
| Replay queries matched | 39 |

| Benchmark queries run | 5 |
| Benchmark queries matched | 5 |
| Benchmark queries faster on Snowdrop | 3 |

## Post-Migration Benchmark

| Query | Matched | PostgreSQL avg ms | Snowdrop avg ms | Speedup | Faster |
|---|---|---|---|---|---|
| audit_activity_scan_rollup | yes | 1.614 | 2.483 | 0.65x | postgres |
| orders_status_count_scan | yes | 0.602 | 0.703 | 0.86x | postgres |
| tickets_status_scan | yes | 0.989 | 0.820 | 1.21x | snowdrop |
| payments_status_count_scan | yes | 0.599 | 0.568 | 1.05x | snowdrop |
| seat_hold_status_scan | yes | 0.522 | 0.479 | 1.09x | snowdrop |

### Benchmark `audit_activity_scan_rollup`

- Snowdrop-favorable scan-heavy activity dashboard over the largest append-style fact table.
- Result set matched: `true`
- PostgreSQL average latency: `1.614 ms`
- Snowdrop average latency: `2.483 ms`
- Speedup vs PostgreSQL: `0.65x`
- Faster engine: `postgres`

```sql
SELECT event_day, event_kind, COUNT(*) AS activity_count FROM audit_events GROUP BY event_day, event_kind ORDER BY event_day, event_kind LIMIT 60;
```

### Benchmark `orders_status_count_scan`

- Single-table status distribution over order facts with low-cardinality grouping.
- Result set matched: `true`
- PostgreSQL average latency: `0.602 ms`
- Snowdrop average latency: `0.703 ms`
- Speedup vs PostgreSQL: `0.86x`
- Faster engine: `postgres`

```sql
SELECT order_status, COUNT(*) AS order_count FROM orders GROUP BY order_status ORDER BY order_status;
```

### Benchmark `tickets_status_scan`

- High-row-count status distribution over tickets without join materialization.
- Result set matched: `true`
- PostgreSQL average latency: `0.989 ms`
- Snowdrop average latency: `0.820 ms`
- Speedup vs PostgreSQL: `1.21x`
- Faster engine: `snowdrop`

```sql
SELECT ticket_status, COUNT(*) AS ticket_count FROM tickets GROUP BY ticket_status ORDER BY ticket_status;
```

### Benchmark `payments_status_count_scan`

- Payment fact status distribution using a compact projection.
- Result set matched: `true`
- PostgreSQL average latency: `0.599 ms`
- Snowdrop average latency: `0.568 ms`
- Speedup vs PostgreSQL: `1.05x`
- Faster engine: `snowdrop`

```sql
SELECT payment_status, COUNT(*) AS payment_count FROM payments GROUP BY payment_status ORDER BY payment_status;
```

### Benchmark `seat_hold_status_scan`

- Operational hold-state dashboard over a focused fact-like table.
- Result set matched: `true`
- PostgreSQL average latency: `0.522 ms`
- Snowdrop average latency: `0.479 ms`
- Speedup vs PostgreSQL: `1.09x`
- Faster engine: `snowdrop`

```sql
SELECT hold_status, COUNT(*) AS hold_count FROM seat_holds GROUP BY hold_status ORDER BY hold_status;
```

## Table Runs

| Schema | Table | Translation | Load status | Source rows | Target rows | Load ms | Count match | Sample match | Aggregate match |
|---|---|---|---|---|---|---:|---|---|---|
| public | customers | ready | verified | 400 | 400 | 344.9 | yes | yes | yes |
| public | promotions | ready | verified | 32 | 32 | 543.0 | yes | yes | yes |
| public | venues | ready | verified | 30 | 30 | 105.9 | yes | yes | yes |
| public | accounts | ready | verified | 400 | 400 | 478.9 | yes | yes | yes |
| public | events | ready | verified | 160 | 160 | 288.0 | yes | yes | yes |
| public | audit_events | ready | verified | 5000 | 5000 | 4400.5 | yes | yes | yes |
| public | orders | ready | verified | 1600 | 1600 | 1561.4 | yes | yes | yes |
| public | shows | ready | verified | 800 | 800 | 694.7 | yes | yes | yes |
| public | order_items | ready | verified | 3200 | 3200 | 2778.5 | yes | yes | yes |
| public | payments | ready | verified | 1600 | 1600 | 1241.2 | yes | yes | yes |
| public | seat_holds | ready | verified | 1000 | 1000 | 753.7 | yes | yes | yes |
| public | refunds | ready | verified | 84 | 84 | 189.8 | yes | yes | yes |
| public | tickets | ready | verified | 3200 | 3200 | 2660.5 | yes | yes | yes |

## Table `public`.`customers`

- Translation status: `ready`
- Load status: `verified`
- DDL applied: `yes`
- Source row count: `400`
- Target row count: `400`
- Load timing: total `344.89 ms`, export `7.47 ms`, literal build `2.39 ms`, Snowdrop insert `335.03 ms`, chunks `1`, trusted bulk load `true`

### Replay Checks

| Name | Matched | Source SQL | Target SQL | Note |
|---|---|---|---|---|
| row_count | yes | SELECT COUNT(*) FROM "public"."customers"; | SELECT COUNT(*) FROM "customers"; | - |
| ordered_sample | yes | SELECT "id", "email", "full_name", "tier", "marketing_opt_in", "created_on" FROM "public"."customers" ORDER BY "id" LIMIT 25; | SELECT "id", "email", "full_name", "tier", "marketing_opt_in", "created_on" FROM "customers" ORDER BY "id" LIMIT 25; | - |
| numeric_aggregates | yes | SELECT COUNT(*), COUNT("id"), SUM("id") FROM "public"."customers"; | SELECT COUNT(*), COUNT("id"), SUM("id") FROM "customers"; | - |

## Table `public`.`promotions`

- Translation status: `ready`
- Load status: `verified`
- DDL applied: `yes`
- Source row count: `32`
- Target row count: `32`
- Load timing: total `543.00 ms`, export `0.98 ms`, literal build `0.13 ms`, Snowdrop insert `541.89 ms`, chunks `1`, trusted bulk load `true`

### Replay Checks

| Name | Matched | Source SQL | Target SQL | Note |
|---|---|---|---|---|
| row_count | yes | SELECT COUNT(*) FROM "public"."promotions"; | SELECT COUNT(*) FROM "promotions"; | - |
| ordered_sample | yes | SELECT "id", "promo_code", "discount_percent", "active" FROM "public"."promotions" ORDER BY "id" LIMIT 25; | SELECT "id", "promo_code", "discount_percent", "active" FROM "promotions" ORDER BY "id" LIMIT 25; | - |
| numeric_aggregates | yes | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("discount_percent"), SUM("discount_percent") FROM "public"."promotions"; | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("discount_percent"), SUM("discount_percent") FROM "promotions"; | - |

## Table `public`.`venues`

- Translation status: `ready`
- Load status: `verified`
- DDL applied: `yes`
- Source row count: `30`
- Target row count: `30`
- Load timing: total `105.91 ms`, export `1.08 ms`, literal build `0.14 ms`, Snowdrop insert `104.68 ms`, chunks `1`, trusted bulk load `true`

### Replay Checks

| Name | Matched | Source SQL | Target SQL | Note |
|---|---|---|---|---|
| row_count | yes | SELECT COUNT(*) FROM "public"."venues"; | SELECT COUNT(*) FROM "venues"; | - |
| ordered_sample | yes | SELECT "id", "venue_code", "name", "city", "capacity" FROM "public"."venues" ORDER BY "id" LIMIT 25; | SELECT "id", "venue_code", "name", "city", "capacity" FROM "venues" ORDER BY "id" LIMIT 25; | - |
| numeric_aggregates | yes | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("capacity"), SUM("capacity") FROM "public"."venues"; | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("capacity"), SUM("capacity") FROM "venues"; | - |

## Table `public`.`accounts`

- Translation status: `ready`
- Load status: `verified`
- DDL applied: `yes`
- Source row count: `400`
- Target row count: `400`
- Load timing: total `478.89 ms`, export `6.64 ms`, literal build `1.75 ms`, Snowdrop insert `470.49 ms`, chunks `1`, trusted bulk load `true`

### Replay Checks

| Name | Matched | Source SQL | Target SQL | Note |
|---|---|---|---|---|
| row_count | yes | SELECT COUNT(*) FROM "public"."accounts"; | SELECT COUNT(*) FROM "accounts"; | - |
| ordered_sample | yes | SELECT "id", "customer_id", "status", "credit_limit", "opened_on" FROM "public"."accounts" ORDER BY "id" LIMIT 25; | SELECT "id", "customer_id", "status", "credit_limit", "opened_on" FROM "accounts" ORDER BY "id" LIMIT 25; | - |
| numeric_aggregates | yes | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("customer_id"), SUM("customer_id"), COUNT("credit_limit"), SUM("credit_limit") FROM "public"."accounts"; | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("customer_id"), SUM("customer_id"), COUNT("credit_limit"), SUM("credit_limit") FROM "accounts"; | - |

## Table `public`.`events`

- Translation status: `ready`
- Load status: `verified`
- DDL applied: `yes`
- Source row count: `160`
- Target row count: `160`
- Load timing: total `287.96 ms`, export `3.37 ms`, literal build `1.04 ms`, Snowdrop insert `283.54 ms`, chunks `1`, trusted bulk load `true`

### Replay Checks

| Name | Matched | Source SQL | Target SQL | Note |
|---|---|---|---|---|
| row_count | yes | SELECT COUNT(*) FROM "public"."events"; | SELECT COUNT(*) FROM "events"; | - |
| ordered_sample | yes | SELECT "id", "venue_id", "event_code", "title", "category", "starts_on", "base_price" FROM "public"."events" ORDER BY "id" LIMIT 25; | SELECT "id", "venue_id", "event_code", "title", "category", "starts_on", "base_price" FROM "events" ORDER BY "id" LIMIT 25; | - |
| numeric_aggregates | yes | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("venue_id"), SUM("venue_id"), COUNT("base_price"), SUM("base_price") FROM "public"."events"; | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("venue_id"), SUM("venue_id"), COUNT("base_price"), SUM("base_price") FROM "events"; | - |

## Table `public`.`audit_events`

- Translation status: `ready`
- Load status: `verified`
- DDL applied: `yes`
- Source row count: `5000`
- Target row count: `5000`
- Load timing: total `4400.51 ms`, export `80.00 ms`, literal build `26.33 ms`, Snowdrop insert `4294.16 ms`, chunks `5`, trusted bulk load `true`

### Replay Checks

| Name | Matched | Source SQL | Target SQL | Note |
|---|---|---|---|---|
| row_count | yes | SELECT COUNT(*) FROM "public"."audit_events"; | SELECT COUNT(*) FROM "audit_events"; | - |
| ordered_sample | yes | SELECT "id", "account_id", "event_kind", "created_at", "event_day", "payload_hint" FROM "public"."audit_events" ORDER BY "id" LIMIT 25; | SELECT "id", "account_id", "event_kind", "created_at", "event_day", "payload_hint" FROM "audit_events" ORDER BY "id" LIMIT 25; | - |
| numeric_aggregates | yes | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("account_id"), SUM("account_id") FROM "public"."audit_events"; | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("account_id"), SUM("account_id") FROM "audit_events"; | - |

## Table `public`.`orders`

- Translation status: `ready`
- Load status: `verified`
- DDL applied: `yes`
- Source row count: `1600`
- Target row count: `1600`
- Load timing: total `1561.41 ms`, export `44.50 ms`, literal build `13.99 ms`, Snowdrop insert `1502.90 ms`, chunks `2`, trusted bulk load `true`

### Replay Checks

| Name | Matched | Source SQL | Target SQL | Note |
|---|---|---|---|---|
| row_count | yes | SELECT COUNT(*) FROM "public"."orders"; | SELECT COUNT(*) FROM "orders"; | - |
| ordered_sample | yes | SELECT "id", "account_id", "promo_id", "order_status", "ordered_at", "subtotal", "discount_total", "grand_total" FROM "public"."orders" ORDER BY "id" LIMIT 25; | SELECT "id", "account_id", "promo_id", "order_status", "ordered_at", "subtotal", "discount_total", "grand_total" FROM "orders" ORDER BY "id" LIMIT 25; | - |
| numeric_aggregates | yes | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("account_id"), SUM("account_id"), COUNT("promo_id"), SUM("promo_id"), COUNT("subtotal"), SUM("subtotal"), COUNT("discount_total"), SUM("discount_total"), COUNT("grand_total"), SUM("grand_total") FROM "public"."orders"; | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("account_id"), SUM("account_id"), COUNT("promo_id"), SUM("promo_id"), COUNT("subtotal"), SUM("subtotal"), COUNT("discount_total"), SUM("discount_total"), COUNT("grand_total"), SUM("grand_total") FROM "orders"; | - |

## Table `public`.`shows`

- Translation status: `ready`
- Load status: `verified`
- DDL applied: `yes`
- Source row count: `800`
- Target row count: `800`
- Load timing: total `694.69 ms`, export `14.17 ms`, literal build `5.03 ms`, Snowdrop insert `675.48 ms`, chunks `1`, trusted bulk load `true`

### Replay Checks

| Name | Matched | Source SQL | Target SQL | Note |
|---|---|---|---|---|
| row_count | yes | SELECT COUNT(*) FROM "public"."shows"; | SELECT COUNT(*) FROM "shows"; | - |
| ordered_sample | yes | SELECT "id", "event_id", "show_code", "show_day", "seat_capacity", "sold_count" FROM "public"."shows" ORDER BY "id" LIMIT 25; | SELECT "id", "event_id", "show_code", "show_day", "seat_capacity", "sold_count" FROM "shows" ORDER BY "id" LIMIT 25; | - |
| numeric_aggregates | yes | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("event_id"), SUM("event_id"), COUNT("seat_capacity"), SUM("seat_capacity"), COUNT("sold_count"), SUM("sold_count") FROM "public"."shows"; | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("event_id"), SUM("event_id"), COUNT("seat_capacity"), SUM("seat_capacity"), COUNT("sold_count"), SUM("sold_count") FROM "shows"; | - |

## Table `public`.`order_items`

- Translation status: `ready`
- Load status: `verified`
- DDL applied: `yes`
- Source row count: `3200`
- Target row count: `3200`
- Load timing: total `2778.46 ms`, export `55.03 ms`, literal build `18.25 ms`, Snowdrop insert `2705.16 ms`, chunks `4`, trusted bulk load `true`

### Replay Checks

| Name | Matched | Source SQL | Target SQL | Note |
|---|---|---|---|---|
| row_count | yes | SELECT COUNT(*) FROM "public"."order_items"; | SELECT COUNT(*) FROM "order_items"; | - |
| ordered_sample | yes | SELECT "id", "order_id", "show_id", "line_no", "quantity", "unit_price", "line_total" FROM "public"."order_items" ORDER BY "id" LIMIT 25; | SELECT "id", "order_id", "show_id", "line_no", "quantity", "unit_price", "line_total" FROM "order_items" ORDER BY "id" LIMIT 25; | - |
| numeric_aggregates | yes | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("order_id"), SUM("order_id"), COUNT("show_id"), SUM("show_id"), COUNT("line_no"), SUM("line_no"), COUNT("quantity"), SUM("quantity"), COUNT("unit_price"), SUM("unit_price"), COUNT("line_total"), SUM("line_total") FROM "public"."order_items"; | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("order_id"), SUM("order_id"), COUNT("show_id"), SUM("show_id"), COUNT("line_no"), SUM("line_no"), COUNT("quantity"), SUM("quantity"), COUNT("unit_price"), SUM("unit_price"), COUNT("line_total"), SUM("line_total") FROM "order_items"; | - |

## Table `public`.`payments`

- Translation status: `ready`
- Load status: `verified`
- DDL applied: `yes`
- Source row count: `1600`
- Target row count: `1600`
- Load timing: total `1241.24 ms`, export `25.22 ms`, literal build `8.53 ms`, Snowdrop insert `1207.47 ms`, chunks `2`, trusted bulk load `true`

### Replay Checks

| Name | Matched | Source SQL | Target SQL | Note |
|---|---|---|---|---|
| row_count | yes | SELECT COUNT(*) FROM "public"."payments"; | SELECT COUNT(*) FROM "payments"; | - |
| ordered_sample | yes | SELECT "id", "order_id", "payment_status", "amount", "paid_at", "provider_ref" FROM "public"."payments" ORDER BY "id" LIMIT 25; | SELECT "id", "order_id", "payment_status", "amount", "paid_at", "provider_ref" FROM "payments" ORDER BY "id" LIMIT 25; | - |
| numeric_aggregates | yes | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("order_id"), SUM("order_id"), COUNT("amount"), SUM("amount") FROM "public"."payments"; | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("order_id"), SUM("order_id"), COUNT("amount"), SUM("amount") FROM "payments"; | - |

## Table `public`.`seat_holds`

- Translation status: `ready`
- Load status: `verified`
- DDL applied: `yes`
- Source row count: `1000`
- Target row count: `1000`
- Load timing: total `753.71 ms`, export `15.11 ms`, literal build `5.33 ms`, Snowdrop insert `733.25 ms`, chunks `1`, trusted bulk load `true`

### Replay Checks

| Name | Matched | Source SQL | Target SQL | Note |
|---|---|---|---|---|
| row_count | yes | SELECT COUNT(*) FROM "public"."seat_holds"; | SELECT COUNT(*) FROM "seat_holds"; | - |
| ordered_sample | yes | SELECT "id", "show_id", "customer_id", "seat_label", "hold_status", "expires_at" FROM "public"."seat_holds" ORDER BY "id" LIMIT 25; | SELECT "id", "show_id", "customer_id", "seat_label", "hold_status", "expires_at" FROM "seat_holds" ORDER BY "id" LIMIT 25; | - |
| numeric_aggregates | yes | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("show_id"), SUM("show_id"), COUNT("customer_id"), SUM("customer_id") FROM "public"."seat_holds"; | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("show_id"), SUM("show_id"), COUNT("customer_id"), SUM("customer_id") FROM "seat_holds"; | - |

## Table `public`.`refunds`

- Translation status: `ready`
- Load status: `verified`
- DDL applied: `yes`
- Source row count: `84`
- Target row count: `84`
- Load timing: total `189.81 ms`, export `1.74 ms`, literal build `0.37 ms`, Snowdrop insert `187.69 ms`, chunks `1`, trusted bulk load `true`

### Replay Checks

| Name | Matched | Source SQL | Target SQL | Note |
|---|---|---|---|---|
| row_count | yes | SELECT COUNT(*) FROM "public"."refunds"; | SELECT COUNT(*) FROM "refunds"; | - |
| ordered_sample | yes | SELECT "id", "payment_id", "refund_status", "amount", "refunded_at" FROM "public"."refunds" ORDER BY "id" LIMIT 25; | SELECT "id", "payment_id", "refund_status", "amount", "refunded_at" FROM "refunds" ORDER BY "id" LIMIT 25; | - |
| numeric_aggregates | yes | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("payment_id"), SUM("payment_id"), COUNT("amount"), SUM("amount") FROM "public"."refunds"; | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("payment_id"), SUM("payment_id"), COUNT("amount"), SUM("amount") FROM "refunds"; | - |

## Table `public`.`tickets`

- Translation status: `ready`
- Load status: `verified`
- DDL applied: `yes`
- Source row count: `3200`
- Target row count: `3200`
- Load timing: total `2660.49 ms`, export `56.92 ms`, literal build `22.23 ms`, Snowdrop insert `2581.31 ms`, chunks `4`, trusted bulk load `true`

### Replay Checks

| Name | Matched | Source SQL | Target SQL | Note |
|---|---|---|---|---|
| row_count | yes | SELECT COUNT(*) FROM "public"."tickets"; | SELECT COUNT(*) FROM "tickets"; | - |
| ordered_sample | yes | SELECT "id", "order_item_id", "show_id", "ticket_code", "seat_label", "ticket_status", "issued_at" FROM "public"."tickets" ORDER BY "id" LIMIT 25; | SELECT "id", "order_item_id", "show_id", "ticket_code", "seat_label", "ticket_status", "issued_at" FROM "tickets" ORDER BY "id" LIMIT 25; | - |
| numeric_aggregates | yes | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("order_item_id"), SUM("order_item_id"), COUNT("show_id"), SUM("show_id") FROM "public"."tickets"; | SELECT COUNT(*), COUNT("id"), SUM("id"), COUNT("order_item_id"), SUM("order_item_id"), COUNT("show_id"), SUM("show_id") FROM "tickets"; | - |

