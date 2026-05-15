ALTER TABLE users RENAME COLUMN nickname TO handle;
SELECT id, handle FROM users WHERE id = 1;
