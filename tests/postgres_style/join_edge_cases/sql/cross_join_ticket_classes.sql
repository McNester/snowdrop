SELECT c.name, tc.label
FROM customers c
CROSS JOIN ticket_classes tc
ORDER BY c.name, tc.label;
