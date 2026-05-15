SELECT id FROM events WHERE event_date <= CAST('2024-01-02' AS DATE) ORDER BY event_date, id;
