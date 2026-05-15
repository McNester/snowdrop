SELECT name, (SELECT COUNT(*) FROM orders) AS total_orders
FROM customers
ORDER BY id;
