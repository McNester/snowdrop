# Live Trades Explorer — Snowdrop vs PostgreSQL

A touchable demo: the **same SQL** runs live against Snowdrop and PostgreSQL over
real Binance trade data. Pick a symbol, side, and price range — watch the
latency bars.

It is a single-table filtered aggregate (`COUNT`, `SUM(price*qty)`, `AVG`, `MIN`,
`MAX`) over millions of rows. Symbol and side are dictionary-encoded to integer
columns so every predicate is a numeric (SIMD-friendly) comparison — Snowdrop's
columnar cache scans these far faster than PostgreSQL's row scan.

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

## Notes

- Each `/api/query` runs the query 5× per engine and reports the median.
- Results from both engines are identical (float rounding aside) — same schema,
  same SQL.
- PostgreSQL runs without an index on the filtered columns; both do a full scan,
  which is the honest "same query, no tuning" comparison.
