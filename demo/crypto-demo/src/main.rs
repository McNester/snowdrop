//! Crypto trades explorer: the same filter+aggregate query runs live against
//! Snowdrop and PostgreSQL over real Binance trade data, so the latency
//! difference is visible and touchable.
//!
//!   crypto-demo seed    — load ./data CSVs (Binance aggTrades) into both engines
//!   crypto-demo serve   — serve the web UI and the /api/query endpoint

use anyhow::{Context, Result};
use serde::Serialize;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio_postgres::{Client, NoTls, SimpleQueryMessage};

const INDEX_HTML: &str = include_str!("static/index.html");

fn data_dir() -> String {
    std::env::var("DATA_DIR").unwrap_or_else(|_| "/tmp/binance-data".to_string())
}

fn snowdrop_conn() -> String {
    std::env::var("SNOWDROP_CONN").unwrap_or_else(|_| {
        "host=127.0.0.1 port=5432 user=postgres password=postgres dbname=crypto".to_string()
    })
}

fn postgres_conn(dbname: &str) -> String {
    let base = std::env::var("POSTGRES_HOSTPORT").unwrap_or_else(|_| "127.0.0.1 5434".to_string());
    let mut it = base.split_whitespace();
    let host = it.next().unwrap_or("127.0.0.1");
    let port = it.next().unwrap_or("5434");
    format!("host={host} port={port} user=postgres password=postgres dbname={dbname}")
}

async fn connect(conn: &str) -> Result<Client> {
    let (client, connection) = tokio_postgres::connect(conn, NoTls)
        .await
        .with_context(|| format!("connect failed: {conn}"))?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    Ok(client)
}

#[tokio::main]
async fn main() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("seed") => seed().await,
        Some("serve") => serve().await,
        _ => {
            eprintln!("usage: crypto-demo [seed|serve]");
            Ok(())
        }
    }
}

// ── Seeding ──────────────────────────────────────────────────────────────

/// One trade row, parsed from a Binance aggTrades CSV line.
struct Trade {
    is_buy: u8,
    price: f64,
    qty: f64,
    minute: u32,
}

/// Parse a Binance aggTrades CSV file. Columns:
/// aggId, price, qty, firstId, lastId, timestamp(ms), isBuyerMaker, isBestMatch.
/// Trade side = taker side: isBuyerMaker=true means the taker sold.
fn parse_csv(path: &std::path::Path) -> Result<Vec<Trade>> {
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut rows = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let mut f = line.split(',');
        let _id = f.next();
        let price: f64 = match f.next().and_then(|v| v.parse().ok()) {
            Some(p) => p,
            None => continue,
        };
        let qty: f64 = f.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let _first = f.next();
        let _last = f.next();
        let ts_ms: i64 = f.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        let is_buyer_maker = f.next().map(|v| v.eq_ignore_ascii_case("true")).unwrap_or(false);
        rows.push(Trade {
            is_buy: if is_buyer_maker { 0 } else { 1 },
            price,
            qty,
            minute: ((ts_ms / 60_000) % 1440) as u32, // minute of day (UTC)
        });
    }
    Ok(rows)
}

// symbol and side are dictionary-encoded to integer columns (sym_id, is_buy) so
// every filter the UI sends is a SIMD-friendly numeric comparison rather than a
// per-row string compare. `hour` (0-23) is a plain int so GROUP BY hour takes
// Snowdrop's fast integer-keyed grouping path.
const CREATE_TABLE: &str = "CREATE TABLE trades (id BIGINT PRIMARY KEY, sym_id INT, \
     symbol VARCHAR(12), is_buy INT, price FLOAT, qty FLOAT, hour INT)";

