SELECT c.name, COUNT(o.id) AS order_rows
FROM customers c
LEFT JOIN orders o ON c.id = o.customer_id
GROUP BY c.id, c.name
HAVING (SELECT COUNT(*) FROM orders o2 WHERE o2.customer_id = c.id) >= 1
ORDER BY c.id;
