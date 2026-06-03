# Snowdrop Migration Preflight

Generated at: `2026-05-25T20:34:43.830275628+00:00`

- Source PostgreSQL database: `ticketing_orders_large`
- Schema filter: `all non-system schemas`

## Summary

| Metric | Value |
|---|---|
| Tables | 13 |
| Columns | 78 |
| Constraints | 37 |
| Indexes | 0 |
| Supported tables | 13 |
| Partial tables | 0 |
| Unsupported tables | 0 |

| DDL-ready tables | 13 |
| DDL-partial tables | 0 |
| DDL-blocked tables | 0 |

## Findings By Status

| Status | Count |
|---|---|

## Table Compatibility

| Schema | Table | Status | Translation | Columns | Constraints | Indexes |
|---|---|---|---|---|---|---|
| public | accounts | supported | ready | 5 | 3 | 0 |
| public | audit_events | supported | ready | 6 | 2 | 0 |
| public | customers | supported | ready | 6 | 2 | 0 |
| public | events | supported | ready | 7 | 3 | 0 |
| public | order_items | supported | ready | 7 | 4 | 0 |
| public | orders | supported | ready | 8 | 3 | 0 |
| public | payments | supported | ready | 6 | 3 | 0 |
| public | promotions | supported | ready | 4 | 2 | 0 |
| public | refunds | supported | ready | 5 | 2 | 0 |
| public | seat_holds | supported | ready | 6 | 4 | 0 |
| public | shows | supported | ready | 6 | 3 | 0 |
| public | tickets | supported | ready | 7 | 4 | 0 |
| public | venues | supported | ready | 5 | 2 | 0 |

## Table `public`.`accounts`

- Overall status: `supported`
- Translation preview status: `ready`
- table shape is directly compatible with the current Snowdrop subset

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
| accounts_customer_id_fkey | FOREIGN KEY | customer_id | supported | public.customers(id) |  |
| accounts_customer_id_key | UNIQUE | customer_id | supported | - |  |
| accounts_pkey | PRIMARY KEY | id | supported | - |  |

### Translation Preview

```sql
-- Source table public.accounts
CREATE TABLE "accounts" (
  "id" BIGINT NOT NULL,
  "customer_id" BIGINT NOT NULL,
  "status" VARCHAR(16) NOT NULL DEFAULT 'active',
  "credit_limit" NUMERIC(12,2) NOT NULL DEFAULT 0,
  "opened_on" DATE NOT NULL,
  FOREIGN KEY ("customer_id") REFERENCES "customers" ("id"),
  UNIQUE ("customer_id"),
  PRIMARY KEY ("id")
);
```

## Table `public`.`audit_events`

- Overall status: `supported`
- Translation preview status: `ready`
- table shape is directly compatible with the current Snowdrop subset

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
```

## Table `public`.`customers`

- Overall status: `supported`
- Translation preview status: `ready`
- table shape is directly compatible with the current Snowdrop subset

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
  PRIMARY KEY ("id")
);
```

## Table `public`.`events`

- Overall status: `supported`
- Translation preview status: `ready`
- table shape is directly compatible with the current Snowdrop subset

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
| events_event_code_key | UNIQUE | event_code | supported | - |  |
| events_pkey | PRIMARY KEY | id | supported | - |  |
| events_venue_id_fkey | FOREIGN KEY | venue_id | supported | public.venues(id) |  |

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
  UNIQUE ("event_code"),
  PRIMARY KEY ("id"),
  FOREIGN KEY ("venue_id") REFERENCES "venues" ("id")
);
```

## Table `public`.`order_items`

- Overall status: `supported`
- Translation preview status: `ready`
- table shape is directly compatible with the current Snowdrop subset

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
| order_items_order_id_fkey | FOREIGN KEY | order_id | supported | public.orders(id) |  |
| order_items_order_id_line_no_key | UNIQUE | order_id, line_no | supported | - | multi-column UNIQUE is supported in the current narrow Phase 4 slice |
| order_items_pkey | PRIMARY KEY | id | supported | - |  |
| order_items_show_id_fkey | FOREIGN KEY | show_id | supported | public.shows(id) |  |

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
  FOREIGN KEY ("order_id") REFERENCES "orders" ("id"),
  UNIQUE ("order_id", "line_no"),
  PRIMARY KEY ("id"),
  FOREIGN KEY ("show_id") REFERENCES "shows" ("id")
);
```

