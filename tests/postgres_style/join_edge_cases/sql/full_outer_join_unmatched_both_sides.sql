SELECT COALESCE(c.name, 'orphan') AS customer_name, COALESCE(CAST(o.id AS TEXT), 'none') AS order_id
FROM customers c
FULL OUTER JOIN orders o ON c.id = o.customer_id
ORDER BY customer_name, order_id;
