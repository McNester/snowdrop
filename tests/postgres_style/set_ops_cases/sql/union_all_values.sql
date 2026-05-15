SELECT value
FROM left_values
UNION ALL
SELECT value
FROM right_values
ORDER BY value;
