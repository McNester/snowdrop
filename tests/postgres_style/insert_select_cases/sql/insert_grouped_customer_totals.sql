INSERT INTO customer_totals (customer, total)
SELECT customer, SUM(total) AS total
FROM live_orders
GROUP BY customer
ORDER BY customer;

SELECT customer, total
FROM customer_totals
ORDER BY customer;
