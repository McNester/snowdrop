# Snowdrop Migration Preflight

Generated at: `2026-05-11T03:53:13.187794542+00:00`

- Source PostgreSQL database: `ticketing_orders_large`
- Schema filter: `all non-system schemas`

## Summary

| Metric | Value |
|---|---|
| Tables | 13 |
| Columns | 78 |
| Constraints | 51 |
| Indexes | 29 |
| Supported tables | 0 |
| Partial tables | 13 |
| Unsupported tables | 0 |

| DDL-ready tables | 0 |
| DDL-partial tables | 13 |
| DDL-blocked tables | 0 |

## Findings By Status

| Status | Count |
|---|---|
| partial | 43 |

## Table Compatibility

| Schema | Table | Status | Translation | Columns | Constraints | Indexes |
|---|---|---|---|---|---|---|
| public | accounts | partial | partial | 5 | 4 | 2 |
| public | audit_events | partial | partial | 6 | 2 | 2 |
| public | customers | partial | partial | 6 | 3 | 2 |
| public | events | partial | partial | 7 | 4 | 3 |
| public | order_items | partial | partial | 7 | 7 | 3 |
| public | orders | partial | partial | 8 | 6 | 2 |
| public | payments | partial | partial | 6 | 4 | 3 |
| public | promotions | partial | partial | 4 | 3 | 2 |
| public | refunds | partial | partial | 5 | 3 | 1 |
| public | seat_holds | partial | partial | 6 | 4 | 2 |
| public | shows | partial | partial | 6 | 4 | 3 |
| public | tickets | partial | partial | 7 | 4 | 2 |
| public | venues | partial | partial | 5 | 3 | 2 |

## Compatibility Findings

| Status | Object | Category | Message | Recommendation |
|---|---|---|---|---|
| partial | constraint `public.accounts.accounts_credit_limit_check` | constraint | CHECK constraint needs manual review during migration | validate this constraint against the current Snowdrop subset before applying it automatically |
| partial | index `public.accounts.accounts_customer_id_key` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | index `public.accounts.accounts_pkey` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | index `public.audit_events.audit_events_pkey` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | index `public.audit_events.idx_audit_events_account_day` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | constraint `public.customers.customers_tier_check` | constraint | CHECK constraint needs manual review during migration | validate this constraint against the current Snowdrop subset before applying it automatically |
| partial | index `public.customers.customers_email_key` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | index `public.customers.customers_pkey` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | constraint `public.events.events_base_price_check` | constraint | CHECK constraint needs manual review during migration | validate this constraint against the current Snowdrop subset before applying it automatically |
| partial | index `public.events.events_event_code_key` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | index `public.events.events_pkey` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | index `public.events.idx_events_starts_on` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | constraint `public.order_items.order_items_line_total_check` | constraint | CHECK constraint needs manual review during migration | validate this constraint against the current Snowdrop subset before applying it automatically |
| partial | constraint `public.order_items.order_items_quantity_check` | constraint | CHECK constraint needs manual review during migration | validate this constraint against the current Snowdrop subset before applying it automatically |
| partial | constraint `public.order_items.order_items_unit_price_check` | constraint | CHECK constraint needs manual review during migration | validate this constraint against the current Snowdrop subset before applying it automatically |
| partial | index `public.order_items.idx_order_items_show_id` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | index `public.order_items.order_items_order_id_line_no_key` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | index `public.order_items.order_items_pkey` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | constraint `public.orders.orders_discount_total_check` | constraint | CHECK constraint needs manual review during migration | validate this constraint against the current Snowdrop subset before applying it automatically |
| partial | constraint `public.orders.orders_grand_total_check` | constraint | CHECK constraint needs manual review during migration | validate this constraint against the current Snowdrop subset before applying it automatically |
| partial | constraint `public.orders.orders_subtotal_check` | constraint | CHECK constraint needs manual review during migration | validate this constraint against the current Snowdrop subset before applying it automatically |
| partial | index `public.orders.idx_orders_account_ordered_at` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | index `public.orders.orders_pkey` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | constraint `public.payments.payments_amount_check` | constraint | CHECK constraint needs manual review during migration | validate this constraint against the current Snowdrop subset before applying it automatically |
| partial | index `public.payments.idx_payments_order_id` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | index `public.payments.payments_pkey` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | index `public.payments.payments_provider_ref_key` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | constraint `public.promotions.promotions_discount_percent_check` | constraint | CHECK constraint needs manual review during migration | validate this constraint against the current Snowdrop subset before applying it automatically |
| partial | index `public.promotions.promotions_pkey` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | index `public.promotions.promotions_promo_code_key` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | constraint `public.refunds.refunds_amount_check` | constraint | CHECK constraint needs manual review during migration | validate this constraint against the current Snowdrop subset before applying it automatically |
| partial | index `public.refunds.refunds_pkey` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | index `public.seat_holds.seat_holds_pkey` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | index `public.seat_holds.seat_holds_show_id_seat_label_key` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | constraint `public.shows.shows_check` | constraint | CHECK constraint needs manual review during migration | validate this constraint against the current Snowdrop subset before applying it automatically |
| partial | index `public.shows.idx_shows_event_day` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | index `public.shows.shows_pkey` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | index `public.shows.shows_show_code_key` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | index `public.tickets.tickets_pkey` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | index `public.tickets.tickets_ticket_code_key` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | constraint `public.venues.venues_capacity_check` | constraint | CHECK constraint needs manual review during migration | validate this constraint against the current Snowdrop subset before applying it automatically |
| partial | index `public.venues.venues_pkey` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |
| partial | index `public.venues.venues_venue_code_key` | index | index definition needs review during migration | review whether this index is required for the first migration cut and recreate it after load if needed |

