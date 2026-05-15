SELECT id, COUNT(*) FILTER (WHERE score > 5) OVER (PARTITION BY grp) AS filtered_count
FROM scores;
