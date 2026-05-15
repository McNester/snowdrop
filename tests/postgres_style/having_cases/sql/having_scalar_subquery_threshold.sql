SELECT d.name, SUM(w.hours) AS total_hours
FROM departments d
INNER JOIN worklog w ON d.id = w.dept_id
GROUP BY d.id, d.name
HAVING SUM(w.hours) > (SELECT AVG(hours) FROM worklog)
ORDER BY total_hours DESC, d.name;