## Partial Feature Inventory

### constraint

- constraint public.accounts.accounts_credit_limit_check -> CHECK constraint needs manual review during migration
- constraint public.customers.customers_tier_check -> CHECK constraint needs manual review during migration
- constraint public.events.events_base_price_check -> CHECK constraint needs manual review during migration
- constraint public.order_items.order_items_line_total_check -> CHECK constraint needs manual review during migration
- constraint public.order_items.order_items_quantity_check -> CHECK constraint needs manual review during migration
- constraint public.order_items.order_items_unit_price_check -> CHECK constraint needs manual review during migration
- constraint public.orders.orders_discount_total_check -> CHECK constraint needs manual review during migration
- constraint public.orders.orders_grand_total_check -> CHECK constraint needs manual review during migration
- constraint public.orders.orders_subtotal_check -> CHECK constraint needs manual review during migration
- constraint public.payments.payments_amount_check -> CHECK constraint needs manual review during migration
- constraint public.promotions.promotions_discount_percent_check -> CHECK constraint needs manual review during migration
- constraint public.refunds.refunds_amount_check -> CHECK constraint needs manual review during migration
- constraint public.shows.shows_check -> CHECK constraint needs manual review during migration
- constraint public.venues.venues_capacity_check -> CHECK constraint needs manual review during migration

### index

- index public.accounts.accounts_customer_id_key -> index definition needs review during migration
- index public.accounts.accounts_pkey -> index definition needs review during migration
- index public.audit_events.audit_events_pkey -> index definition needs review during migration
- index public.audit_events.idx_audit_events_account_day -> index definition needs review during migration
- index public.customers.customers_email_key -> index definition needs review during migration
- index public.customers.customers_pkey -> index definition needs review during migration
- index public.events.events_event_code_key -> index definition needs review during migration
- index public.events.events_pkey -> index definition needs review during migration
- index public.events.idx_events_starts_on -> index definition needs review during migration
- index public.order_items.idx_order_items_show_id -> index definition needs review during migration
- index public.order_items.order_items_order_id_line_no_key -> index definition needs review during migration
- index public.order_items.order_items_pkey -> index definition needs review during migration
- index public.orders.idx_orders_account_ordered_at -> index definition needs review during migration
- index public.orders.orders_pkey -> index definition needs review during migration
- index public.payments.idx_payments_order_id -> index definition needs review during migration
- index public.payments.payments_pkey -> index definition needs review during migration
- index public.payments.payments_provider_ref_key -> index definition needs review during migration
- index public.promotions.promotions_pkey -> index definition needs review during migration
- index public.promotions.promotions_promo_code_key -> index definition needs review during migration
- index public.refunds.refunds_pkey -> index definition needs review during migration
- index public.seat_holds.seat_holds_pkey -> index definition needs review during migration
- index public.seat_holds.seat_holds_show_id_seat_label_key -> index definition needs review during migration
- index public.shows.idx_shows_event_day -> index definition needs review during migration
- index public.shows.shows_pkey -> index definition needs review during migration
- index public.shows.shows_show_code_key -> index definition needs review during migration
- index public.tickets.tickets_pkey -> index definition needs review during migration
- index public.tickets.tickets_ticket_code_key -> index definition needs review during migration
- index public.venues.venues_pkey -> index definition needs review during migration
- index public.venues.venues_venue_code_key -> index definition needs review during migration

## Table `public`.`accounts`

- Overall status: `partial`
- Translation preview status: `partial`
- table needs manual review for partial features before a production migration
- Translation note: index 'accounts_customer_id_key' is omitted from the preview and should be recreated manually if still needed
- Translation note: index 'accounts_pkey' is omitted from the preview and should be recreated manually if still needed

### Columns

| Column | Source type | Snowdrop type | Nullable | Status | Notes |
|---|---|---|---|---|---|
| id | bigint | BIGINT | no | supported |  |
| customer_id | bigint | BIGINT | no | supported |  |
| status | varchar(16) | VARCHAR(16) | no | supported |  |
| credit_limit | numeric(12,2) | NUMERIC(12,2) | no | supported |  |
| opened_on | date | DATE | no | supported |  |

### Constraints

| Name | Kind | Columns | Status | Reference | Notes |
|---|---|---|---|---|---|
| accounts_credit_limit_check | CHECK |  | partial | - | CHECK constraints are supported only for the current safe row-local scalar subset |
| accounts_customer_id_fkey | FOREIGN KEY | customer_id | supported | public.customers(id) |  |
| accounts_customer_id_key | UNIQUE | customer_id | supported | - |  |
| accounts_pkey | PRIMARY KEY | id | supported | - |  |

### Indexes

| Name | Status | Definition |
|---|---|---|
| accounts_customer_id_key | partial | CREATE UNIQUE INDEX accounts_customer_id_key ON public.accounts USING btree (customer_id) |
| accounts_pkey | partial | CREATE UNIQUE INDEX accounts_pkey ON public.accounts USING btree (id) |

### Translation Preview

