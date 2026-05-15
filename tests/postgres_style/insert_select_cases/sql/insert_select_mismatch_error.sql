INSERT INTO customer_totals (customer, total)
SELECT id, customer, total
FROM live_orders;
