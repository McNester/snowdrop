-- Snowdrop migration translation preview
-- Source PostgreSQL database: ticketing_orders_large

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

