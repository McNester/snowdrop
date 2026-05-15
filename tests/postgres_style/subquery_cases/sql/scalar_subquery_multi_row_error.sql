SELECT name, (SELECT customer_id FROM orders) AS any_customer
FROM customers
ORDER BY id;
