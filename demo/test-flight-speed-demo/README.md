# Test Flight PostgreSQL vs Snowdrop Speed Demo

This demo is a local Docker-only adaptation of `G:\Test_Project`.

It keeps the Test Flight product scenario but replaces Prisma/SQLite with raw PostgreSQL-wire SQL so the same API can run against:

- PostgreSQL source database: `test_flight_large`
- Snowdrop migrated database: `test_flight_large_snowdrop`

## Run

From the repository root:

```powershell
docker compose up -d --build orchestrator postgres neo4j
```

Prepare and migrate the fixture using:

```powershell
docker run --rm --platform linux/amd64 --network snowdrop-1_default -e PGPASSWORD=postgres -v E:\snowdrop-1:/app -w /app postgres:16 bash -lc "psql -h snowdrop-1-postgres-1 -p 5432 -U postgres -d postgres -c 'DROP DATABASE IF EXISTS test_flight_large;' && psql -h snowdrop-1-postgres-1 -p 5432 -U postgres -d postgres -c 'CREATE DATABASE test_flight_large;' && psql -h snowdrop-1-postgres-1 -p 5432 -U postgres -d test_flight_large -f tests/large_postgres_bases_for_test/test_flight/schema.sql && psql -h snowdrop-1-postgres-1 -p 5432 -U postgres -d test_flight_large -f tests/large_postgres_bases_for_test/test_flight/seed.sql"
```

```powershell
docker run --rm --platform linux/amd64 --network snowdrop-1_default -e PGPASSWORD=postgres -e SNOWDROP_HOST=snowdrop-1-orchestrator-1 -e SNOWDROP_PORT=5432 -e POSTGRES_HOST=snowdrop-1-postgres-1 -e POSTGRES_PORT=5432 -e HEALTH_HOST=snowdrop-1-orchestrator-1 -e HEALTH_PORT=8080 -e METRICS_HOST=snowdrop-1-orchestrator-1 -e METRICS_PORT=9090 -v E:\snowdrop-1:/app -w /app rust:1.86-bookworm bash -lc "export PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin && cargo run -p snowdrop-testkit -- migration-auto --dump-dir tests/large_postgres_bases_for_test/test_flight --source-database test_flight_large --target-database test_flight_large_snowdrop"
```

Start both sites:

```powershell
docker compose -f docker-compose.test-flight-demo.yml up -d --build
```

Open:

- PostgreSQL: `http://localhost:8110`
- Snowdrop: `http://localhost:8120`

## What To Demo

- Login with `demo@testflight.local` / `demo12345`.
- Search `Kyiv -> Warsaw`.
- Book one flight.
- Refresh and cancel bookings.
- Run dashboards.
- Compare the sticky latency panel on both sites.

The read-heavy dashboards and flight search are the main Snowdrop-friendly story. CRUD is intentionally kept visible to prove the site is alive and to expose write-path bottlenecks honestly.

## Action Benchmark

Run the same login/search/book/list/cancel/dashboard actions against both running sites and write a comparison report:

```powershell
docker run --rm --platform linux/amd64 --network snowdrop-1_default -e POSTGRES_BASE_URL=http://test-flight-postgres-site:8110 -e SNOWDROP_BASE_URL=http://test-flight-snowdrop-site:8120 -e RUNS=7 -v E:\snowdrop-1:/app -w /app/demo/test-flight-speed-demo node:22-bookworm-slim node scripts/compare-actions.mjs
```

Outputs:

- `reports/test-flight-demo/action-comparison.json`
- `reports/test-flight-demo/action-comparison.md`

The report is intentionally honest: read-heavy actions can show Snowdrop wins, while write actions currently show the remaining Snowdrop write-path gap.
