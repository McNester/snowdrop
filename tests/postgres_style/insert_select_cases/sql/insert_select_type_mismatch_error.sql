INSERT INTO customer_totals (customer, total)
SELECT customer, status
FROM live_orders;
