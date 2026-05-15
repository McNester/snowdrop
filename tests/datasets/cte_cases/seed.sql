INSERT INTO customers (id, name, region) VALUES
  (1, 'Alpha', 'east'),
  (2, 'Beta', 'west'),
  (3, 'Gamma', 'east');

INSERT INTO orders (id, customer_id, total, status) VALUES
  (1, 1, 50, 'paid'),
  (2, 1, 75, 'paid'),
  (3, 2, 20, 'draft'),
  (4, 3, 40, 'paid');