```sql
-- Source table public.accounts
CREATE TABLE "accounts" (
  "id" BIGINT NOT NULL,
  "customer_id" BIGINT NOT NULL,
  "status" VARCHAR(16) NOT NULL DEFAULT 'active',
  "credit_limit" NUMERIC(12,2) NOT NULL DEFAULT 0,
  "opened_on" DATE NOT NULL,
  CHECK (((credit_limit >= (0)))),
  FOREIGN KEY ("customer_id") REFERENCES "customers" ("id"),
  UNIQUE ("customer_id"),
  PRIMARY KEY ("id")
);

-- NOTE: index 'accounts_customer_id_key' is omitted from the preview and should be recreated manually if still needed
-- NOTE: index 'accounts_pkey' is omitted from the preview and should be recreated manually if still needed
```

## Table `public`.`audit_events`

- Overall status: `partial`
- Translation preview status: `partial`
- table needs manual review for partial features before a production migration
- Translation note: index 'audit_events_pkey' is omitted from the preview and should be recreated manually if still needed
- Translation note: index 'idx_audit_events_account_day' is omitted from the preview and should be recreated manually if still needed

### Columns

| Column | Source type | Snowdrop type | Nullable | Status | Notes |
|---|---|---|---|---|---|
| id | bigint | BIGINT | no | supported |  |
| account_id | bigint | BIGINT | no | supported |  |
| event_kind | varchar(32) | VARCHAR(32) | no | supported |  |
| created_at | timestamp without time zone | TIMESTAMP | no | supported |  |
| event_day | date | DATE | no | supported |  |
| payload_hint | varchar(160) | VARCHAR(160) | yes | supported |  |

### Constraints

| Name | Kind | Columns | Status | Reference | Notes |
|---|---|---|---|---|---|
| audit_events_account_id_fkey | FOREIGN KEY | account_id | supported | public.accounts(id) |  |
| audit_events_pkey | PRIMARY KEY | id | supported | - |  |

### Indexes

| Name | Status | Definition |
|---|---|---|
| audit_events_pkey | partial | CREATE UNIQUE INDEX audit_events_pkey ON public.audit_events USING btree (id) |
| idx_audit_events_account_day | partial | CREATE INDEX idx_audit_events_account_day ON public.audit_events USING btree (account_id, event_day) |

### Translation Preview

```sql
-- Source table public.audit_events
CREATE TABLE "audit_events" (
  "id" BIGINT NOT NULL,
  "account_id" BIGINT NOT NULL,
  "event_kind" VARCHAR(32) NOT NULL,
  "created_at" TIMESTAMP NOT NULL,
  "event_day" DATE NOT NULL,
  "payload_hint" VARCHAR(160),
  FOREIGN KEY ("account_id") REFERENCES "accounts" ("id"),
  PRIMARY KEY ("id")
);

-- NOTE: index 'audit_events_pkey' is omitted from the preview and should be recreated manually if still needed
-- NOTE: index 'idx_audit_events_account_day' is omitted from the preview and should be recreated manually if still needed
```

## Table `public`.`customers`

- Overall status: `partial`
- Translation preview status: `partial`
- table needs manual review for partial features before a production migration
- Translation note: index 'customers_email_key' is omitted from the preview and should be recreated manually if still needed
- Translation note: index 'customers_pkey' is omitted from the preview and should be recreated manually if still needed

### Columns

| Column | Source type | Snowdrop type | Nullable | Status | Notes |
|---|---|---|---|---|---|
| id | bigint | BIGINT | no | supported |  |
| email | varchar(255) | VARCHAR(255) | no | supported |  |
| full_name | varchar(128) | VARCHAR(128) | no | supported |  |
| tier | varchar(16) | VARCHAR(16) | no | supported |  |
| marketing_opt_in | boolean | BOOLEAN | no | supported |  |
| created_on | date | DATE | no | supported |  |

### Constraints

| Name | Kind | Columns | Status | Reference | Notes |
|---|---|---|---|---|---|
| customers_email_key | UNIQUE | email | supported | - |  |
| customers_pkey | PRIMARY KEY | id | supported | - |  |
| customers_tier_check | CHECK |  | partial | - | CHECK constraints are supported only for the current safe row-local scalar subset |

### Indexes

| Name | Status | Definition |
|---|---|---|
| customers_email_key | partial | CREATE UNIQUE INDEX customers_email_key ON public.customers USING btree (email) |
| customers_pkey | partial | CREATE UNIQUE INDEX customers_pkey ON public.customers USING btree (id) |

### Translation Preview

```sql
-- Source table public.customers
CREATE TABLE "customers" (
  "id" BIGINT NOT NULL,
  "email" VARCHAR(255) NOT NULL,
  "full_name" VARCHAR(128) NOT NULL,
  "tier" VARCHAR(16) NOT NULL DEFAULT 'standard',
  "marketing_opt_in" BOOLEAN NOT NULL DEFAULT false,
  "created_on" DATE NOT NULL,
  UNIQUE ("email"),
  PRIMARY KEY ("id"),
  CHECK (((((tier) = 'standard') OR ((tier) = 'vip') OR ((tier) = 'enterprise'))))
);

-- NOTE: index 'customers_email_key' is omitted from the preview and should be recreated manually if still needed
-- NOTE: index 'customers_pkey' is omitted from the preview and should be recreated manually if still needed
```

## Table `public`.`events`

