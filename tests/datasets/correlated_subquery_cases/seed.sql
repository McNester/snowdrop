INSERT INTO customers (id, name) VALUES
  (1, 'Alpha'),
  (2, 'Beta'),
  (3, 'Gamma');

INSERT INTO orders (id, customer_id, total, status) VALUES
  (1, 1, 10, 'draft'),
  (2, 1, 25, 'paid'),
  (3, 2, 40, 'paid');
