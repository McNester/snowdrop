# Live Binance Trades — Snowdrop vs PostgreSQL

A split-screen dashboard: the **same SQL** runs live against Snowdrop (left) and
PostgreSQL (right) over real Binance trade data. Pick a symbol, side, or price
range and both dashboards reload at once — Snowdrop's side fills in while
PostgreSQL is still working.

Each side runs one grouped scan
(`SELECT hour, COUNT(*), SUM(price*qty), AVG(price), SUM(is_buy) ... GROUP BY hour`)
over millions of rows and renders volume / trades / buy-vs-sell charts plus
headline tiles. Symbol and side are dictionary-encoded to integer columns and
`hour` is a plain int, so every predicate and the GROUP BY stay on Snowdrop's
SIMD-friendly columnar fast path. Results from both engines are identical;
typical gap is ~3x in Snowdrop's favour at 16M rows.

## Prerequisites

- Snowdrop orchestrator running on `:5432`, started with
  `SNOWDROP_ENABLE_TRUSTED_BULK_LOAD=1` (lets the loader use the bulk path).
- PostgreSQL on `:5434` (or set `POSTGRES_HOSTPORT="host port"`), user `postgres`
  / password `postgres`.

## 1. Fetch real data

Downloads Binance spot aggTrades (public, no auth) into `/tmp/binance-data`.
Default is 5 days × 6 symbols ≈ 16M trades.

```bash
./demo/crypto-demo/fetch-data.sh
# or choose dates / symbols:
DATES="2024-09-02 2024-09-03" ./demo/crypto-demo/fetch-data.sh BTCUSDT ETHUSDT
```

## 2. Load both engines

```bash
cargo run --release -p crypto-demo -- seed
```

Snowdrop loads via the trusted bulk path (~130k rows/s).

## 3. Serve

```bash
cargo run --release -p crypto-demo -- serve
# open http://localhost:8095
```

## Config (env)

| var | default | meaning |
|---|---|---|
| `SNOWDROP_CONN` | `host=127.0.0.1 port=5432 user=postgres password=postgres dbname=crypto` | Snowdrop connection |
| `POSTGRES_HOSTPORT` | `127.0.0.1 5434` | PostgreSQL host + port |
| `DATA_DIR` | `/tmp/binance-data` | seed CSV directory |
| `BATCH` | `2000` | insert batch size |
| `DEMO_PORT` | `8095` | web UI port |

## Benchmark tab (Grafana + Prometheus)

The **Benchmark** tab runs a battery of queries on both engines in a loop and
graphs the result live. One global run at a time — anyone visiting sees the run
in progress and can't start a second until it finishes.

- Suite selector: **Crypto trades** (the loaded `trades` table) or **TPC-H**
  (Q6/Q1/Q14 against the `tpch` db — load it once with `tpch-bench` first).
- `crypto-demo serve` exposes Prometheus metrics on `:9091` (`METRICS_PORT`):
  `bench_latency_ms`, `bench_speedup`, `bench_queries_total`, `bench_iteration`,
  `bench_running`.
- API: `POST /api/benchmark/start?suite=crypto|tpch` (idempotent),
  `GET /api/benchmark/status`.

### Bring up Grafana + Prometheus

```bash
docker compose -f docker-compose.monitoring.yml up -d
```
- Prometheus scrapes the host's native `:9090` (orchestrator) and `:9091`
  (crypto-demo) via `host.docker.internal`.
- Grafana on `:3000`, anonymous, embedding allowed, dashboard `snowbench`
  auto-provisioned. The Benchmark tab embeds it as an iframe
  (`http://<host>:3000/d/snowbench/...?kiosk`).

Open **port 3000** on the VM firewall so the browser can load the iframe (or
reverse-proxy `/grafana` through nginx with `serve_from_sub_path`). If the demo
is served over HTTPS, Grafana must be too (no mixed content).

Make sure `crypto-demo`'s systemd unit keeps `:9091` reachable (it binds
`0.0.0.0:9091` by default; set `Environment=METRICS_PORT=9091` to be explicit).

## Notes

- Each `/api/query` runs the query 5× per engine and reports the median.
- Results from both engines are identical (float rounding aside) — same schema,
  same SQL.
- PostgreSQL runs without an index on the filtered columns; both do a full scan,
  which is the honest "same query, no tuning" comparison.
