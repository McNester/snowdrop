ALTER TABLE tasks ALTER COLUMN state SET DEFAULT 'queued';
INSERT INTO tasks (id, note) VALUES (4, 'later');
SELECT id, state FROM tasks WHERE id = 4;
