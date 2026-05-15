SELECT c.name, (SELECT COUNT(*) FROM orders o WHERE o.customer_id = c.id) AS order_count
FROM customers c
ORDER BY c.id;
