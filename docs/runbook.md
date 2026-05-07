# Snowdrop Runbook

## Source of Truth

- Current source of truth is the code in this repository plus [performance-architecture.md](/e:/snowdrop-1/docs/performance-architecture.md:1).
- `Snowdrop architecture.pdf` should be treated as historical context. It still describes Neo4j-backed `IndexEntry` storage, while the current code stores secondary indexes in RocksDB and uses Neo4j for schema metadata.

## Configurable Ports

The orchestrator no longer hardcodes listener ports. These env vars control runtime ports:

- `PGWIRE_PORT`
- `HEALTH_PORT`
- `METRICS_PORT`

Docker host port publishing is controlled separately:

- `SNOWDROP_HOST_PG_PORT`
- `SNOWDROP_HOST_HEALTH_PORT`
- `SNOWDROP_HOST_METRICS_PORT`
- `POSTGRES_HOST_PORT`
- `KVROCKS_HOST_PORT`
- `NEO4J_HOST_HTTP_PORT`
- `NEO4J_HOST_BOLT_PORT`

Defaults are documented in [.env.example](/e:/snowdrop-1/.env.example:1).

## Docker Compose

PowerShell example with defaults overridden:

```powershell
$env:SNOWDROP_HOST_PG_PORT = "15432"
$env:SNOWDROP_HOST_HEALTH_PORT = "18080"
$env:SNOWDROP_HOST_METRICS_PORT = "19090"
$env:POSTGRES_HOST_PORT = "15433"
$env:KVROCKS_HOST_PORT = "16666"
$env:NEO4J_HOST_HTTP_PORT = "17474"
$env:NEO4J_HOST_BOLT_PORT = "17687"

docker compose up -d
```

Internal container listeners can also be changed if needed:

```powershell
$env:PGWIRE_PORT = "5432"
$env:HEALTH_PORT = "8080"
$env:METRICS_PORT = "9090"
docker compose up -d
```

## Health, Metrics, SQL Client

Default endpoints:

```powershell
curl http://localhost:8080/health
curl http://localhost:9090/metrics
cargo run -p sndsql -- --port 5432 -d snowdrop
```

Custom host ports:

```powershell
curl http://localhost:18080/health
curl http://localhost:19090/metrics
cargo run -p sndsql -- --port 15432 -d snowdrop
```

## Testing Foundation

Canonical testing workflow:

```powershell
docker compose up -d --build orchestrator
$env:PGPASSWORD = "<your-postgres-password>"
cargo run -p snowdrop-testkit -- list-datasets
cargo run -p snowdrop-testkit -- run-all --with-tpch
```

Per-suite examples:

```powershell
cargo run -p snowdrop-testkit -- load --dataset tiny_smoke
cargo run -p snowdrop-testkit -- run --dataset tiny_smoke --suite golden
cargo run -p snowdrop-testkit -- run --dataset edge_cases --suite differential
cargo run -p snowdrop-testkit -- run --suite sqllogictest
cargo run -p snowdrop-testkit -- run --suite tap
cargo run -p snowdrop-testkit -- run --suite postgres-style
cargo run -p snowdrop-testkit -- report
```

Testing docs:

- [testing.en.md](/e:/snowdrop-1/docs/testing.en.md:1)
- [testing.ru.md](/e:/snowdrop-1/docs/testing.ru.md:1)
- [sql-compatibility-matrix.md](/e:/snowdrop-1/docs/sql-compatibility-matrix.md:1)
- [postgres-migration-readiness-roadmap.md](/e:/snowdrop-1/docs/postgres-migration-readiness-roadmap.md:1)

Generated reports:

- [reports/test-report.md](/e:/snowdrop-1/reports/test-report.md:1)
- [reports/README.md](/e:/snowdrop-1/reports/README.md:1)

## Benchmark Commands

Snowdrop vs PostgreSQL benchmark:

```powershell
$env:PGPASSWORD = "<your-postgres-password>"
cargo run --release -p tpch-bench -- --port 15432 --pg-port 15433 --scale 1 --runs 10 --with-indexes --seed 42 --output reports/benchmarks/tpch-results.json
```

Benchmark from a Docker-networked Rust container:

```powershell
docker run --rm --network snowdrop-1_default -e PGPASSWORD=$env:PGPASSWORD -v E:\snowdrop-1:/app -w /app rust:1.86-bookworm `
  bash -lc "export PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin && cargo run --release -p tpch-bench -- --host orchestrator --pg-host postgres --port 5432 --pg-port 5432 --scale 1 --runs 3 --with-indexes --seed 42 --output reports/benchmarks/tpch-results.json"
```

## Notes

- `docker-compose.yml` no longer forces `linux/arm64`, so it can run natively on `AMD64` hosts when matching images exist.
- PostgreSQL-backed differential suites and benchmarks read the password from `PGPASSWORD` first and fall back to `POSTGRES_PASSWORD`.
- After query-engine or SQL-surface changes, rebuild the `orchestrator` service before running `snowdrop-testkit`; otherwise the harness may talk to an older Snowdrop binary.
- `snowdrop-testkit run` and `run-all` now perform a small Phase 3 runtime preflight and will stop early with a rebuild hint if Snowdrop is stale. Set `SNOWDROP_SKIP_RUNTIME_PROBE=1` only if you intentionally want to bypass that guard.
- The benchmark suite already accepts non-default ports via `--port` and `--pg-port`.
- Use `--seed` for reproducible data generation across Snowdrop and PostgreSQL.
- Use `--host` and `--pg-host` when the benchmark client runs inside Docker instead of on the host.
- `tpch-bench` focuses on raw benchmark execution and JSON output, while the unified human-facing markdown report is owned by `snowdrop-testkit`.
