ALTER TABLE users ADD COLUMN status VARCHAR(16) DEFAULT 'active';
INSERT INTO users (id, email, nickname) VALUES (3, 'cara@example.com', 'cara');
SELECT id, email, status FROM users WHERE id = 3;
