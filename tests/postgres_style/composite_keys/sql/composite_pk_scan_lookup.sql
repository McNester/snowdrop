INSERT INTO memberships (tenant_id, user_id, role) VALUES (1, 12, 'editor');
SELECT tenant_id, user_id, role FROM memberships WHERE tenant_id = 1 ORDER BY user_id;
