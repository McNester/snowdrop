WITH paid_orders AS (
  SELECT customer_id, total
  FROM orders
  WHERE status = 'paid'
)
SELECT customer_id, SUM(total) AS total_spent
FROM paid_orders
GROUP BY customer_id
ORDER BY total_spent DESC, customer_id;
