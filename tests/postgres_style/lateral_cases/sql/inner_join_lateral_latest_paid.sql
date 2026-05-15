SELECT c.name, recent.total
FROM customers c
INNER JOIN LATERAL (
  SELECT o.total
  FROM orders o
  WHERE o.customer_id = c.id AND o.status = 'paid'
  ORDER BY o.total DESC
  LIMIT 1
) AS recent ON TRUE
ORDER BY c.name;
