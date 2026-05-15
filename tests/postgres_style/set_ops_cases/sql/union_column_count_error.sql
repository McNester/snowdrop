SELECT id, value
FROM left_values
UNION
SELECT value
FROM right_values;
