SELECT id, SUM(score) OVER (PARTITION BY grp ORDER BY id) AS running_sum
FROM scores;
