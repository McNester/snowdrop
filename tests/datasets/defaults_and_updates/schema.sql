CREATE TABLE tasks (
  id BIGINT PRIMARY KEY,
  state VARCHAR(16) DEFAULT 'new',
  priority INT DEFAULT 1,
  note VARCHAR(255)
);
