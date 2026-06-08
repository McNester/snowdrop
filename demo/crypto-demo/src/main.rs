//! Crypto trades explorer: the same filter+aggregate query runs live against
//! Snowdrop and PostgreSQL over real Binance trade data, so the latency
//! difference is visible and touchable.
//!
//!   crypto-demo seed    — load ./data CSVs (Binance aggTrades) into both engines
//!   crypto-demo serve   — serve the web UI and the /api/query endpoint

use anyhow::{Context, Result};
use serde::Serialize;
use std::io::{BufRead, BufReader};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
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

    let symbols = load_symbols(&sd[0]).await?;
    let first = symbols.first().map(|s| s.sym_id).unwrap_or(0);
    let app = App {
        sd,
        pg,
        symbols: Arc::new(symbols),
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

