SELECT constraint_catalog, constraint_schema, constraint_name, unique_constraint_catalog, unique_constraint_schema, unique_constraint_name, update_rule, delete_rule
FROM information_schema.referential_constraints
WHERE constraint_schema = 'public'
ORDER BY constraint_name;
