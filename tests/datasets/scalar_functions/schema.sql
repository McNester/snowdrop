CREATE TABLE profiles (
  id BIGINT PRIMARY KEY,
  display_name VARCHAR(255),
  fallback_name VARCHAR(255),
  score INT,
  score_text VARCHAR(32),
  amount_text VARCHAR(32),
  active_text VARCHAR(16),
  due_date DATE,
  event_at TIMESTAMP
);