- Overall status: `partial`
- Translation preview status: `partial`
- table needs manual review for partial features before a production migration
- Translation note: index 'events_event_code_key' is omitted from the preview and should be recreated manually if still needed
- Translation note: index 'events_pkey' is omitted from the preview and should be recreated manually if still needed
- Translation note: index 'idx_events_starts_on' is omitted from the preview and should be recreated manually if still needed

### Columns

| Column | Source type | Snowdrop type | Nullable | Status | Notes |
|---|---|---|---|---|---|
| id | bigint | BIGINT | no | supported |  |
| venue_id | bigint | BIGINT | no | supported |  |
| event_code | varchar(32) | VARCHAR(32) | no | supported |  |
| title | varchar(160) | VARCHAR(160) | no | supported |  |
| category | varchar(32) | VARCHAR(32) | no | supported |  |
| starts_on | date | DATE | no | supported |  |
| base_price | numeric(10,2) | NUMERIC(10,2) | no | supported |  |

### Constraints

| Name | Kind | Columns | Status | Reference | Notes |
|---|---|---|---|---|---|
| events_base_price_check | CHECK |  | partial | - | CHECK constraints are supported only for the current safe row-local scalar subset |
| events_event_code_key | UNIQUE | event_code | supported | - |  |
| events_pkey | PRIMARY KEY | id | supported | - |  |
| events_venue_id_fkey | FOREIGN KEY | venue_id | supported | public.venues(id) |  |

### Indexes

| Name | Status | Definition |
|---|---|---|
| events_event_code_key | partial | CREATE UNIQUE INDEX events_event_code_key ON public.events USING btree (event_code) |
| events_pkey | partial | CREATE UNIQUE INDEX events_pkey ON public.events USING btree (id) |
| idx_events_starts_on | partial | CREATE INDEX idx_events_starts_on ON public.events USING btree (starts_on) |

### Translation Preview

```sql
-- Source table public.events
CREATE TABLE "events" (
  "id" BIGINT NOT NULL,
  "venue_id" BIGINT NOT NULL,
  "event_code" VARCHAR(32) NOT NULL,
  "title" VARCHAR(160) NOT NULL,
  "category" VARCHAR(32) NOT NULL,
  "starts_on" DATE NOT NULL,
  "base_price" NUMERIC(10,2) NOT NULL,
  CHECK (((base_price >= (0)))),
  UNIQUE ("event_code"),
  PRIMARY KEY ("id"),
  FOREIGN KEY ("venue_id") REFERENCES "venues" ("id")
);

-- NOTE: index 'events_event_code_key' is omitted from the preview and should be recreated manually if still needed
-- NOTE: index 'events_pkey' is omitted from the preview and should be recreated manually if still needed
-- NOTE: index 'idx_events_starts_on' is omitted from the preview and should be recreated manually if still needed
```

## Table `public`.`order_items`

- Overall status: `partial`
- Translation preview status: `partial`
- table needs manual review for partial features before a production migration
- Translation note: index 'idx_order_items_show_id' is omitted from the preview and should be recreated manually if still needed
- Translation note: index 'order_items_order_id_line_no_key' is omitted from the preview and should be recreated manually if still needed
- Translation note: index 'order_items_pkey' is omitted from the preview and should be recreated manually if still needed

### Columns

| Column | Source type | Snowdrop type | Nullable | Status | Notes |
|---|---|---|---|---|---|
| id | bigint | BIGINT | no | supported |  |
| order_id | bigint | BIGINT | no | supported |  |
| show_id | bigint | BIGINT | no | supported |  |
| line_no | integer | INT | no | supported |  |
| quantity | integer | INT | no | supported |  |
| unit_price | numeric(10,2) | NUMERIC(10,2) | no | supported |  |
| line_total | numeric(12,2) | NUMERIC(12,2) | no | supported |  |

### Constraints

| Name | Kind | Columns | Status | Reference | Notes |
|---|---|---|---|---|---|
| order_items_line_total_check | CHECK |  | partial | - | CHECK constraints are supported only for the current safe row-local scalar subset |
| order_items_order_id_fkey | FOREIGN KEY | order_id | supported | public.orders(id) |  |
| order_items_order_id_line_no_key | UNIQUE | order_id, line_no | supported | - | multi-column UNIQUE is supported in the current narrow Phase 4 slice |
| order_items_pkey | PRIMARY KEY | id | supported | - |  |
| order_items_quantity_check | CHECK |  | partial | - | CHECK constraints are supported only for the current safe row-local scalar subset |
| order_items_show_id_fkey | FOREIGN KEY | show_id | supported | public.shows(id) |  |
| order_items_unit_price_check | CHECK |  | partial | - | CHECK constraints are supported only for the current safe row-local scalar subset |

### Indexes

| Name | Status | Definition |
|---|---|---|
| idx_order_items_show_id | partial | CREATE INDEX idx_order_items_show_id ON public.order_items USING btree (show_id) |
| order_items_order_id_line_no_key | partial | CREATE UNIQUE INDEX order_items_order_id_line_no_key ON public.order_items USING btree (order_id, line_no) |
| order_items_pkey | partial | CREATE UNIQUE INDEX order_items_pkey ON public.order_items USING btree (id) |

### Translation Preview

