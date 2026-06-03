-- Snowdrop migration translation preview
-- Source PostgreSQL database: ticketing_orders_large

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

-- Source table public.promotions
CREATE TABLE "promotions" (
  "id" BIGINT NOT NULL,
  "promo_code" VARCHAR(32) NOT NULL,
  "discount_percent" INT NOT NULL,
  "active" BOOLEAN NOT NULL DEFAULT true,
  PRIMARY KEY ("id"),
  UNIQUE ("promo_code")
);

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

