SELECT table_catalog, table_schema, table_name, column_name, ordinal_position, constraint_name
FROM information_schema.key_column_usage
WHERE table_schema = 'public'
  AND table_name IN ('accounts', 'parent_accounts')
ORDER BY table_name, constraint_name, ordinal_position;