```sql
-- Source table public.order_items
CREATE TABLE "order_items" (
  "id" BIGINT NOT NULL,
  "order_id" BIGINT NOT NULL,
  "show_id" BIGINT NOT NULL,
  "line_no" INT NOT NULL,
  "quantity" INT NOT NULL,
  "unit_price" NUMERIC(10,2) NOT NULL,
  "line_total" NUMERIC(12,2) NOT NULL,
  CHECK (((line_total >= (0)))),
  FOREIGN KEY ("order_id") REFERENCES "orders" ("id"),
  UNIQUE ("order_id", "line_no"),
  PRIMARY KEY ("id"),
  CHECK (((quantity > 0))),
  FOREIGN KEY ("show_id") REFERENCES "shows" ("id"),
  CHECK (((unit_price >= (0))))
);

-- NOTE: index 'idx_order_items_show_id' is omitted from the preview and should be recreated manually if still needed
-- NOTE: index 'order_items_order_id_line_no_key' is omitted from the preview and should be recreated manually if still needed
-- NOTE: index 'order_items_pkey' is omitted from the preview and should be recreated manually if still needed
```

## Table `public`.`orders`

- Overall status: `partial`
- Translation preview status: `partial`
- table needs manual review for partial features before a production migration
- Translation note: index 'idx_orders_account_ordered_at' is omitted from the preview and should be recreated manually if still needed
- Translation note: index 'orders_pkey' is omitted from the preview and should be recreated manually if still needed

### Columns

| Column | Source type | Snowdrop type | Nullable | Status | Notes |
|---|---|---|---|---|---|
| id | bigint | BIGINT | no | supported |  |
| account_id | bigint | BIGINT | no | supported |  |
| promo_id | bigint | BIGINT | yes | supported |  |
| order_status | varchar(16) | VARCHAR(16) | no | supported |  |
| ordered_at | timestamp without time zone | TIMESTAMP | no | supported |  |
| subtotal | numeric(12,2) | NUMERIC(12,2) | no | supported |  |
| discount_total | numeric(12,2) | NUMERIC(12,2) | no | supported |  |
| grand_total | numeric(12,2) | NUMERIC(12,2) | no | supported |  |

### Constraints

| Name | Kind | Columns | Status | Reference | Notes |
|---|---|---|---|---|---|
| orders_account_id_fkey | FOREIGN KEY | account_id | supported | public.accounts(id) |  |
| orders_discount_total_check | CHECK |  | partial | - | CHECK constraints are supported only for the current safe row-local scalar subset |
| orders_grand_total_check | CHECK |  | partial | - | CHECK constraints are supported only for the current safe row-local scalar subset |
| orders_pkey | PRIMARY KEY | id | supported | - |  |
| orders_promo_id_fkey | FOREIGN KEY | promo_id | supported | public.promotions(id) |  |
| orders_subtotal_check | CHECK |  | partial | - | CHECK constraints are supported only for the current safe row-local scalar subset |

### Indexes

| Name | Status | Definition |
|---|---|---|
| idx_orders_account_ordered_at | partial | CREATE INDEX idx_orders_account_ordered_at ON public.orders USING btree (account_id, ordered_at) |
| orders_pkey | partial | CREATE UNIQUE INDEX orders_pkey ON public.orders USING btree (id) |

### Translation Preview

```sql
-- Source table public.orders
CREATE TABLE "orders" (
  "id" BIGINT NOT NULL,
  "account_id" BIGINT NOT NULL,
  "promo_id" BIGINT,
  "order_status" VARCHAR(16) NOT NULL DEFAULT 'placed',
  "ordered_at" TIMESTAMP NOT NULL,
  "subtotal" NUMERIC(12,2) NOT NULL,
  "discount_total" NUMERIC(12,2) NOT NULL DEFAULT 0,
  "grand_total" NUMERIC(12,2) NOT NULL,
  FOREIGN KEY ("account_id") REFERENCES "accounts" ("id"),
  CHECK (((discount_total >= (0)))),
  CHECK (((grand_total >= (0)))),
  PRIMARY KEY ("id"),
  FOREIGN KEY ("promo_id") REFERENCES "promotions" ("id"),
  CHECK (((subtotal >= (0))))
);

-- NOTE: index 'idx_orders_account_ordered_at' is omitted from the preview and should be recreated manually if still needed
-- NOTE: index 'orders_pkey' is omitted from the preview and should be recreated manually if still needed
```

## Table `public`.`payments`

- Overall status: `partial`
- Translation preview status: `partial`
- table needs manual review for partial features before a production migration
- Translation note: index 'idx_payments_order_id' is omitted from the preview and should be recreated manually if still needed
- Translation note: index 'payments_pkey' is omitted from the preview and should be recreated manually if still needed
- Translation note: index 'payments_provider_ref_key' is omitted from the preview and should be recreated manually if still needed

### Columns

| Column | Source type | Snowdrop type | Nullable | Status | Notes |
|---|---|---|---|---|---|
| id | bigint | BIGINT | no | supported |  |
| order_id | bigint | BIGINT | no | supported |  |
| payment_status | varchar(16) | VARCHAR(16) | no | supported |  |
| amount | numeric(12,2) | NUMERIC(12,2) | no | supported |  |
| paid_at | timestamp without time zone | TIMESTAMP | no | supported |  |
| provider_ref | varchar(64) | VARCHAR(64) | no | supported |  |

### Constraints

