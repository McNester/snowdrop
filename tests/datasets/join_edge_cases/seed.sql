INSERT INTO customers (id, name) VALUES
  (1, 'Alice'),
  (2, 'Bob'),
  (3, 'Cara');

INSERT INTO orders (id, customer_id, amount) VALUES
  (10, 1, 10),
  (11, 1, 20),
  (12, 2, 5),
  (13, 99, 99);

INSERT INTO ticket_classes (id, label) VALUES
  (1, 'std'),
  (2, 'vip');
