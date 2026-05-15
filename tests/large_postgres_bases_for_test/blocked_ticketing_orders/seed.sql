INSERT INTO blocked_orders (payload, tags)
VALUES
  ('{"kind":"order","state":"new"}', ARRAY['new', 'manual']),
  ('{"kind":"order","state":"review"}', ARRAY['review']);
