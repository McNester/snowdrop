INSERT INTO categories (id, parent_id, name) VALUES
  (1, NULL, 'Root'),
  (2, 1, 'Alpha'),
  (3, 1, 'Beta'),
  (4, 2, 'Alpha-1'),
  (5, 2, 'Alpha-2'),
  (6, 3, 'Beta-1');
