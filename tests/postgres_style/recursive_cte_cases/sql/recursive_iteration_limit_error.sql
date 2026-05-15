WITH RECURSIVE loop_ids(id) AS (
  SELECT id FROM categories WHERE id = 1
  UNION ALL
  SELECT loop_ids.id FROM loop_ids
)
SELECT id FROM loop_ids;