## Table `public`.`orders`

- Overall status: `supported`
- Translation preview status: `ready`
- table shape is directly compatible with the current Snowdrop subset

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
| orders_pkey | PRIMARY KEY | id | supported | - |  |
| orders_promo_id_fkey | FOREIGN KEY | promo_id | supported | public.promotions(id) |  |

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
  PRIMARY KEY ("id"),
  FOREIGN KEY ("promo_id") REFERENCES "promotions" ("id")
);
```

## Table `public`.`payments`

- Overall status: `supported`
- Translation preview status: `ready`
- table shape is directly compatible with the current Snowdrop subset

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
| payments_order_id_fkey | FOREIGN KEY | order_id | supported | public.orders(id) |  |
| payments_pkey | PRIMARY KEY | id | supported | - |  |
| payments_provider_ref_key | UNIQUE | provider_ref | supported | - |  |

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
  FOREIGN KEY ("order_id") REFERENCES "orders" ("id"),
  PRIMARY KEY ("id"),
  UNIQUE ("provider_ref")
);
```

## Table `public`.`promotions`

- Overall status: `supported`
- Translation preview status: `ready`
- table shape is directly compatible with the current Snowdrop subset

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
| promotions_pkey | PRIMARY KEY | id | supported | - |  |
| promotions_promo_code_key | UNIQUE | promo_code | supported | - |  |

### Translation Preview

```sql
-- Source table public.promotions
CREATE TABLE "promotions" (
  "id" BIGINT NOT NULL,
  "promo_code" VARCHAR(32) NOT NULL,
  "discount_percent" INT NOT NULL,
  "active" BOOLEAN NOT NULL DEFAULT true,
  PRIMARY KEY ("id"),
  UNIQUE ("promo_code")
);
```

## Table `public`.`refunds`

- Overall status: `supported`
- Translation preview status: `ready`
- table shape is directly compatible with the current Snowdrop subset

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
| refunds_payment_id_fkey | FOREIGN KEY | payment_id | supported | public.payments(id) |  |
| refunds_pkey | PRIMARY KEY | id | supported | - |  |

### Translation Preview

```sql
-- Source table public.refunds
CREATE TABLE "refunds" (
  "id" BIGINT NOT NULL,
  "payment_id" BIGINT NOT NULL,
  "refund_status" VARCHAR(16) NOT NULL,
  "amount" NUMERIC(12,2) NOT NULL,
  "refunded_at" TIMESTAMP NOT NULL,
  FOREIGN KEY ("payment_id") REFERENCES "payments" ("id"),
  PRIMARY KEY ("id")
);
```

## Table `public`.`seat_holds`

- Overall status: `supported`
- Translation preview status: `ready`
- table shape is directly compatible with the current Snowdrop subset

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
```

## Table `public`.`shows`

- Overall status: `supported`
- Translation preview status: `ready`
- table shape is directly compatible with the current Snowdrop subset

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
| shows_event_id_fkey | FOREIGN KEY | event_id | supported | public.events(id) |  |
| shows_pkey | PRIMARY KEY | id | supported | - |  |
| shows_show_code_key | UNIQUE | show_code | supported | - |  |

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
  FOREIGN KEY ("event_id") REFERENCES "events" ("id"),
  PRIMARY KEY ("id"),
  UNIQUE ("show_code")
);
```

## Table `public`.`tickets`

- Overall status: `supported`
- Translation preview status: `ready`
- table shape is directly compatible with the current Snowdrop subset

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
```

## Table `public`.`venues`

- Overall status: `supported`
- Translation preview status: `ready`
- table shape is directly compatible with the current Snowdrop subset

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
| venues_pkey | PRIMARY KEY | id | supported | - |  |
| venues_venue_code_key | UNIQUE | venue_code | supported | - |  |

### Translation Preview

```sql
-- Source table public.venues
CREATE TABLE "venues" (
  "id" BIGINT NOT NULL,
  "venue_code" VARCHAR(32) NOT NULL,
  "name" VARCHAR(128) NOT NULL,
  "city" VARCHAR(64) NOT NULL,
  "capacity" INT NOT NULL,
  PRIMARY KEY ("id"),
  UNIQUE ("venue_code")
);
```

