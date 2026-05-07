WITH RECURSIVE tree(id, name, depth) AS (
  SELECT id, name, 0 FROM categories WHERE id = 2
  UNION ALL
  SELECT c.id, c.name, tree.depth + 1
  FROM tree
  INNER JOIN categories c ON c.parent_id = tree.id
)
SELECT name, depth FROM tree ORDER BY depth, name;