| Name | Kind | Columns | Status | Reference | Notes |
|---|---|---|---|---|---|
| payments_amount_check | CHECK |  | partial | - | CHECK constraints are supported only for the current safe row-local scalar subset |
| payments_order_id_fkey | FOREIGN KEY | order_id | supported | public.orders(id) |  |
| payments_pkey | PRIMARY KEY | id | supported | - |  |
| payments_provider_ref_key | UNIQUE | provider_ref | supported | - |  |

### Indexes

| Name | Status | Definition |
|---|---|---|
| idx_payments_order_id | partial | CREATE INDEX idx_payments_order_id ON public.payments USING btree (order_id) |
| payments_pkey | partial | CREATE UNIQUE INDEX payments_pkey ON public.payments USING btree (id) |
| payments_provider_ref_key | partial | CREATE UNIQUE INDEX payments_provider_ref_key ON public.payments USING btree (provider_ref) |

### Translation Preview

```sql
-- Source table public.payments
CREATE TABLE "payments" (
  "id" BIGINT NOT NULL,
  "order_id" BIGINT NOT NULL,
  "payment_status" VARCHAR(16) NOT NULL,
  "amount" NUMERIC(12,2) NOT NULL,
  "paid_at" TIMESTAMP NOT NULL,
  "provider_ref" VARCHAR(64) NOT NULL,
  CHECK (((amount >= (0)))),
  FOREIGN KEY ("order_id") REFERENCES "orders" ("id"),
  PRIMARY KEY ("id"),
  UNIQUE ("provider_ref")
);

-- NOTE: index 'idx_payments_order_id' is omitted from the preview and should be recreated manually if still needed
-- NOTE: index 'payments_pkey' is omitted from the preview and should be recreated manually if still needed
-- NOTE: index 'payments_provider_ref_key' is omitted from the preview and should be recreated manually if still needed
```

## Table `public`.`promotions`

- Overall status: `partial`
- Translation preview status: `partial`
- table needs manual review for partial features before a production migration
- Translation note: index 'promotions_pkey' is omitted from the preview and should be recreated manually if still needed
- Translation note: index 'promotions_promo_code_key' is omitted from the preview and should be recreated manually if still needed

### Columns

| Column | Source type | Snowdrop type | Nullable | Status | Notes |
|---|---|---|---|---|---|
| id | bigint | BIGINT | no | supported |  |
| promo_code | varchar(32) | VARCHAR(32) | no | supported |  |
| discount_percent | integer | INT | no | supported |  |
| active | boolean | BOOLEAN | no | supported |  |

### Constraints

| Name | Kind | Columns | Status | Reference | Notes |
|---|---|---|---|---|---|
| promotions_discount_percent_check | CHECK |  | partial | - | CHECK constraints are supported only for the current safe row-local scalar subset |
| promotions_pkey | PRIMARY KEY | id | supported | - |  |
| promotions_promo_code_key | UNIQUE | promo_code | supported | - |  |

### Indexes

| Name | Status | Definition |
|---|---|---|
| promotions_pkey | partial | CREATE UNIQUE INDEX promotions_pkey ON public.promotions USING btree (id) |
| promotions_promo_code_key | partial | CREATE UNIQUE INDEX promotions_promo_code_key ON public.promotions USING btree (promo_code) |

### Translation Preview

```sql
-- Source table public.promotions
CREATE TABLE "promotions" (
  "id" BIGINT NOT NULL,
  "promo_code" VARCHAR(32) NOT NULL,
  "discount_percent" INT NOT NULL,
  "active" BOOLEAN NOT NULL DEFAULT true,
  CHECK ((((discount_percent >= 0) AND (discount_percent <= 80)))),
  PRIMARY KEY ("id"),
  UNIQUE ("promo_code")
);

-- NOTE: index 'promotions_pkey' is omitted from the preview and should be recreated manually if still needed
-- NOTE: index 'promotions_promo_code_key' is omitted from the preview and should be recreated manually if still needed
```

## Table `public`.`refunds`

- Overall status: `partial`
- Translation preview status: `partial`
- table needs manual review for partial features before a production migration
- Translation note: index 'refunds_pkey' is omitted from the preview and should be recreated manually if still needed

### Columns

| Column | Source type | Snowdrop type | Nullable | Status | Notes |
|---|---|---|---|---|---|
| id | bigint | BIGINT | no | supported |  |
| payment_id | bigint | BIGINT | no | supported |  |
| refund_status | varchar(16) | VARCHAR(16) | no | supported |  |
| amount | numeric(12,2) | NUMERIC(12,2) | no | supported |  |
| refunded_at | timestamp without time zone | TIMESTAMP | no | supported |  |

### Constraints

| Name | Kind | Columns | Status | Reference | Notes |
|---|---|---|---|---|---|
| refunds_amount_check | CHECK |  | partial | - | CHECK constraints are supported only for the current safe row-local scalar subset |
| refunds_payment_id_fkey | FOREIGN KEY | payment_id | supported | public.payments(id) |  |
| refunds_pkey | PRIMARY KEY | id | supported | - |  |

### Indexes

| Name | Status | Definition |
|---|---|---|
| refunds_pkey | partial | CREATE UNIQUE INDEX refunds_pkey ON public.refunds USING btree (id) |

### Translation Preview

