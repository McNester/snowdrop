SELECT d.name, SUM(w.hours) AS total_hours, COUNT(*) AS entry_count
FROM departments d
INNER JOIN worklog w ON d.id = w.dept_id
GROUP BY d.name
HAVING COUNT(*) >= 2
ORDER BY total_hours - entry_count DESC, d.name;
