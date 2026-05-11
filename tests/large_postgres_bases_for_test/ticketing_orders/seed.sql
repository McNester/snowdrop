INSERT INTO customers (id, email, full_name, tier, marketing_opt_in, created_on)
SELECT
  gs,
  format('customer%05s@example.com', gs),
  format('Customer %s', gs),
  CASE
    WHEN gs % 25 = 0 THEN 'enterprise'
    WHEN gs % 7 = 0 THEN 'vip'
    ELSE 'standard'
  END,
  (gs % 3 = 0),
  DATE '2024-01-01' + ((gs % 365) * INTERVAL '1 day')
FROM generate_series(1, 4000) AS gs;

INSERT INTO accounts (id, customer_id, status, credit_limit, opened_on)
SELECT
  gs,
  gs,
  CASE WHEN gs % 29 = 0 THEN 'review' ELSE 'active' END,
  ((gs % 40) * 250)::numeric(12,2),
  DATE '2024-01-01' + ((gs % 365) * INTERVAL '1 day')
FROM generate_series(1, 4000) AS gs;

INSERT INTO venues (id, venue_code, name, city, capacity)
SELECT
  gs,
  format('VEN%03s', gs),
  format('Venue %s', gs),
  CASE (gs % 5)
    WHEN 0 THEN 'London'
    WHEN 1 THEN 'Berlin'
    WHEN 2 THEN 'Prague'
    WHEN 3 THEN 'Warsaw'
    ELSE 'Madrid'
  END,
  1500 + (gs % 20) * 250
FROM generate_series(1, 100) AS gs;

INSERT INTO events (id, venue_id, event_code, title, category, starts_on, base_price)
SELECT
  gs,
  ((gs - 1) % 100) + 1,
  format('EVT%05s', gs),
  format('Event %s', gs),
  CASE (gs % 6)
    WHEN 0 THEN 'concert'
    WHEN 1 THEN 'sports'
    WHEN 2 THEN 'festival'
    WHEN 3 THEN 'comedy'
    WHEN 4 THEN 'family'
    ELSE 'conference'
  END,
  DATE '2025-01-01' + ((gs % 180) * INTERVAL '1 day'),
  (35 + (gs % 40) * 3)::numeric(10,2)
FROM generate_series(1, 1600) AS gs;

INSERT INTO shows (id, event_id, show_code, show_day, seat_capacity, sold_count)
SELECT
  gs,
  ((gs - 1) % 1600) + 1,
  format('SHW%06s', gs),
  DATE '2025-01-01' + ((gs % 180) * INTERVAL '1 day'),
  800 + (gs % 12) * 100,
  (gs % 300)
FROM generate_series(1, 8000) AS gs;

INSERT INTO promotions (id, promo_code, discount_percent, active)
SELECT
  gs,
  format('PROMO%03s', gs),
  CASE
    WHEN gs % 9 = 0 THEN 25
    WHEN gs % 5 = 0 THEN 15
    ELSE 10
  END,
  gs % 4 <> 0
FROM generate_series(1, 64) AS gs;

INSERT INTO orders (id, account_id, promo_id, order_status, ordered_at, subtotal, discount_total, grand_total)
SELECT
  gs,
  ((gs - 1) % 4000) + 1,
  CASE WHEN gs % 8 = 0 THEN ((gs - 1) % 64) + 1 ELSE NULL END,
  CASE
    WHEN gs % 19 = 0 THEN 'refunded'
    WHEN gs % 11 = 0 THEN 'failed'
    ELSE 'placed'
  END,
  TIMESTAMP '2025-01-01 09:00:00' + ((gs % 120) || ' days')::interval + ((gs % 480) || ' minutes')::interval,
  (50 + (gs % 7) * 20)::numeric(12,2),
  CASE WHEN gs % 8 = 0 THEN (5 + (gs % 4) * 3)::numeric(12,2) ELSE 0::numeric(12,2) END,
  (50 + (gs % 7) * 20 - CASE WHEN gs % 8 = 0 THEN (5 + (gs % 4) * 3) ELSE 0 END)::numeric(12,2)