```sql
-- Source table public.refunds
CREATE TABLE "refunds" (
  "id" BIGINT NOT NULL,
  "payment_id" BIGINT NOT NULL,
  "refund_status" VARCHAR(16) NOT NULL,
  "amount" NUMERIC(12,2) NOT NULL,
  "refunded_at" TIMESTAMP NOT NULL,
  CHECK (((amount >= (0)))),
  FOREIGN KEY ("payment_id") REFERENCES "payments" ("id"),
  PRIMARY KEY ("id")
);

-- NOTE: index 'refunds_pkey' is omitted from the preview and should be recreated manually if still needed
```

## Table `public`.`seat_holds`

- Overall status: `partial`
- Translation preview status: `partial`
- table needs manual review for partial features before a production migration
- Translation note: index 'seat_holds_pkey' is omitted from the preview and should be recreated manually if still needed
- Translation note: index 'seat_holds_show_id_seat_label_key' is omitted from the preview and should be recreated manually if still needed

### Columns

| Column | Source type | Snowdrop type | Nullable | Status | Notes |
|---|---|---|---|---|---|
| id | bigint | BIGINT | no | supported |  |
| show_id | bigint | BIGINT | no | supported |  |
| customer_id | bigint | BIGINT | no | supported |  |
| seat_label | varchar(16) | VARCHAR(16) | no | supported |  |
| hold_status | varchar(16) | VARCHAR(16) | no | supported |  |
| expires_at | timestamp without time zone | TIMESTAMP | no | supported |  |

### Constraints

| Name | Kind | Columns | Status | Reference | Notes |
|---|---|---|---|---|---|
| seat_holds_customer_id_fkey | FOREIGN KEY | customer_id | supported | public.customers(id) |  |
| seat_holds_pkey | PRIMARY KEY | id | supported | - |  |
| seat_holds_show_id_fkey | FOREIGN KEY | show_id | supported | public.shows(id) |  |
| seat_holds_show_id_seat_label_key | UNIQUE | show_id, seat_label | supported | - | multi-column UNIQUE is supported in the current narrow Phase 4 slice |

### Indexes

| Name | Status | Definition |
|---|---|---|
| seat_holds_pkey | partial | CREATE UNIQUE INDEX seat_holds_pkey ON public.seat_holds USING btree (id) |
| seat_holds_show_id_seat_label_key | partial | CREATE UNIQUE INDEX seat_holds_show_id_seat_label_key ON public.seat_holds USING btree (show_id, seat_label) |

### Translation Preview

```sql
-- Source table public.seat_holds
CREATE TABLE "seat_holds" (
  "id" BIGINT NOT NULL,
  "show_id" BIGINT NOT NULL,
  "customer_id" BIGINT NOT NULL,
  "seat_label" VARCHAR(16) NOT NULL,
  "hold_status" VARCHAR(16) NOT NULL DEFAULT 'held',
  "expires_at" TIMESTAMP NOT NULL,
  FOREIGN KEY ("customer_id") REFERENCES "customers" ("id"),
  PRIMARY KEY ("id"),
  FOREIGN KEY ("show_id") REFERENCES "shows" ("id"),
  UNIQUE ("show_id", "seat_label")
);

-- NOTE: index 'seat_holds_pkey' is omitted from the preview and should be recreated manually if still needed
-- NOTE: index 'seat_holds_show_id_seat_label_key' is omitted from the preview and should be recreated manually if still needed
```

## Table `public`.`shows`

- Overall status: `partial`
- Translation preview status: `partial`
- table needs manual review for partial features before a production migration
- Translation note: index 'idx_shows_event_day' is omitted from the preview and should be recreated manually if still needed
- Translation note: index 'shows_pkey' is omitted from the preview and should be recreated manually if still needed
- Translation note: index 'shows_show_code_key' is omitted from the preview and should be recreated manually if still needed

### Columns

| Column | Source type | Snowdrop type | Nullable | Status | Notes |
|---|---|---|---|---|---|
| id | bigint | BIGINT | no | supported |  |
| event_id | bigint | BIGINT | no | supported |  |
| show_code | varchar(32) | VARCHAR(32) | no | supported |  |
| show_day | date | DATE | no | supported |  |
| seat_capacity | integer | INT | no | supported |  |
| sold_count | integer | INT | no | supported |  |

### Constraints

| Name | Kind | Columns | Status | Reference | Notes |
|---|---|---|---|---|---|
| shows_check | CHECK |  | partial | - | CHECK constraints are supported only for the current safe row-local scalar subset |
| shows_event_id_fkey | FOREIGN KEY | event_id | supported | public.events(id) |  |
| shows_pkey | PRIMARY KEY | id | supported | - |  |
| shows_show_code_key | UNIQUE | show_code | supported | - |  |

### Indexes

| Name | Status | Definition |
|---|---|---|
| idx_shows_event_day | partial | CREATE INDEX idx_shows_event_day ON public.shows USING btree (event_id, show_day) |
| shows_pkey | partial | CREATE UNIQUE INDEX shows_pkey ON public.shows USING btree (id) |
| shows_show_code_key | partial | CREATE UNIQUE INDEX shows_show_code_key ON public.shows USING btree (show_code) |

### Translation Preview

