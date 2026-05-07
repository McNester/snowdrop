SELECT grp, SUM(score) AS total, ROW_NUMBER() OVER (ORDER BY SUM(score) DESC) AS rn
FROM scores
GROUP BY grp
ORDER BY grp;
