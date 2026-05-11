CREATE TABLE customers (
  id BIGINT PRIMARY KEY,
  email VARCHAR(255) NOT NULL UNIQUE,
  full_name VARCHAR(128) NOT NULL,
  tier VARCHAR(16) NOT NULL DEFAULT 'standard',
  marketing_opt_in BOOLEAN NOT NULL DEFAULT FALSE,
  created_on DATE NOT NULL,
  CHECK (tier = 'standard' OR tier = 'vip' OR tier = 'enterprise')
);

CREATE TABLE accounts (
  id BIGINT PRIMARY KEY,
  customer_id BIGINT NOT NULL UNIQUE,
  status VARCHAR(16) NOT NULL DEFAULT 'active',
  credit_limit NUMERIC(12,2) NOT NULL DEFAULT 0,
  opened_on DATE NOT NULL,
  CHECK (credit_limit >= 0),
  FOREIGN KEY (customer_id) REFERENCES customers(id)
);

CREATE TABLE venues (
  id BIGINT PRIMARY KEY,
  venue_code VARCHAR(32) NOT NULL UNIQUE,
  name VARCHAR(128) NOT NULL,
  city VARCHAR(64) NOT NULL,
  capacity INT NOT NULL,
  CHECK (capacity > 0)
);

CREATE TABLE events (
  id BIGINT PRIMARY KEY,
  venue_id BIGINT NOT NULL,
  event_code VARCHAR(32) NOT NULL UNIQUE,
  title VARCHAR(160) NOT NULL,
  category VARCHAR(32) NOT NULL,
  starts_on DATE NOT NULL,
  base_price NUMERIC(10,2) NOT NULL,
  CHECK (base_price >= 0),
  FOREIGN KEY (venue_id) REFERENCES venues(id)
);

CREATE TABLE shows (
  id BIGINT PRIMARY KEY,
  event_id BIGINT NOT NULL,
  show_code VARCHAR(32) NOT NULL UNIQUE,
  show_day DATE NOT NULL,
  seat_capacity INT NOT NULL,
  sold_count INT NOT NULL DEFAULT 0,
  CHECK (seat_capacity >= sold_count),
  FOREIGN KEY (event_id) REFERENCES events(id)
);

CREATE TABLE promotions (
  id BIGINT PRIMARY KEY,
  promo_code VARCHAR(32) NOT NULL UNIQUE,
  discount_percent INT NOT NULL,
  active BOOLEAN NOT NULL DEFAULT TRUE,
  CHECK (discount_percent BETWEEN 0 AND 80)
);

CREATE TABLE orders (
  id BIGINT PRIMARY KEY,
  account_id BIGINT NOT NULL,
  promo_id BIGINT,
  order_status VARCHAR(16) NOT NULL DEFAULT 'placed',
  ordered_at TIMESTAMP NOT NULL,
  subtotal NUMERIC(12,2) NOT NULL,
  discount_total NUMERIC(12,2) NOT NULL DEFAULT 0,
  grand_total NUMERIC(12,2) NOT NULL,
  CHECK (subtotal >= 0),
  CHECK (discount_total >= 0),
  CHECK (grand_total >= 0),
  FOREIGN KEY (account_id) REFERENCES accounts(id),
  FOREIGN KEY (promo_id) REFERENCES promotions(id)
);

CREATE TABLE order_items (
  id BIGINT PRIMARY KEY,
  order_id BIGINT NOT NULL,
  show_id BIGINT NOT NULL,
  line_no INT NOT NULL,
  quantity INT NOT NULL,
  unit_price NUMERIC(10,2) NOT NULL,
  line_total NUMERIC(12,2) NOT NULL,
  UNIQUE (order_id, line_no),
  CHECK (quantity > 0),
  CHECK (unit_price >= 0),
  CHECK (line_total >= 0),
  FOREIGN KEY (order_id) REFERENCES orders(id),
  FOREIGN KEY (show_id) REFERENCES shows(id)
);

CREATE TABLE tickets (
  id BIGINT PRIMARY KEY,
  order_item_id BIGINT NOT NULL,
  show_id BIGINT NOT NULL,
  ticket_code VARCHAR(48) NOT NULL UNIQUE,
  seat_label VARCHAR(16) NOT NULL,
  ticket_status VARCHAR(16) NOT NULL DEFAULT 'issued',
  issued_at TIMESTAMP NOT NULL,
  FOREIGN KEY (order_item_id) REFERENCES order_items(id),
  FOREIGN KEY (show_id) REFERENCES shows(id)
);

CREATE TABLE payments (
  id BIGINT PRIMARY KEY,
  order_id BIGINT NOT NULL,
  payment_status VARCHAR(16) NOT NULL,
  amount NUMERIC(12,2) NOT NULL,
  paid_at TIMESTAMP NOT NULL,
  provider_ref VARCHAR(64) NOT NULL UNIQUE,
  CHECK (amount >= 0),
  FOREIGN KEY (order_id) REFERENCES orders(id)
);

CREATE TABLE refunds (
  id BIGINT PRIMARY KEY,
  payment_id BIGINT NOT NULL,
  refund_status VARCHAR(16) NOT NULL,
  amount NUMERIC(12,2) NOT NULL,
  refunded_at TIMESTAMP NOT NULL,
  CHECK (amount >= 0),
  FOREIGN KEY (payment_id) REFERENCES payments(id)
);

CREATE TABLE seat_holds (
  id BIGINT PRIMARY KEY,
  show_id BIGINT NOT NULL,
  customer_id BIGINT NOT NULL,
  seat_label VARCHAR(16) NOT NULL,
  hold_status VARCHAR(16) NOT NULL DEFAULT 'held',
  expires_at TIMESTAMP NOT NULL,
  UNIQUE (show_id, seat_label),
  FOREIGN KEY (show_id) REFERENCES shows(id),
  FOREIGN KEY (customer_id) REFERENCES customers(id)
);

CREATE TABLE audit_events (
  id BIGINT PRIMARY KEY,
  account_id BIGINT NOT NULL,
  event_kind VARCHAR(32) NOT NULL,
  created_at TIMESTAMP NOT NULL,
  event_day DATE NOT NULL,
  payload_hint VARCHAR(160),
  FOREIGN KEY (account_id) REFERENCES accounts(id)
);

CREATE INDEX idx_events_starts_on ON events(starts_on);
CREATE INDEX idx_shows_event_day ON shows(event_id, show_day);
CREATE INDEX idx_orders_account_ordered_at ON orders(account_id, ordered_at);
CREATE INDEX idx_order_items_show_id ON order_items(show_id);
CREATE INDEX idx_payments_order_id ON payments(order_id);
CREATE INDEX idx_audit_events_account_day ON audit_events(account_id, event_day);
