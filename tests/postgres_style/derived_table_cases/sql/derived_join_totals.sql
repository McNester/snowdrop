SELECT c.name, dt.total_spent
FROM customers c
INNER JOIN (
  SELECT customer_id, SUM(total) AS total_spent
  FROM orders
  WHERE status = 'paid'
  GROUP BY customer_id
) AS dt ON c.id = dt.customer_id
ORDER BY dt.total_spent DESC, c.name;
