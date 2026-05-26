# Snowdrop Migration Speed Demo

This local Docker demo compares the same ticketing/orders SQL workload against PostgreSQL and Snowdrop.

It expects:
- PostgreSQL source database: `ticketing_orders_large`
- Snowdrop migrated database: `ticketing_orders_large_snowdrop`
- query catalog: `tests/large_postgres_bases_for_test/ticketing_orders/benchmark_queries.json`

## Run

Prepare services and data using the fixture runbook:

```powershell
docker compose up -d --build orchestrator postgres neo4j
```

Then follow the commands in:

- `tests/large_postgres_bases_for_test/ticketing_orders/README.md`

Start the demo site:

```powershell
docker compose -f docker-compose.demo.yml up -d --build migration-speed-demo
```

Open:

- `http://localhost:8095`

## What It Shows

- live PostgreSQL latency
- live Snowdrop latency
- speedup ratio
- row count
- result-set match status
- first 50 result rows

The demo is intentionally local-first and Docker-first so it does not require local `cargo`, local `node`, or local `psql`.

The ticketing/orders catalog is join-heavy, so it is a correctness and migration demonstration first. PostgreSQL may be faster on several of these queries until Snowdrop's join path is optimized; use the TPC-H benchmark runbook for scan-heavy performance comparisons.
