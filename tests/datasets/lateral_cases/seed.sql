INSERT INTO customers (id, name, tier) VALUES
  (1, 'Alpha', 'gold'),
  (2, 'Beta', 'silver'),
  (3, 'Gamma', 'gold');

INSERT INTO orders (id, customer_id, total, status) VALUES
  (1, 1, 50, 'paid'),
  (2, 1, 75, 'paid'),
  (3, 2, 20, 'draft'),
  (4, 3, 40, 'paid');
