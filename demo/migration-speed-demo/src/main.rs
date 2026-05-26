use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_postgres::{Client, NoTls, SimpleQueryMessage};

const INDEX_HTML: &str = include_str!("static/index.html");

#[derive(Debug, Clone, Deserialize, Serialize)]
struct QueryCase {
    name: String,
    sql: String,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QueryRequest {
    name: String,
    engine: Option<String>,
}

#[derive(Debug, Serialize)]
struct QueryRun {
    engine: String,
    latency_ms: f64,
    row_count: usize,
    rows: Vec<Vec<String>>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct CompareResponse {
    query: QueryCase,
    postgres: QueryRun,
    snowdrop: QueryRun,
    matched: bool,
    speedup_vs_postgres: Option<f64>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let port = env("DEMO_PORT", "8095");
    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr).await?;
    println!("migration-speed-demo listening on http://{addr}");

    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(err) = handle_connection(stream).await {
                eprintln!("request failed: {err:#}");
            }
        });
    }
}

async fn handle_connection(mut stream: TcpStream) -> Result<()> {
    let mut buffer = vec![0u8; 1024 * 1024];
    let mut read = stream.read(&mut buffer).await?;
    while !request_body_complete(&buffer[..read]) && read < buffer.len() {
        let n = stream.read(&mut buffer[read..]).await?;
        if n == 0 {
            break;
        }
        read += n;
    }
    let request = String::from_utf8_lossy(&buffer[..read]).to_string();
    let response = route_request(&request).await;
    stream.write_all(response.as_bytes()).await?;
    Ok(())
}

