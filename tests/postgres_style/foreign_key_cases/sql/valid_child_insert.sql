INSERT INTO children (id, parent_id, label) VALUES (11, 2, 'linked');
SELECT id, parent_id, label FROM children WHERE id = 11;
