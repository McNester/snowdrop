# Ticketing Orders Large PostgreSQL Fixture

This fixture is the Phase 6.5 large migration candidate.

It is intentionally generated from checked-in SQL instead of a binary dump so it can be recreated in Docker or CI with deterministic mock data.

Profile:
- single-schema `public` OLTP + reporting-friendly ticketing/orders workload
- heavy join paths across customers, events, shows, tickets, orders, payments, promotions, refunds, seat holds, and audit events
- separate benchmark catalogs for honest join-gap testing and Snowdrop-favorable scan/aggregate demos
- green automatic-migration subset only: PK, FK, UNIQUE, DEFAULT, NUMERIC, DATE, TIMESTAMP, VARCHAR, BOOLEAN
- realistic skew via hot events, partial refunds, failed payments, and repeat customers
- demo-sized seed volume that is large enough for repeatable latency comparisons but still completes with the current row-by-row migration loader

Approximate seeded scale:
- `customers`: 400
- `accounts`: 400
- `venues`: 30
- `events`: 160
- `shows`: 800
- `orders`: 1,600
- `order_items`: 3,200
- `tickets`: 3,200
- `payments`: 1,600
- `refunds`: ~84
- `seat_holds`: 1,000
- `audit_events`: 5,000

The fixture deliberately favors reproducible demo completion over maximum volume. Increase the `generate_series(...)` bounds in `seed.sql` for stress runs after the migration loader gains bulk insert support.

Recommended usage:
- `cargo run -p snowdrop-testkit -- migration-preflight --dump-dir tests/large_postgres_bases_for_test/ticketing_orders --source-database ticketing_orders_large`
- `cargo run -p snowdrop-testkit -- migration-auto --dump-dir tests/large_postgres_bases_for_test/ticketing_orders --source-database ticketing_orders_large`

Fixture path for migration:
- `tests/large_postgres_bases_for_test/ticketing_orders`

Files:
- `schema.sql`: PostgreSQL schema for the migration candidate
- `seed.sql`: deterministic mock-data seed with easy `generate_series(...)` scaling knobs
- `benchmark_queries.json`: join-heavy gap suite; useful for tracking current planner/JOIN weaknesses honestly
- `benchmark_queries_snowdrop_favorable.json`: scan-heavy fact/aggregate suite; this is the default `migration-auto` and demo-site benchmark catalog when present

Ready commands:

1. Start the required services:
- `docker compose up -d --build orchestrator postgres neo4j`

2. Create a persistent PostgreSQL source database for demo use:
- `docker run --rm --platform linux/amd64 --network snowdrop-1_default -e PGPASSWORD=postgres -v E:\snowdrop-1:/app -w /app postgres:16 bash -lc "psql -h snowdrop-1-postgres-1 -p 5432 -U postgres -d postgres -c 'DROP DATABASE IF EXISTS ticketing_orders_large;' && psql -h snowdrop-1-postgres-1 -p 5432 -U postgres -d postgres -c 'CREATE DATABASE ticketing_orders_large;' && psql -h snowdrop-1-postgres-1 -p 5432 -U postgres -d ticketing_orders_large -f tests/large_postgres_bases_for_test/ticketing_orders/schema.sql && psql -h snowdrop-1-postgres-1 -p 5432 -U postgres -d ticketing_orders_large -f tests/large_postgres_bases_for_test/ticketing_orders/seed.sql"`

3. Run preflight against the live source database:
- `docker run --rm --platform linux/amd64 --network snowdrop-1_default -e PGPASSWORD=postgres -e SNOWDROP_HOST=snowdrop-1-orchestrator-1 -e SNOWDROP_PORT=5432 -e POSTGRES_HOST=snowdrop-1-postgres-1 -e POSTGRES_PORT=5432 -e HEALTH_HOST=snowdrop-1-orchestrator-1 -e HEALTH_PORT=8080 -e METRICS_HOST=snowdrop-1-orchestrator-1 -e METRICS_PORT=9090 -v E:\snowdrop-1:/app -w /app rust:1.86-bookworm bash -lc "export PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin && cargo run -p snowdrop-testkit -- migration-preflight --database ticketing_orders_large"`

4. Run the full automatic migration:
- `docker run --rm --platform linux/amd64 --network snowdrop-1_default -e PGPASSWORD=postgres -e SNOWDROP_HOST=snowdrop-1-orchestrator-1 -e SNOWDROP_PORT=5432 -e POSTGRES_HOST=snowdrop-1-postgres-1 -e POSTGRES_PORT=5432 -e HEALTH_HOST=snowdrop-1-orchestrator-1 -e HEALTH_PORT=8080 -e METRICS_HOST=snowdrop-1-orchestrator-1 -e METRICS_PORT=9090 -v E:\snowdrop-1:/app -w /app rust:1.86-bookworm bash -lc "export PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin && cargo run -p snowdrop-testkit -- migration-auto --dump-dir tests/large_postgres_bases_for_test/ticketing_orders --source-database ticketing_orders_large --target-database ticketing_orders_large_snowdrop"`

5. Inspect the migrated Snowdrop database:
- `docker run --rm --platform linux/amd64 --network snowdrop-1_default -e PGPASSWORD=postgres postgres:16 psql -h snowdrop-1-orchestrator-1 -p 5432 -U postgres -d ticketing_orders_large_snowdrop -c "\dt"`
- `docker run --rm --platform linux/amd64 --network snowdrop-1_default -e PGPASSWORD=postgres postgres:16 psql -h snowdrop-1-orchestrator-1 -p 5432 -U postgres -d ticketing_orders_large_snowdrop -c "SELECT COUNT(*) FROM orders;"`
- `docker run --rm --platform linux/amd64 --network snowdrop-1_default -e PGPASSWORD=postgres postgres:16 psql -h snowdrop-1-orchestrator-1 -p 5432 -U postgres -d ticketing_orders_large_snowdrop -c "SELECT c.tier, COUNT(*) AS order_count, SUM(o.grand_total) AS gross_revenue FROM customers c JOIN accounts a ON a.customer_id = c.id JOIN orders o ON o.account_id = a.id WHERE o.order_status = 'placed' GROUP BY c.tier ORDER BY gross_revenue DESC, c.tier;"`

6. Start the local PostgreSQL vs Snowdrop demo site:
- `docker compose -f docker-compose.demo.yml up -d --build migration-speed-demo`
- open `http://localhost:8095`

7. Inspect the generated reports:
- `reports/migration/ticketing-orders-large-preflight.md`
- `reports/migration/ticketing-orders-large-ticketing-orders-large-snowdrop-workflow.md`
- `reports/migration/ticketing-orders-large-ticketing-orders-large-snowdrop-auto.md`

What is now included in the workflow report:
- row-count verification
- ordered sample-row verification
- numeric aggregate replay
- post-migration benchmark replay from `benchmark_queries_snowdrop_favorable.json` when present, otherwise `benchmark_queries.json`
- average PostgreSQL latency vs average Snowdrop latency
- result-set match flag for each benchmark query
- per-table load timing: PostgreSQL export, SQL literal build, Snowdrop insert, total load time, chunk count, and whether trusted bulk-load mode was used