fn request_body_complete(buffer: &[u8]) -> bool {
    let Some(header_end) = find_header_end(buffer) else {
        return false;
    };
    let headers = String::from_utf8_lossy(&buffer[..header_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    buffer.len().saturating_sub(header_end + 4) >= content_length
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

async fn route_request(request: &str) -> String {
    match route_request_inner(request).await {
        Ok((status, content_type, body)) => http_response(status, content_type, &body),
        Err(err) => http_response(
            "500 Internal Server Error",
            "application/json",
            &serde_json::json!({ "error": format!("{err:#}") }).to_string(),
        ),
    }
}

async fn route_request_inner(request: &str) -> Result<(&'static str, &'static str, String)> {
    let (method, path) = parse_request_line(request)?;
    match (method, path.as_str()) {
        ("GET", "/") => Ok(("200 OK", "text/html; charset=utf-8", INDEX_HTML.to_string())),
        ("GET", "/health") => Ok((
            "200 OK",
            "application/json",
            serde_json::json!({ "status": "ok" }).to_string(),
        )),
        ("GET", "/queries") => Ok((
            "200 OK",
            "application/json",
            serde_json::to_string(&load_query_catalog()?)?,
        )),
        ("GET", "/latest-report") => Ok((
            "200 OK",
            "application/json",
            read_latest_report().unwrap_or_else(|| "{}".into()),
        )),
        ("POST", "/run-query") => {
            let body = parse_body(request);
            let req: QueryRequest = serde_json::from_str(body)?;
            let query = find_query(&req.name)?;
            let engine = req.engine.unwrap_or_else(|| "snowdrop".into());
            let run = run_query_on_engine(&engine, &query.sql).await;
            Ok((
                "200 OK",
                "application/json",
                serde_json::to_string(&serde_json::json!({ "query": query, "run": run }))?,
            ))
        }
        ("POST", "/compare-query") => {
            let body = parse_body(request);
            let req: QueryRequest = serde_json::from_str(body)?;
            let query = find_query(&req.name)?;
            let postgres = run_query_on_engine("postgres", &query.sql).await;
            let snowdrop = run_query_on_engine("snowdrop", &query.sql).await;
            let matched = postgres.error.is_none()
                && snowdrop.error.is_none()
                && postgres.rows == snowdrop.rows;
            let speedup_vs_postgres = if snowdrop.latency_ms > 0.0 && postgres.error.is_none() {
                Some(postgres.latency_ms / snowdrop.latency_ms)
            } else {
                None
            };
            Ok((
                "200 OK",
                "application/json",
                serde_json::to_string(&CompareResponse {
                    query,
                    postgres,
                    snowdrop,
                    matched,
                    speedup_vs_postgres,
                })?,
            ))
        }
        _ => Ok((
            "404 Not Found",
            "application/json",
            serde_json::json!({ "error": "not found" }).to_string(),
        )),
    }
}

fn parse_request_line(request: &str) -> Result<(&str, String)> {
    let first = request
        .lines()
        .next()
        .ok_or_else(|| anyhow!("empty request"))?;
    let mut parts = first.split_whitespace();
    let method = parts.next().ok_or_else(|| anyhow!("missing method"))?;
    let path = parts.next().ok_or_else(|| anyhow!("missing path"))?;
    Ok((method, path.split('?').next().unwrap_or(path).to_string()))
}

fn parse_body(request: &str) -> &str {
    request.split("\r\n\r\n").nth(1).unwrap_or("")
}

fn http_response(status: &str, content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{body}",
        body.as_bytes().len()
    )
}

fn load_query_catalog() -> Result<Vec<QueryCase>> {
    let path = env(
        "QUERY_CATALOG_PATH",
        "tests/large_postgres_bases_for_test/ticketing_orders/benchmark_queries_snowdrop_favorable.json",
    );
    let raw = fs::read_to_string(&path).with_context(|| format!("could not read {path}"))?;
    Ok(serde_json::from_str(&raw)?)
}

fn find_query(name: &str) -> Result<QueryCase> {
    load_query_catalog()?
        .into_iter()
        .find(|query| query.name == name)
        .ok_or_else(|| anyhow!("unknown query '{name}'"))
}

async fn run_query_on_engine(engine: &str, sql: &str) -> QueryRun {
    match run_query_on_engine_inner(engine, sql).await {
        Ok(run) => run,
        Err(err) => QueryRun {
            engine: engine.into(),
            latency_ms: 0.0,
            row_count: 0,
            rows: Vec::new(),
            error: Some(format!("{err:#}")),
        },
    }
}

async fn run_query_on_engine_inner(engine: &str, sql: &str) -> Result<QueryRun> {
    let client = connect_engine(engine).await?;
    let started = Instant::now();
    let messages = client.simple_query(sql).await?;
    let latency_ms = started.elapsed().as_secs_f64() * 1000.0;
    let rows = messages
        .into_iter()
        .filter_map(|message| match message {
            SimpleQueryMessage::Row(row) => Some(
                (0..row.len())
                    .map(|idx| row.get(idx).unwrap_or("NULL").to_string())
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .collect::<Vec<_>>();

    Ok(QueryRun {
        engine: engine.into(),
        latency_ms,
        row_count: rows.len(),
        rows: rows.into_iter().take(50).collect(),
        error: None,
    })
}

async fn connect_engine(engine: &str) -> Result<Client> {
    let mut cfg = tokio_postgres::Config::new();
    match engine {
        "postgres" => {
            cfg.host(&env("POSTGRES_HOST", "postgres"))
                .port(env("POSTGRES_PORT", "5432").parse()?)
                .user(&env("POSTGRES_USER", "postgres"))
                .password(&env("POSTGRES_PASSWORD", "postgres"))
                .dbname(&env("POSTGRES_DB", "ticketing_orders_large"));
        }
        "snowdrop" => {
            cfg.host(&env("SNOWDROP_HOST", "orchestrator"))
                .port(env("SNOWDROP_PORT", "5432").parse()?)
                .user(&env("SNOWDROP_USER", "postgres"))
                .dbname(&env("SNOWDROP_DB", "ticketing_orders_large_snowdrop"));
        }
        other => return Err(anyhow!("unknown engine '{other}'")),
    }

    let (client, connection) = cfg.connect(NoTls).await?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    Ok(client)
}

fn read_latest_report() -> Option<String> {
    let path = env(
        "LATEST_REPORT_PATH",
        "reports/migration/ticketing-orders-large-ticketing-orders-large-snowdrop-auto.json",
    );
    fs::read_to_string(path).ok()
}

fn env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.into())
}
