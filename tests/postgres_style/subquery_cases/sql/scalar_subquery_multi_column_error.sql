SELECT name, (SELECT id, customer_id FROM orders LIMIT 1)
FROM customers
ORDER BY id;
