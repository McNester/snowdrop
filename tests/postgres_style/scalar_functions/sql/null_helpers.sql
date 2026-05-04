SELECT id, COALESCE(display_name, fallback_name, 'missing'), NULLIF(fallback_name, '') FROM profiles ORDER BY id;
