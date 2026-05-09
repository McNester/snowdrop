SELECT con.conname, con.contype
FROM pg_catalog.pg_constraint con
LEFT JOIN pg_catalog.pg_class c ON c.oid = con.conrelid
LEFT JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = 'public' AND con.conrelid <> 0
ORDER BY con.conname;
