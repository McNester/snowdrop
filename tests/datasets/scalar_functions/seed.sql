INSERT INTO profiles (id, display_name, fallback_name, score, score_text, amount_text, active_text, due_date, event_at) VALUES
  (1, '  Alice  ', NULL, 10, '10', '10.5', 'true', '2020-01-02', '2000-01-01 00:00:00'),
  (2, NULL, 'backup', NULL, NULL, '2.25', 'false', '2999-01-01', '2999-01-01 00:00:00'),
  (3, 'Bob', '', -3, '-3', NULL, 't', '2000-12-31', '2020-06-01 12:00:00'),
  (4, '', 'spare', 0, '0', '0.0', 'f', '2024-05-05', NULL);
