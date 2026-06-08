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
// per-row string compare — that is where Snowdrop's columnar scan wins big.
const CREATE_TABLE: &str = "CREATE TABLE trades (id BIGINT PRIMARY KEY, sym_id INT, \
     symbol VARCHAR(12), is_buy INT, price FLOAT, qty FLOAT, minute INT)";

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
            "{marker}INSERT INTO trades (id, sym_id, symbol, is_buy, price, qty, minute) VALUES "
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
                t.minute
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
    sd: Arc<Client>,
    pg: Arc<Client>,
    symbols: Arc<Vec<SymbolInfo>>,
}

#[derive(Serialize)]
struct EngineResult {
    median_ms: f64,
    trades: i64,
    volume: f64,
    avg_px: f64,
    low: f64,
    high: f64,
    error: Option<String>,
}

#[derive(Serialize)]
struct CompareResponse {
    sql: String,
    snowdrop: EngineResult,
    postgres: EngineResult,
    speedup: Option<f64>,
}

async fn serve() -> Result<()> {
    let sd = connect(&snowdrop_conn()).await?;
    let _ = sd.simple_query("USE crypto;").await;
    let pg = connect(&postgres_conn("crypto")).await?;

    // Discover symbols + their price range for the UI sliders.
    let symbols = load_symbols(&sd).await?;
    let first = symbols.first().map(|s| s.sym_id).unwrap_or(0);
    let app = App {
        sd: Arc::new(sd),
        pg: Arc::new(pg),
        symbols: Arc::new(symbols),
    };

    // Warm both engines (builds Snowdrop's columnar cache, PG's page cache).
    let _ = run_engine(&app.sd, &build_sql(first, None, None, None)).await;
    let _ = run_engine(&app.pg, &build_sql(first, None, None, None)).await;

    let port = std::env::var("DEMO_PORT").unwrap_or_else(|_| "8095".to_string());
    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr).await?;
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
    let mut buf = vec![0u8; 64 * 1024];
    let mut n = stream.read(&mut buf).await?;
    while !headers_complete(&buf[..n]) && n < buf.len() {
        let more = stream.read(&mut buf[n..]).await?;
        if more == 0 {
            break;
        }
        n += more;
    }
    let req = String::from_utf8_lossy(&buf[..n]).to_string();
    let (status, ctype, body) = route(&req, &app).await;
    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
        body.len()
    );
    stream.write_all(resp.as_bytes()).await?;
    stream.write_all(body.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

fn headers_complete(b: &[u8]) -> bool {
    b.windows(4).any(|w| w == b"\r\n\r\n")
}

/// One GROUP BY query (handled by Snowdrop's fast aggregate path) gives every
/// symbol and its price range for the UI.
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
    out.sort_by(|a, b| b.max.partial_cmp(&a.max).unwrap()); // priciest first
    Ok(out)
}

async fn route(req: &str, app: &App) -> (&'static str, &'static str, String) {
    let first = req.lines().next().unwrap_or("");
    if first.starts_with("GET / ") || first.starts_with("GET /index.html") {
        return ("200 OK", "text/html; charset=utf-8", INDEX_HTML.to_string());
    }
    if first.starts_with("GET /api/symbols") {
        return (
            "200 OK",
            "application/json",
            serde_json::to_string(app.symbols.as_ref()).unwrap(),
        );
    }
    if first.starts_with("POST /api/query") {
        let body = req.split("\r\n\r\n").nth(1).unwrap_or("");
        return ("200 OK", "application/json", api_query(body, app).await);
    }
    ("404 Not Found", "text/plain", "not found".to_string())
}

async fn api_query(body: &str, app: &App) -> String {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let req_symbol = v.get("symbol").and_then(|s| s.as_str()).unwrap_or("");
    let side = v.get("side").and_then(|s| s.as_str()).unwrap_or("ALL");
    let price_min = v.get("priceMin").and_then(|s| s.as_f64());
    let price_max = v.get("priceMax").and_then(|s| s.as_f64());

    // Resolve the symbol to its integer id; default to the first known symbol.
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

    let sql = build_sql(sym_id, is_buy, price_min, price_max);
    let snowdrop = run_engine(&app.sd, &sql).await;
    let postgres = run_engine(&app.pg, &sql).await;
    let speedup = match (snowdrop.median_ms, postgres.median_ms) {
        (s, p) if s > 0.0 && p > 0.0 => Some(p / s),
        _ => None,
    };
    serde_json::to_string(&CompareResponse {
        sql,
        snowdrop,
        postgres,
        speedup,
    })
    .unwrap()
}

fn build_sql(sym_id: i32, is_buy: Option<i32>, price_min: Option<f64>, price_max: Option<f64>) -> String {
    // All predicates are numeric (sym_id, is_buy, price) so the columnar scan
    // stays on the SIMD-friendly fast path.
    let mut where_clause = format!("sym_id = {sym_id}");
    if let Some(b) = is_buy {
        where_clause.push_str(&format!(" AND is_buy = {b}"));
    }
    if let Some(min) = price_min {
        where_clause.push_str(&format!(" AND price >= {min:.4}"));
    }
    if let Some(max) = price_max {
        where_clause.push_str(&format!(" AND price <= {max:.4}"));
    }
    // No COALESCE: Snowdrop's columnar aggregate path rejects it. Empty results
    // yield SQL NULL, which the metric parser already treats as zero.
    format!(
        "SELECT COUNT(*) AS trades, SUM(price * qty) AS volume, \
         AVG(price) AS avg_px, MIN(price) AS low, MAX(price) AS high \
         FROM trades WHERE {where_clause};"
    )
}

/// Run the query a few times and return the median latency + parsed metrics.
async fn run_engine(client: &Client, sql: &str) -> EngineResult {
    let mut samples = Vec::new();
    let mut last_row: Option<(i64, f64, f64, f64, f64)> = None;
    for _ in 0..5 {
        let t = Instant::now();
        match client.simple_query(sql).await {
            Ok(messages) => {
                samples.push(t.elapsed().as_secs_f64() * 1000.0);
                last_row = parse_metrics(&messages);
            }
            Err(e) => {
                return EngineResult {
                    median_ms: 0.0,
                    trades: 0,
                    volume: 0.0,
                    avg_px: 0.0,
                    low: 0.0,
                    high: 0.0,
                    error: Some(e.to_string()),
                };
            }
        }
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = samples[samples.len() / 2];
    let (trades, volume, avg_px, low, high) = last_row.unwrap_or((0, 0.0, 0.0, 0.0, 0.0));
    EngineResult {
        median_ms: median,
        trades,
        volume,
        avg_px,
        low,
        high,
        error: None,
    }
}

fn parse_metrics(messages: &[SimpleQueryMessage]) -> Option<(i64, f64, f64, f64, f64)> {
    for m in messages {
        if let SimpleQueryMessage::Row(row) = m {
            let g = |name: &str| row.get(name).unwrap_or("0");
            return Some((
                g("trades").parse().unwrap_or(0),
                g("volume").parse().unwrap_or(0.0),
                g("avg_px").parse().unwrap_or(0.0),
                g("low").parse().unwrap_or(0.0),
                g("high").parse().unwrap_or(0.0),
            ));
        }
    }
    None
}
