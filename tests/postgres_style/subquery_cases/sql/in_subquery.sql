SELECT name
FROM customers
WHERE id IN (SELECT customer_id FROM orders WHERE total >= 25)
ORDER BY name;