FROM generate_series(1, 18000) AS gs;

INSERT INTO order_items (id, order_id, show_id, line_no, quantity, unit_price, line_total)
SELECT
  gs,
  ((gs - 1) / 2) + 1,
  ((gs - 1) % 8000) + 1,
  CASE WHEN gs % 2 = 0 THEN 2 ELSE 1 END,
  CASE WHEN gs % 5 = 0 THEN 2 ELSE 1 END,
  (40 + (gs % 15) * 4)::numeric(10,2),
  ((CASE WHEN gs % 5 = 0 THEN 2 ELSE 1 END) * (40 + (gs % 15) * 4))::numeric(12,2)
FROM generate_series(1, 36000) AS gs;

INSERT INTO tickets (id, order_item_id, show_id, ticket_code, seat_label, ticket_status, issued_at)
SELECT
  gs,
  gs,
  ((gs - 1) % 8000) + 1,
  format('TKT%08s', gs),
  format('S-%s', ((gs - 1) % 900) + 1),
  CASE WHEN gs % 17 = 0 THEN 'void' ELSE 'issued' END,
  TIMESTAMP '2025-01-01 10:00:00' + ((gs % 120) || ' days')::interval
FROM generate_series(1, 36000) AS gs;

INSERT INTO payments (id, order_id, payment_status, amount, paid_at, provider_ref)
SELECT
  gs,
  gs,
  CASE
    WHEN gs % 19 = 0 THEN 'refunded'
    WHEN gs % 11 = 0 THEN 'failed'
    ELSE 'captured'
  END,
  (50 + (gs % 7) * 20 - CASE WHEN gs % 8 = 0 THEN (5 + (gs % 4) * 3) ELSE 0 END)::numeric(12,2),
  TIMESTAMP '2025-01-01 09:15:00' + ((gs % 120) || ' days')::interval + ((gs % 480) || ' minutes')::interval,
  format('PAY-%08s', gs)
FROM generate_series(1, 18000) AS gs;

INSERT INTO refunds (id, payment_id, refund_status, amount, refunded_at)
SELECT
  gs,
  gs,
  'processed',
  ((10 + (gs % 6) * 5))::numeric(12,2),
  TIMESTAMP '2025-02-01 12:00:00' + ((gs % 90) || ' days')::interval
FROM generate_series(19, 18000, 19) AS gs;

INSERT INTO seat_holds (id, show_id, customer_id, seat_label, hold_status, expires_at)
SELECT
  gs,
  ((gs - 1) % 8000) + 1,
  ((gs - 1) % 4000) + 1,
  format('H-%s', ((gs - 1) % 700) + 1),
  CASE WHEN gs % 6 = 0 THEN 'expired' ELSE 'held' END,
  TIMESTAMP '2025-01-10 15:00:00' + ((gs % 45) || ' days')::interval
FROM generate_series(1, 10000) AS gs;

INSERT INTO audit_events (id, account_id, event_kind, created_at, event_day, payload_hint)
SELECT
  gs,
  ((gs - 1) % 4000) + 1,
  CASE (gs % 5)
    WHEN 0 THEN 'login'
    WHEN 1 THEN 'search'
    WHEN 2 THEN 'cart'
    WHEN 3 THEN 'purchase'
    ELSE 'refund'
  END,
  TIMESTAMP '2025-01-01 00:00:00' + ((gs % 120) || ' days')::interval + ((gs % 1440) || ' minutes')::interval,
  DATE '2025-01-01' + ((gs % 120) * INTERVAL '1 day'),
  CASE WHEN gs % 13 = 0 THEN 'hot-event' ELSE NULL END
FROM generate_series(1, 80000) AS gs;
