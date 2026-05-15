SELECT c.name
FROM customers c
INNER JOIN LATERAL (VALUES (1)) AS recent(total) ON TRUE;
