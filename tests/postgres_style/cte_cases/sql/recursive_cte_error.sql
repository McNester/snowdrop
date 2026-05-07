WITH RECURSIVE paid_orders(customer_id, total) AS (
  SELECT customer_id, total FROM orders WHERE status = 'paid'
  UNION ALL
  SELECT paid_orders.customer_id, paid_orders.total FROM paid_orders
), extra(customer_id, total) AS (
  SELECT customer_id, total FROM paid_orders
  UNION ALL
  SELECT extra.customer_id, extra.total FROM extra
)
SELECT customer_id FROM extra;
