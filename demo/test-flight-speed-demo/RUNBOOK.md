# Test Flight Demo Runbook

This runbook starts two local copies of the same Test Flight site:

- PostgreSQL: `http://localhost:8110`
- Snowdrop: `http://localhost:8120`

## 1. Start Snowdrop and PostgreSQL

```powershell
docker compose up -d --build orchestrator postgres neo4j
```

## 2. Reset PostgreSQL Source Database

```powershell
docker run --rm --platform linux/amd64 --network snowdrop-1_default -e PGPASSWORD=postgres -v E:\snowdrop-1:/app -w /app postgres:16 bash -lc "psql -h snowdrop-1-postgres-1 -p 5432 -U postgres -d postgres -c 'DROP DATABASE IF EXISTS test_flight_large;' && psql -h snowdrop-1-postgres-1 -p 5432 -U postgres -d postgres -c 'CREATE DATABASE test_flight_large;' && psql -h snowdrop-1-postgres-1 -p 5432 -U postgres -d test_flight_large -f tests/large_postgres_bases_for_test/test_flight/schema.sql && psql -h snowdrop-1-postgres-1 -p 5432 -U postgres -d test_flight_large -f tests/large_postgres_bases_for_test/test_flight/seed.sql"
```

## 3. Migrate PostgreSQL To Snowdrop

```powershell
docker run --rm --platform linux/amd64 --network snowdrop-1_default -e PGPASSWORD=postgres -e SNOWDROP_HOST=snowdrop-1-orchestrator-1 -e SNOWDROP_PORT=5432 -e POSTGRES_HOST=snowdrop-1-postgres-1 -e POSTGRES_PORT=5432 -e HEALTH_HOST=snowdrop-1-orchestrator-1 -e HEALTH_PORT=8080 -e METRICS_HOST=snowdrop-1-orchestrator-1 -e METRICS_PORT=9090 -v E:\snowdrop-1:/app -w /app rust:1.86-bookworm bash -lc "export PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin && cargo run -p snowdrop-testkit -- migration-auto --dump-dir tests/large_postgres_bases_for_test/test_flight --source-database test_flight_large --target-database test_flight_large_snowdrop"
```

## 4. Start Both Sites

```powershell
docker compose -f docker-compose.test-flight-demo.yml up -d --build
```

Open:

- `http://localhost:8110`
- `http://localhost:8120`

## 5. Demo Script

1. Open both sites side by side.
2. Login with `demo@testflight.local` / `demo12345`.
3. Search `Kyiv -> Warsaw`.
4. Compare the sticky latency panel.
5. Book a flight on both sites.
6. Open "My bookings" and cancel the new booking.
7. Run dashboards and compare read-heavy query latency.

The dashboard/search path is the main Snowdrop-friendly story. Booking writes are intentionally shown too, because they expose the current write-path performance honestly.

## 6. Repeatable Action Benchmark

After both sites are running, use this Docker-only command to compare the same actions the UI buttons perform:

```powershell
docker run --rm --platform linux/amd64 --network snowdrop-1_default -e POSTGRES_BASE_URL=http://test-flight-postgres-site:8110 -e SNOWDROP_BASE_URL=http://test-flight-snowdrop-site:8120 -e RUNS=7 -v E:\snowdrop-1:/app -w /app/demo/test-flight-speed-demo node:22-bookworm-slim node scripts/compare-actions.mjs
```

Read the latest report:

- `reports/test-flight-demo/action-comparison.md`

Interpretation:

- `search_flights_button` and `dashboard_flight_status_button` are the main Snowdrop-friendly read scenarios.
- `book_flight_button` and `cancel_booking_button` are write-path checks and are expected to be slower until the Snowdrop DML/WAL/storage path is optimized.
