CREATE TABLE events (
  id BIGINT PRIMARY KEY,
  event_date DATE,
  event_at TIMESTAMP,
  reminder_at TIMESTAMP,
  label VARCHAR(32)
);