```sql
-- Source table public.shows
CREATE TABLE "shows" (
  "id" BIGINT NOT NULL,
  "event_id" BIGINT NOT NULL,
  "show_code" VARCHAR(32) NOT NULL,
  "show_day" DATE NOT NULL,
  "seat_capacity" INT NOT NULL,
  "sold_count" INT NOT NULL DEFAULT 0,
  CHECK (((seat_capacity >= sold_count))),
  FOREIGN KEY ("event_id") REFERENCES "events" ("id"),
  PRIMARY KEY ("id"),
  UNIQUE ("show_code")
);

-- NOTE: index 'idx_shows_event_day' is omitted from the preview and should be recreated manually if still needed
-- NOTE: index 'shows_pkey' is omitted from the preview and should be recreated manually if still needed
-- NOTE: index 'shows_show_code_key' is omitted from the preview and should be recreated manually if still needed
```

## Table `public`.`tickets`

- Overall status: `partial`
- Translation preview status: `partial`
- table needs manual review for partial features before a production migration
- Translation note: index 'tickets_pkey' is omitted from the preview and should be recreated manually if still needed
- Translation note: index 'tickets_ticket_code_key' is omitted from the preview and should be recreated manually if still needed

### Columns

| Column | Source type | Snowdrop type | Nullable | Status | Notes |
|---|---|---|---|---|---|
| id | bigint | BIGINT | no | supported |  |
| order_item_id | bigint | BIGINT | no | supported |  |
| show_id | bigint | BIGINT | no | supported |  |
| ticket_code | varchar(48) | VARCHAR(48) | no | supported |  |
| seat_label | varchar(16) | VARCHAR(16) | no | supported |  |
| ticket_status | varchar(16) | VARCHAR(16) | no | supported |  |
| issued_at | timestamp without time zone | TIMESTAMP | no | supported |  |

### Constraints

| Name | Kind | Columns | Status | Reference | Notes |
|---|---|---|---|---|---|
| tickets_order_item_id_fkey | FOREIGN KEY | order_item_id | supported | public.order_items(id) |  |
| tickets_pkey | PRIMARY KEY | id | supported | - |  |
| tickets_show_id_fkey | FOREIGN KEY | show_id | supported | public.shows(id) |  |
| tickets_ticket_code_key | UNIQUE | ticket_code | supported | - |  |

### Indexes

| Name | Status | Definition |
|---|---|---|
| tickets_pkey | partial | CREATE UNIQUE INDEX tickets_pkey ON public.tickets USING btree (id) |
| tickets_ticket_code_key | partial | CREATE UNIQUE INDEX tickets_ticket_code_key ON public.tickets USING btree (ticket_code) |

### Translation Preview

```sql
-- Source table public.tickets
CREATE TABLE "tickets" (
  "id" BIGINT NOT NULL,
  "order_item_id" BIGINT NOT NULL,
  "show_id" BIGINT NOT NULL,
  "ticket_code" VARCHAR(48) NOT NULL,
  "seat_label" VARCHAR(16) NOT NULL,
  "ticket_status" VARCHAR(16) NOT NULL DEFAULT 'issued',
  "issued_at" TIMESTAMP NOT NULL,
  FOREIGN KEY ("order_item_id") REFERENCES "order_items" ("id"),
  PRIMARY KEY ("id"),
  FOREIGN KEY ("show_id") REFERENCES "shows" ("id"),
  UNIQUE ("ticket_code")
);

-- NOTE: index 'tickets_pkey' is omitted from the preview and should be recreated manually if still needed
-- NOTE: index 'tickets_ticket_code_key' is omitted from the preview and should be recreated manually if still needed
```

## Table `public`.`venues`

- Overall status: `partial`
- Translation preview status: `partial`
- table needs manual review for partial features before a production migration
- Translation note: index 'venues_pkey' is omitted from the preview and should be recreated manually if still needed
- Translation note: index 'venues_venue_code_key' is omitted from the preview and should be recreated manually if still needed

### Columns

| Column | Source type | Snowdrop type | Nullable | Status | Notes |
|---|---|---|---|---|---|
| id | bigint | BIGINT | no | supported |  |
| venue_code | varchar(32) | VARCHAR(32) | no | supported |  |
| name | varchar(128) | VARCHAR(128) | no | supported |  |
| city | varchar(64) | VARCHAR(64) | no | supported |  |
| capacity | integer | INT | no | supported |  |

### Constraints

| Name | Kind | Columns | Status | Reference | Notes |
|---|---|---|---|---|---|
| venues_capacity_check | CHECK |  | partial | - | CHECK constraints are supported only for the current safe row-local scalar subset |
| venues_pkey | PRIMARY KEY | id | supported | - |  |
| venues_venue_code_key | UNIQUE | venue_code | supported | - |  |

### Indexes

| Name | Status | Definition |
|---|---|---|
| venues_pkey | partial | CREATE UNIQUE INDEX venues_pkey ON public.venues USING btree (id) |
| venues_venue_code_key | partial | CREATE UNIQUE INDEX venues_venue_code_key ON public.venues USING btree (venue_code) |

### Translation Preview

```sql
-- Source table public.venues
CREATE TABLE "venues" (
  "id" BIGINT NOT NULL,
  "venue_code" VARCHAR(32) NOT NULL,
  "name" VARCHAR(128) NOT NULL,
  "city" VARCHAR(64) NOT NULL,
  "capacity" INT NOT NULL,
  CHECK (((capacity > 0))),
  PRIMARY KEY ("id"),
  UNIQUE ("venue_code")
);

-- NOTE: index 'venues_pkey' is omitted from the preview and should be recreated manually if still needed
-- NOTE: index 'venues_venue_code_key' is omitted from the preview and should be recreated manually if still needed
```