async fn seed() -> Result<()> {
    let dir = data_dir();
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .with_context(|| format!("read data dir {dir} (run fetch-data.sh first)"))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "csv").unwrap_or(false))
        .collect();
    files.sort();
    if files.is_empty() {
        anyhow::bail!("no CSVs in {dir} — run demo/crypto-demo/fetch-data.sh");
    }

    println!("PostgreSQL: creating database + table ...");
    let pg_boot = connect(&postgres_conn("postgres")).await?;
    let _ = pg_boot.simple_query("CREATE DATABASE crypto;").await;
    drop(pg_boot);
    let pg = connect(&postgres_conn("crypto")).await?;
    pg.simple_query("DROP TABLE IF EXISTS trades;").await?;
    pg.simple_query(&CREATE_TABLE.replace("FLOAT", "DOUBLE PRECISION"))
        .await?;

    println!("Snowdrop: creating database + table ...");
    let sd = connect(&snowdrop_conn()).await?;
    let _ = sd.simple_query("CREATE DATABASE crypto;").await;
    let _ = sd.simple_query("USE crypto;").await;
    let _ = sd.simple_query("DROP TABLE trades;").await;
    sd.simple_query(CREATE_TABLE).await?;

    let mut id = 1u64;
    let mut grand_total = 0usize;
    let started = Instant::now();
    for (sym_id, path) in files.iter().enumerate() {
        let symbol = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("UNK")
            .trim_end_matches("USDT")
            .to_string();
        let rows = parse_csv(path)?;
        println!("{symbol} (id {sym_id}): {} trades", rows.len());
        insert_symbol(&pg, "PostgreSQL", sym_id as u32, &symbol, &rows, &mut id.clone()).await?;
        insert_symbol(&sd, "Snowdrop", sym_id as u32, &symbol, &rows, &mut id).await?;
        grand_total += rows.len();
    }
    println!(
        "Done. {grand_total} real trades in both engines in {:.1}s. Run: crypto-demo serve",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

async fn insert_symbol(
    client: &Client,
    label: &str,
    sym_id: u32,
    symbol: &str,
    rows: &[Trade],
    id: &mut u64,
) -> Result<()> {
    let batch = std::env::var("BATCH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2000usize);
    // Snowdrop ignores the marker; with SNOWDROP_ENABLE_TRUSTED_BULK_LOAD set it
    // skips the per-batch constraint/FK round trips for a fresh bulk load.
    let marker = if label == "Snowdrop" {
        "/* snowdrop:trusted_bulk_load */ "
    } else {
        ""
    };
    for chunk in rows.chunks(batch) {
        let mut sql = format!(
            "{marker}INSERT INTO trades (id, sym_id, symbol, is_buy, price, qty, hour) VALUES "
        );
        for (i, t) in chunk.iter().enumerate() {
            if i > 0 {
                sql.push(',');
            }
            sql.push_str(&format!(
                "({},{},'{}',{},{:.2},{:.6},{})",
                *id + i as u64,
                sym_id,
                symbol,
                t.is_buy,
                t.price,
                t.qty,
                t.minute / 60
            ));
        }
        sql.push(';');
        client
            .simple_query(&sql)
            .await
            .with_context(|| format!("{label} insert batch ({symbol})"))?;
        *id += chunk.len() as u64;
    }
    Ok(())
}

// ── Serving ──────────────────────────────────────────────────────────────

#[derive(Clone, Serialize)]
struct SymbolInfo {
    symbol: String,
    sym_id: i32,
    min: f64,
    max: f64,
}

#[derive(Clone)]
struct App {
    sd: Vec<Arc<Client>>,
    pg: Vec<Arc<Client>>,
    symbols: Arc<Vec<SymbolInfo>>,
    bench: Arc<BenchShared>,
}

#[derive(Serialize, Default)]
struct Totals {
    trades: i64,
    volume: f64,
    avg_px: f64,
}

#[derive(Serialize)]
struct HourBucket {
    hour: i32,
    trades: i64,
    volume: f64,
    avg_px: f64,
    buys: i64,
}

#[derive(Serialize)]
struct DashboardResponse {
    engine: String,
    ms: f64,
    totals: Totals,
    hourly: Vec<HourBucket>,
    error: Option<String>,
}

// ── Benchmark (Grafana-backed, single global idempotent run) ───────────────

/// Default run length (seconds) if the caller doesn't pass `secs`. Clamped to
/// [10, 600]. Long enough for Prometheus to plot a clear time series.
const BENCH_DEFAULT_SECS: u64 = 60;

#[derive(Serialize, Clone, Default)]
struct QueryStat {
    query: String,
    sql: String,
    sd_ms: f64,
    pg_ms: f64,
    speedup: f64,
}

#[derive(Serialize, Clone, Default)]
struct BenchStatus {
    running: bool,
    finished: bool,
    suite: String,
    iteration: u64,
    elapsed_s: f64,
    duration_s: u64,
    current: String,
    stats: Vec<QueryStat>,
}

#[derive(Default)]
struct BenchShared {
    running: AtomicBool,
    status: Mutex<BenchStatus>,
}

impl BenchShared {
    async fn status_json(&self) -> String {
        let mut s = self.status.lock().await.clone();
        s.running = self.running.load(Ordering::SeqCst);
        serde_json::to_string(&s).unwrap()
    }
}

/// Idempotent: only one run at a time. If a run is in progress, this is a no-op
/// and the caller just sees the live status. The run is spawned detached.
fn start_benchmark(app: &App, suite: &str, duration: Duration) {
    if app
        .running_cas()
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return; // already running
    }
    let app = app.clone();
    let suite = suite.to_string();
    tokio::spawn(async move {
        if let Err(e) = run_benchmark(&app, &suite, duration).await {
            eprintln!("benchmark error: {e:#}");
        }
        app.bench.running.store(false, Ordering::SeqCst);
        metrics::gauge!("bench_running").set(0.0);
        let mut st = app.bench.status.lock().await;
        st.running = false;
        st.finished = true;
        st.current = String::new();
    });
}

impl App {
    fn running_cas(&self) -> &AtomicBool {
        &self.bench.running
    }
}

/// (display name, snowdrop SQL, postgres SQL). For TPC-H, Snowdrop reads the
/// cross-db qualified `tpch.` tables; PostgreSQL connects to the `tpch` db.
fn suite_queries(suite: &str) -> Vec<(&'static str, String, String)> {
    if suite == "tpch" {
        let sd = |s: &str| s.replace("{p}", "tpch.");
        let pg = |s: &str| s.replace("{p}", "");
        let q6 = "SELECT SUM(l_extendedprice * l_discount) AS revenue FROM {p}lineitem \
                  WHERE l_shipdate >= '1994-01-01' AND l_shipdate < '1995-01-01' \
                  AND l_discount >= 0.05 AND l_discount <= 0.07 AND l_quantity < 24;";
        let q1 = "SELECT l_returnflag, l_linestatus, SUM(l_quantity), SUM(l_extendedprice), \
                  SUM(l_extendedprice * (1 - l_discount)), AVG(l_quantity), AVG(l_extendedprice), \
                  AVG(l_discount), COUNT(*) FROM {p}lineitem WHERE l_shipdate <= '1998-09-02' \
                  GROUP BY l_returnflag, l_linestatus ORDER BY l_returnflag, l_linestatus;";
        // Aliases promo_revenue / total_revenue + the l/p aliases are required by
        // Snowdrop's Q14 fast path (it's a join — the general path is far slower).
        let q14 = "SELECT SUM(CASE WHEN p_type LIKE 'PROMO%' THEN l_extendedprice * (1 - l_discount) \
                   ELSE 0 END) AS promo_revenue, SUM(l_extendedprice * (1 - l_discount)) AS total_revenue \
                   FROM {p}lineitem l INNER JOIN {p}part p ON l.l_partkey = p.p_partkey \
                   WHERE l.l_shipdate >= '1995-01-01' AND l.l_shipdate < '1995-02-01';";
        vec![
            ("Q6 revenue", sd(q6), pg(q6)),
            ("Q1 pricing", sd(q1), pg(q1)),
            ("Q14 promo", sd(q14), pg(q14)),
        ]
    } else {
        // Crypto trades — same SQL on both engines (table `trades`).
        let q = |s: &str| (s.to_string(), s.to_string());
        let (c1s, c1p) = q("SELECT COUNT(*), SUM(price*qty), AVG(price) FROM trades WHERE is_buy = 1;");
        let (c2s, c2p) =
            q("SELECT COUNT(*), SUM(price*qty), AVG(price) FROM trades WHERE hour >= 9 AND hour <= 16;");
        let (c3s, c3p) =
            q("SELECT hour, COUNT(*), SUM(price*qty) FROM trades WHERE is_buy = 1 GROUP BY hour ORDER BY hour;");
        let (c4s, c4p) = q("SELECT COUNT(*), AVG(price), SUM(qty) FROM trades WHERE price >= 0;");
        vec![
            ("Buy-side volume", c1s, c1p),
            ("Market hours", c2s, c2p),
            ("Volume by hour", c3s, c3p),
            ("Full scan", c4s, c4p),
        ]
    }
}

async fn time_query(client: &Client, sql: &str) -> f64 {
    let t = Instant::now();
    let _ = client.simple_query(sql).await;
    t.elapsed().as_secs_f64() * 1000.0
}

async fn set_current(app: &App, what: &str) {
    app.bench.status.lock().await.current = what.to_string();
}

async fn run_benchmark(app: &App, suite: &str, duration: Duration) -> Result<()> {
    metrics::gauge!("bench_running").set(1.0);
    let queries = suite_queries(suite);
    {
        let mut st = app.bench.status.lock().await;
        *st = BenchStatus {
            running: true,
            finished: false,
            suite: suite.to_string(),
            iteration: 0,
            elapsed_s: 0.0,
            duration_s: duration.as_secs(),
            current: String::new(),
            stats: queries
                .iter()
                .map(|(n, sd_sql, _)| QueryStat {
                    query: n.to_string(),
                    sql: sd_sql.clone(),
                    ..Default::default()
                })
                .collect(),
        };
    }

    // Dedicated connections so the benchmark doesn't fight the live dashboard.
    let sd = connect(&snowdrop_conn()).await?;
    let _ = sd.simple_query("USE crypto;").await;
    let pg_db = if suite == "tpch" { "tpch" } else { "crypto" };
    let pg = connect(&postgres_conn(pg_db)).await?;

    let start = Instant::now();
    let mut iteration = 0u64;
    while start.elapsed() < duration {
        iteration += 1;
        for (i, (name, sd_sql, pg_sql)) in queries.iter().enumerate() {
            set_current(app, &format!("Snowdrop · {name}")).await;
            let sd_ms = time_query(&sd, sd_sql).await;
            set_current(app, &format!("PostgreSQL · {name}")).await;
            let pg_ms = time_query(&pg, pg_sql).await;
            let speedup = if sd_ms > 0.0 { pg_ms / sd_ms } else { 0.0 };

            metrics::gauge!("bench_latency_ms", "engine" => "snowdrop", "query" => name.to_string(), "suite" => suite.to_string()).set(sd_ms);
            metrics::gauge!("bench_latency_ms", "engine" => "postgres", "query" => name.to_string(), "suite" => suite.to_string()).set(pg_ms);
            metrics::gauge!("bench_speedup", "query" => name.to_string(), "suite" => suite.to_string()).set(speedup);
            metrics::counter!("bench_queries_total", "engine" => "snowdrop").increment(1);
            metrics::counter!("bench_queries_total", "engine" => "postgres").increment(1);

            let mut st = app.bench.status.lock().await;
            if let Some(slot) = st.stats.get_mut(i) {
                slot.sd_ms = sd_ms;
                slot.pg_ms = pg_ms;
                slot.speedup = speedup;
            }
        }
        metrics::gauge!("bench_iteration", "suite" => suite.to_string()).set(iteration as f64);
        {
            let mut st = app.bench.status.lock().await;
            st.iteration = iteration;
            st.elapsed_s = start.elapsed().as_secs_f64();
        }
        // Brief pause so Prometheus can scrape the moving gauges.
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Ok(())
}

async fn serve() -> Result<()> {
    // Two connections per engine so the totals and hourly queries run in
    // parallel (real server-side concurrency, not pipelined on one connection).
    let mut sd = Vec::new();
    let mut pg = Vec::new();
    for _ in 0..2 {
        let c = connect(&snowdrop_conn()).await?;
        let _ = c.simple_query("USE crypto;").await;
        sd.push(Arc::new(c));
        pg.push(Arc::new(connect(&postgres_conn("crypto")).await?));
    }

    // Prometheus metrics endpoint (scraped by Prometheus → graphed in Grafana).
    let metrics_port: u16 = std::env::var("METRICS_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(9091);
    if let Err(e) = metrics_exporter_prometheus::PrometheusBuilder::new()
        .with_http_listener(([0, 0, 0, 0], metrics_port))
        .install()
    {
        eprintln!("metrics exporter not started: {e}");
    } else {
        println!("metrics on http://localhost:{metrics_port}/metrics");
    }
    metrics::gauge!("bench_running").set(0.0);

    let symbols = load_symbols(&sd[0]).await?;
    let first = symbols.first().map(|s| s.sym_id).unwrap_or(0);
    let app = App {
        sd,
        pg,
        symbols: Arc::new(symbols),
        bench: Arc::new(BenchShared::default()),
    };

    // Warm both engines (builds Snowdrop's columnar cache, PG's page cache).
    let _ = run_dashboard(&app, "snowdrop", first, None, None, None).await;
    let _ = run_dashboard(&app, "postgres", first, None, None, None).await;

    let port = std::env::var("DEMO_PORT").unwrap_or_else(|_| "8095".to_string());
    let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    println!("crypto-demo on http://localhost:{port}");
    loop {
        let (stream, _) = listener.accept().await?;
        let app = app.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(stream, app).await {
                eprintln!("request error: {e:#}");
            }
        });
    }
}

async fn handle(mut stream: TcpStream, app: App) -> Result<()> {
    let mut buf = vec![0u8; 16 * 1024];
    let n = stream.read(&mut buf).await?;
    let req = String::from_utf8_lossy(&buf[..n]).to_string();
    let line = req.lines().next().unwrap_or("");
    let (status, ctype, body) = route(line, &app).await;
    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
        body.len()
    );
    stream.write_all(resp.as_bytes()).await?;
    stream.write_all(body.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

async fn load_symbols(client: &Client) -> Result<Vec<SymbolInfo>> {
    let sql = "SELECT symbol, sym_id, MIN(price) AS lo, MAX(price) AS hi \
               FROM trades GROUP BY symbol, sym_id;";
    let messages = client.simple_query(sql).await?;
    let mut out = Vec::new();
    for m in &messages {
        if let SimpleQueryMessage::Row(row) = m {
            out.push(SymbolInfo {
                symbol: row.get("symbol").unwrap_or("").to_string(),
                sym_id: row.get("sym_id").and_then(|v| v.parse().ok()).unwrap_or(0),
                min: row.get("lo").and_then(|v| v.parse().ok()).unwrap_or(0.0),
                max: row.get("hi").and_then(|v| v.parse().ok()).unwrap_or(0.0),
            });
        }
    }
    out.sort_by(|a, b| b.max.partial_cmp(&a.max).unwrap());
    Ok(out)
}

async fn route(line: &str, app: &App) -> (&'static str, &'static str, String) {
    if line.starts_with("GET / ") || line.starts_with("GET /index.html") {
        return ("200 OK", "text/html; charset=utf-8", INDEX_HTML.to_string());
    }
    if line.starts_with("GET /api/symbols") {
        return (
            "200 OK",
            "application/json",
            serde_json::to_string(app.symbols.as_ref()).unwrap(),
        );
    }
    if line.starts_with("GET /api/dashboard") {
        return ("200 OK", "application/json", api_dashboard(line, app).await);
    }
    if line.starts_with("POST /api/benchmark/start") {
        let suite = match query_param(line, "suite") {
            Some("tpch") => "tpch",
            _ => "crypto",
        };
        let secs = query_param(line, "secs")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(BENCH_DEFAULT_SECS)
            .clamp(10, 600);
        start_benchmark(app, suite, Duration::from_secs(secs));
        return ("200 OK", "application/json", app.bench.status_json().await);
    }
    if line.starts_with("GET /api/benchmark/status") {
        return ("200 OK", "application/json", app.bench.status_json().await);
    }
    ("404 Not Found", "text/plain", "not found".to_string())
}

fn query_param<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let q = line.split('?').nth(1)?.split(' ').next()?;
    q.split('&')
        .find_map(|kv| kv.strip_prefix(&format!("{key}=")))
}

async fn api_dashboard(line: &str, app: &App) -> String {
    let engine = query_param(line, "engine").unwrap_or("snowdrop");
    let req_symbol = query_param(line, "sym").unwrap_or("");
    let side = query_param(line, "side").unwrap_or("ALL");
    let pmin = query_param(line, "pmin").and_then(|v| v.parse().ok());
    let pmax = query_param(line, "pmax").and_then(|v| v.parse().ok());

    let sym_id = app
        .symbols
        .iter()
        .find(|s| s.symbol.eq_ignore_ascii_case(req_symbol))
        .or_else(|| app.symbols.first())
        .map(|s| s.sym_id)
        .unwrap_or(0);
    let is_buy = match side.to_uppercase().as_str() {
        "BUY" => Some(1),
        "SELL" => Some(0),
        _ => None,
    };
    let resp = run_dashboard(app, engine, sym_id, is_buy, pmin, pmax).await;
    serde_json::to_string(&resp).unwrap()
}

fn where_clause(sym_id: i32, is_buy: Option<i32>, pmin: Option<f64>, pmax: Option<f64>) -> String {
    let mut w = format!("sym_id = {sym_id}");
    if let Some(b) = is_buy {
        w.push_str(&format!(" AND is_buy = {b}"));
    }
    if let Some(min) = pmin {
        w.push_str(&format!(" AND price >= {min:.4}"));
    }
    if let Some(max) = pmax {
        w.push_str(&format!(" AND price <= {max:.4}"));
    }
    w
}

/// Run the dashboard's single grouped query for one engine; the totals tiles
/// are derived from the hourly buckets so only one scan is needed.
async fn run_dashboard(
    app: &App,
    engine: &str,
    sym_id: i32,
    is_buy: Option<i32>,
    pmin: Option<f64>,
    pmax: Option<f64>,
) -> DashboardResponse {
    let client = if engine == "postgres" { &app.pg[0] } else { &app.sd[0] };
    let w = where_clause(sym_id, is_buy, pmin, pmax);
    let hourly_sql = format!(
        "SELECT hour, COUNT(*) AS trades, SUM(price*qty) AS volume, AVG(price) AS avg_px, \
         SUM(is_buy) AS buys FROM trades WHERE {w} GROUP BY hour ORDER BY hour;"
    );

    let t = Instant::now();
    let res = client.simple_query(&hourly_sql).await;
    let ms = t.elapsed().as_secs_f64() * 1000.0;

    let mut resp = DashboardResponse {
        engine: engine.to_string(),
        ms,
        totals: Totals::default(),
        hourly: Vec::new(),
        error: None,
    };
    match res {
        Ok(hm) => {
            resp.hourly = parse_hourly(&hm);
            resp.totals = totals_from_hourly(&resp.hourly);
        }
        Err(e) => resp.error = Some(e.to_string()),
    }
    resp
}

/// Roll per-hour buckets up into the headline totals. avg_px is a trade-count
/// weighted mean of the per-hour averages.
fn totals_from_hourly(hourly: &[HourBucket]) -> Totals {
    let mut trades = 0i64;
    let mut volume = 0.0;
    let mut price_sum = 0.0;
    for h in hourly {
        trades += h.trades;
        volume += h.volume;
        price_sum += h.avg_px * h.trades as f64;
    }
    Totals {
        trades,
        volume,
        avg_px: if trades > 0 { price_sum / trades as f64 } else { 0.0 },
    }
}

fn rget(row: &tokio_postgres::SimpleQueryRow, name: &str) -> String {
    row.get(name).unwrap_or("0").to_string()
}

fn parse_hourly(messages: &[SimpleQueryMessage]) -> Vec<HourBucket> {
    let mut out = Vec::new();
    for m in messages {
        if let SimpleQueryMessage::Row(r) = m {
            out.push(HourBucket {
                hour: rget(r, "hour").parse().unwrap_or(0),
                trades: rget(r, "trades").parse().unwrap_or(0),
                volume: rget(r, "volume").parse().unwrap_or(0.0),
                avg_px: rget(r, "avg_px").parse().unwrap_or(0.0),
                buys: rget(r, "buys").parse().unwrap_or(0),
            });
        }
    }
    out
}

