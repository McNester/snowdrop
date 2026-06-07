# Test Flight Large PostgreSQL Fixture

This fixture is the canonical local demo database for the Test Flight PostgreSQL vs Snowdrop sites.

It is intentionally Snowdrop-safe:

- no Prisma migrations
- no SQLite syntax
- no JSONB, views, triggers, functions, enum types, partial indexes, or CHECK constraints
- PK, FK, UNIQUE, DEFAULT, VARCHAR, BOOLEAN, NUMERIC, DATE, TIMESTAMP, INT, BIGINT only

Approximate seeded scale:

- `users`: 5,000
- `airports`: 20
- `airlines`: 10
- `aircraft`: 50
- `flights`: 20,000
- `bookings`: 2,000
- `flight_search_events`: 50,000
- `route_daily_stats`: 3,600

Demo user:

- email: `demo@testflight.local`
- password: `demo12345`

## Commands

Start infrastructure:

```powershell
docker compose up -d --build orchestrator postgres neo4j
```

Create PostgreSQL source DB:

```powershell
docker run --rm --platform linux/amd64 --network snowdrop-1_default -e PGPASSWORD=postgres -v E:\snowdrop-1:/app -w /app postgres:16 bash -lc "psql -h snowdrop-1-postgres-1 -p 5432 -U postgres -d postgres -c 'DROP DATABASE IF EXISTS test_flight_large;' && psql -h snowdrop-1-postgres-1 -p 5432 -U postgres -d postgres -c 'CREATE DATABASE test_flight_large;' && psql -h snowdrop-1-postgres-1 -p 5432 -U postgres -d test_flight_large -f tests/large_postgres_bases_for_test/test_flight/schema.sql && psql -h snowdrop-1-postgres-1 -p 5432 -U postgres -d test_flight_large -f tests/large_postgres_bases_for_test/test_flight/seed.sql"
```

Run automatic migration:

```powershell
docker run --rm --platform linux/amd64 --network snowdrop-1_default -e PGPASSWORD=postgres -e SNOWDROP_HOST=snowdrop-1-orchestrator-1 -e SNOWDROP_PORT=5432 -e POSTGRES_HOST=snowdrop-1-postgres-1 -e POSTGRES_PORT=5432 -e HEALTH_HOST=snowdrop-1-orchestrator-1 -e HEALTH_PORT=8080 -e METRICS_HOST=snowdrop-1-orchestrator-1 -e METRICS_PORT=9090 -v E:\snowdrop-1:/app -w /app rust:1.86-bookworm bash -lc "export PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin && cargo run -p snowdrop-testkit -- migration-auto --dump-dir tests/large_postgres_bases_for_test/test_flight --source-database test_flight_large --target-database test_flight_large_snowdrop"
```

Start both Test Flight sites:

```powershell
docker compose -f docker-compose.test-flight-demo.yml up -d --build
```

Open:

- PostgreSQL site: `http://localhost:8110`
- Snowdrop site: `http://localhost:8120`

Inspect Snowdrop migrated DB:

```powershell
docker run --rm --platform linux/amd64 --network snowdrop-1_default -e PGPASSWORD=postgres postgres:16 psql -h snowdrop-1-orchestrator-1 -p 5432 -U postgres -d test_flight_large_snowdrop -c "\dt"
```

Run one benchmark query manually:

```powershell
docker run --rm --platform linux/amd64 --network snowdrop-1_default -e PGPASSWORD=postgres postgres:16 psql -h snowdrop-1-orchestrator-1 -p 5432 -U postgres -d test_flight_large_snowdrop -c "SELECT event_day, event_kind, COUNT(*) AS activity_count FROM flight_search_events GROUP BY event_day, event_kind ORDER BY event_day DESC, event_kind LIMIT 40;"
```
