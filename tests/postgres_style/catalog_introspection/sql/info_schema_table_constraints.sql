SELECT table_catalog, table_schema, table_name, constraint_name, constraint_type
FROM information_schema.table_constraints
WHERE table_schema = 'public'
  AND table_name IN ('accounts', 'parent_accounts')
  AND constraint_name NOT LIKE '%_not_null'
ORDER BY table_name, constraint_name;
