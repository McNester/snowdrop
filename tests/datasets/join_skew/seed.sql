INSERT INTO customers (id, name, tier) VALUES
  (1, 'Acme', 'gold'),
  (2, 'Beta', 'silver'),
  (3, 'Cyan', 'bronze');

INSERT INTO orders (id, customer_id, total) VALUES
  (1, 1, 100.00),
  (2, 1, 125.00),
  (3, 1, 130.00),
  (4, 1, 140.00),
  (5, 2, 50.00),
  (6, 3, 25.00);
