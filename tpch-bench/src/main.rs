use rand::Rng;
use serde::Serialize;
use std::time::Instant;
use tokio_postgres::{NoTls, SimpleQueryMessage};

const SCALE: usize = 1;

const CUSTOMERS: usize = 150 * SCALE;
const ORDERS: usize = 1500 * SCALE;
const LINEITEMS_PER_ORDER: usize = 4;
const PARTS: usize = 200 * SCALE;

const SEGMENTS: [&str; 5] = ["AUTOMOBILE", "BUILDING", "FURNITURE", "MACHINERY", "HOUSEHOLD"];
const RETURN_FLAGS: [&str; 3] = ["A", "N", "R"];
const LINE_STATUSES: [&str; 2] = ["F", "O"];
const PART_TYPES: [&str; 6] = [
    "PROMO BURNISHED COPPER", "STANDARD POLISHED TIN",
    "PROMO ANODIZED STEEL", "ECONOMY BRUSHED BRASS",
    "SMALL PLATED NICKEL", "MEDIUM BURNISHED IRON",
];

#[derive(Serialize, Clone)]
struct QueryResult {
    query: String,
    system: String,
    scale: usize,
    phase: String, // "baseline" or "indexed"
    runs: Vec<RunResult>,
}

#[derive(Serialize, Clone)]
struct RunResult {
    run: u32,
    latency_ms: f64,
    row_count: usize,
}

struct Config {
    host: String,
    port: u16,
    user: String,
    dbname: String,
    label: String,
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut scale = 1usize;
    let mut with_indexes = false;
    let mut output_path: Option<String> = None;
    let mut snowdrop_port = 5432u16;
    let mut pg_port = 5433u16;
    let mut runs = 3u32;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--scale" | "-s" => { i += 1; scale = args[i].parse().unwrap_or(1); }
            "--with-indexes" => { with_indexes = true; }
            "--output" | "-o" => { i += 1; output_path = Some(args[i].clone()); }
            "--port" => { i += 1; snowdrop_port = args[i].parse().unwrap_or(5432); }
            "--pg-port" => { i += 1; pg_port = args[i].parse().unwrap_or(5433); }
            "--runs" | "-r" => { i += 1; runs = args[i].parse().unwrap_or(3); }
            _ => { scale = args[i].parse().unwrap_or(1); }
        }
        i += 1;
    }

    let total_lineitems = scale * ORDERS * LINEITEMS_PER_ORDER;
    println!("TPC-H Benchmark (scale={}, ~{} lineitem rows, runs={})", scale, total_lineitems, runs);
    if with_indexes { println!("Index mode: will create indexes after baseline"); }

    let snowdrop = Config {
        host: "localhost".into(),
        port: snowdrop_port,
        user: "snowdrop".into(),
        dbname: "snowdrop".into(),
        label: "Snowdrop".into(),
    };

    let postgres = Config {
        host: "localhost".into(),
        port: pg_port,
        user: "postgres".into(),
        dbname: "tpch".into(),
        label: "PostgreSQL 16".into(),
    };

    let mut all_results: Vec<QueryResult> = Vec::new();

    // Run Snowdrop benchmark
    println!("\n--- Loading data into {} ---", snowdrop.label);
    match run_full_benchmark(&snowdrop, scale, true, with_indexes, runs).await {
        Ok(results) => { all_results.extend(results); }
        Err(e) => { eprintln!("Snowdrop error: {}", e); }
    }

    // Run PostgreSQL benchmark
    println!("\n--- Loading data into {} ---", postgres.label);
    match run_full_benchmark(&postgres, scale, false, false, runs).await {
        Ok(results) => { all_results.extend(results); }
        Err(e) => { eprintln!("PostgreSQL error: {}", e); }
    }

    // Generate report
    print_summary_table(&all_results, scale);

    // Save JSON output
    if let Some(path) = output_path {
        let json = serde_json::to_string_pretty(&all_results).unwrap();
        std::fs::write(&path, json).unwrap();
        println!("\nResults saved to {}", path);
    }

    // Generate markdown report
    generate_report(&all_results, scale, with_indexes);
}

async fn run_full_benchmark(
    config: &Config,
    scale: usize,
    is_snowdrop: bool,
    with_indexes: bool,
    runs: u32,
) -> Result<Vec<QueryResult>, Box<dyn std::error::Error>> {
    let password = if is_snowdrop { "" } else { " password=postgres" };
    let conn_str = format!(
        "host={} port={} user={} dbname={}{}",
        config.host, config.port, config.user, config.dbname, password
    );
    let (client, connection) = tokio_postgres::connect(&conn_str, NoTls).await?;
    tokio::spawn(async move { let _ = connection.await; });

    create_schema(&client, is_snowdrop).await?;

    let load_start = Instant::now();
    load_data(&client, scale, is_snowdrop).await?;
    println!("Load time: {:.1}s", load_start.elapsed().as_secs_f64());

    let mut all_results = Vec::new();

    // Baseline benchmark (no indexes)
    println!("\n=== Baseline: {} ===\n", config.label);
    let queries = build_queries(is_snowdrop);
    let results = run_benchmark(&client, &queries, &config.label, scale, "baseline", runs).await?;
    all_results.extend(results);

    // Indexed benchmark (Snowdrop only)
    if with_indexes && is_snowdrop {
        println!("\n--- Creating indexes ---");
        let idx_start = Instant::now();
        client.simple_query("CREATE INDEX idx_shipdate ON tpch.lineitem(l_shipdate);").await?;
        println!("  idx_shipdate created in {:.1}ms", idx_start.elapsed().as_secs_f64() * 1000.0);

        let idx_start = Instant::now();
        client.simple_query("CREATE INDEX idx_partkey ON tpch.lineitem(l_partkey);").await?;
        println!("  idx_partkey created in {:.1}ms", idx_start.elapsed().as_secs_f64() * 1000.0);

        println!("\n=== Indexed: {} ===\n", config.label);
        let results = run_benchmark(&client, &queries, &config.label, scale, "indexed", runs).await?;
        all_results.extend(results);
    }

    Ok(all_results)
}

fn build_queries(is_snowdrop: bool) -> Vec<(&'static str, String)> {
    let prefix = if is_snowdrop { "tpch." } else { "" };

    let q6 = format!(
        "SELECT SUM(l_extendedprice * l_discount) AS revenue \
         FROM {}lineitem \
         WHERE l_shipdate >= '1994-01-01' AND l_shipdate < '1995-01-01' \
         AND l_discount >= 0.05 AND l_discount <= 0.07 \
         AND l_quantity < 24;",
        prefix
    );

    let q1 = format!(
        "SELECT l_returnflag, l_linestatus, \
         SUM(l_quantity) AS sum_qty, \
         SUM(l_extendedprice) AS sum_base_price, \
         SUM(l_extendedprice * (1 - l_discount)) AS sum_disc_price, \
         AVG(l_quantity) AS avg_qty, \
         AVG(l_extendedprice) AS avg_price, \
         AVG(l_discount) AS avg_disc, \
         COUNT(*) AS count_order \
         FROM {}lineitem \
         WHERE l_shipdate <= '1998-09-02' \
         GROUP BY l_returnflag, l_linestatus \
         ORDER BY l_returnflag, l_linestatus;",
        prefix
    );

    let q14 = format!(
        "SELECT SUM(CASE WHEN p_type LIKE 'PROMO%' THEN l_extendedprice * (1 - l_discount) ELSE 0 END) AS promo_revenue, \
         SUM(l_extendedprice * (1 - l_discount)) AS total_revenue \
         FROM {}lineitem l INNER JOIN {}part p ON l.l_partkey = p.p_partkey \
         WHERE l.l_shipdate >= '1995-01-01' AND l.l_shipdate < '1995-02-01';",
        prefix, prefix
    );

    let q3 = format!(
        "SELECT l.l_orderkey, SUM(l.l_extendedprice * (1 - l.l_discount)) AS revenue, o.o_orderdate, o.o_shippriority \
         FROM {}customer c \
         INNER JOIN {}orders o ON c.c_custkey = o.o_custkey \
         INNER JOIN {}lineitem l ON l.l_orderkey = o.o_orderkey \
         WHERE c.c_mktsegment = 'BUILDING' \
         AND o.o_orderdate < '1995-03-15' \
         AND l.l_shipdate > '1995-03-15' \
         GROUP BY l.l_orderkey, o.o_orderdate, o.o_shippriority \
         ORDER BY revenue DESC, o.o_orderdate \
         LIMIT 10;",
        prefix, prefix, prefix
    );

    vec![
        ("Q6", q6),
        ("Q1", q1),
        ("Q14", q14),
        ("Q3", q3),
    ]
}

async fn run_benchmark(
    client: &tokio_postgres::Client,
    queries: &[(&str, String)],
    system: &str,
    scale: usize,
    phase: &str,
    num_runs: u32,
) -> Result<Vec<QueryResult>, Box<dyn std::error::Error>> {
    let mut results = Vec::new();

    for (name, sql) in queries {
        println!("--- {} ---", name);
        let mut query_runs = Vec::new();

        for run in 1..=num_runs {
            let start = Instant::now();
            match client.simple_query(sql).await {
                Ok(msgs) => {
                    let elapsed = start.elapsed();
                    let row_count = msgs.iter()
                        .filter(|m| matches!(m, SimpleQueryMessage::Row(_)))
                        .count();

                    if run == 1 {
                        for msg in &msgs {
                            if let SimpleQueryMessage::Row(row) = msg {
                                let vals: Vec<String> = (0..row.len())
                                    .map(|i| row.get(i).unwrap_or("NULL").to_string())
                                    .collect();
                                println!("  {}", vals.join(" | "));
                            }
                        }
                    }

                    let latency_ms = elapsed.as_secs_f64() * 1000.0;
                    println!("  Run {}: {:.1}ms ({} rows)", run, latency_ms, row_count);
                    query_runs.push(RunResult { run, latency_ms, row_count });
                }
                Err(e) => {
                    eprintln!("  ERROR: {}", e);
                    break;
                }
            }
        }

        results.push(QueryResult {
            query: name.to_string(),
            system: system.to_string(),
            scale,
            phase: phase.to_string(),
            runs: query_runs,
        });
        println!();
    }

    Ok(results)
}

fn print_summary_table(results: &[QueryResult], scale: usize) {
    println!("\n===== Summary (scale={}) =====", scale);
    println!("{:<25} {:>12} {:>12} {:>12} {:>10}",
        "Query", "Snowdrop", "SD Indexed", "PostgreSQL", "vs PG");
    println!("{:-<25} {:-^12} {:-^12} {:-^12} {:-^10}", "", "", "", "", "");

    let query_names = ["Q6", "Q1", "Q14", "Q3"];
    for name in &query_names {
        let sd_baseline = warm_avg(results, name, "Snowdrop", "baseline");
        let sd_indexed = warm_avg(results, name, "Snowdrop", "indexed");
        let pg = warm_avg(results, name, "PostgreSQL 16", "baseline");

        let vs_pg = if pg > 0.0 && sd_baseline > 0.0 {
            let ratio = sd_baseline / pg;
            if ratio < 1.0 { format!("{:.1}x faster", 1.0 / ratio) }
            else { format!("{:.1}x slower", ratio) }
        } else {
            "-".to_string()
        };

        let sd_str = if sd_baseline > 0.0 { format!("{:.1}ms", sd_baseline) } else { "-".into() };
        let sdi_str = if sd_indexed > 0.0 { format!("{:.1}ms", sd_indexed) } else { "-".into() };
        let pg_str = if pg > 0.0 { format!("{:.1}ms", pg) } else { "-".into() };

        println!("{:<25} {:>12} {:>12} {:>12} {:>10}", name, sd_str, sdi_str, pg_str, vs_pg);
    }
}

fn warm_avg(results: &[QueryResult], query: &str, system: &str, phase: &str) -> f64 {
    results.iter()
        .find(|r| r.query == query && r.system == system && r.phase == phase)
        .map(|r| {
            // Skip first (cold) run, average the rest
            let warm: Vec<f64> = r.runs.iter().skip(1).map(|r| r.latency_ms).collect();
            if warm.is_empty() { 0.0 } else { warm.iter().sum::<f64>() / warm.len() as f64 }
        })
        .unwrap_or(0.0)
}

fn generate_report(results: &[QueryResult], scale: usize, with_indexes: bool) {
    let mut report = String::new();
    report.push_str("# Snowdrop Benchmark Report\n\n");
    report.push_str(&format!("Scale factor: {} (~{} lineitem rows)\n\n", scale, scale * ORDERS * LINEITEMS_PER_ORDER));

    report.push_str("## Architecture\n\n");
    report.push_str("Snowdrop is a SQL-compatible database PoC with:\n");
    report.push_str("- Embedded RocksDB for durable row storage\n");
    report.push_str("- In-memory columnar cache with SIMD-friendly bitmap filters\n");
    report.push_str("- RocksDB-native secondary indexes with lexicographic value encoding\n");
    report.push_str("- Neo4j for schema metadata only\n");
    report.push_str("- WAL for crash recovery\n");
    report.push_str("- PostgreSQL wire protocol (pgwire)\n\n");

    report.push_str("## Results\n\n");
    report.push_str("| Query | Snowdrop (warm) | SD Indexed (warm) | PostgreSQL 16 | vs PG (baseline) |\n");
    report.push_str("|---|---|---|---|---|\n");

    let query_names = ["Q6", "Q1", "Q14", "Q3"];
    for name in &query_names {
        let sd = warm_avg(results, name, "Snowdrop", "baseline");
        let sdi = warm_avg(results, name, "Snowdrop", "indexed");
        let pg = warm_avg(results, name, "PostgreSQL 16", "baseline");

        let vs = if pg > 0.0 && sd > 0.0 {
            let ratio = sd / pg;
            if ratio < 1.0 { format!("{:.1}x faster", 1.0 / ratio) }
            else { format!("{:.1}x slower", ratio) }
        } else { "-".into() };

        let sd_str = if sd > 0.0 { format!("{:.1}ms", sd) } else { "-".into() };
        let sdi_str = if sdi > 0.0 { format!("{:.1}ms", sdi) } else { "-".into() };
        let pg_str = if pg > 0.0 { format!("{:.1}ms", pg) } else { "-".into() };

        report.push_str(&format!("| {} | {} | {} | {} | {} |\n", name, sd_str, sdi_str, pg_str, vs));
    }

    report.push_str("\n## Query Descriptions\n\n");
    report.push_str("- **Q6**: Revenue from discounted shipments (SUM + 5-predicate WHERE)\n");
    report.push_str("- **Q1**: Pricing summary (GROUP BY + 7 aggregates)\n");
    report.push_str("- **Q14**: Promotion effect (2-table JOIN + CASE WHEN)\n");
    report.push_str("- **Q3**: Shipping priority (3-table JOIN + GROUP BY + ORDER BY + LIMIT)\n\n");

    if with_indexes {
        report.push_str("## Index Configuration\n\n");
        report.push_str("- `idx_shipdate` on `lineitem(l_shipdate)` -- benefits Q6 date range\n");
        report.push_str("- `idx_partkey` on `lineitem(l_partkey)` -- benefits Q14 join\n\n");
    }

    report.push_str("## Analysis\n\n");
    report.push_str("### Single-Table Scans (Q6, Q1)\n\n");
    report.push_str("The in-memory columnar cache with SIMD-friendly bitmap filters enables ");
    report.push_str("sub-millisecond filtered scans on warm cache. Column data is stored as ");
    report.push_str("contiguous typed arrays (Vec<i64>, Vec<f64>, etc.) enabling LLVM ");
    report.push_str("auto-vectorization of comparison loops.\n\n");

    report.push_str("### JOIN Queries (Q14, Q3)\n\n");
    report.push_str("JOINs remain the primary bottleneck. The current implementation uses ");
    report.push_str("nested-loop joins with row-oriented materialization. Hash join and ");
    report.push_str("columnar join paths are the main optimization opportunity.\n\n");

    report.push_str("### Secondary Indexes\n\n");
    report.push_str("RocksDB-native secondary indexes store entries as prefix-iterable keys ");
    report.push_str("(`idx:{db}:{tbl}:{col}:{value_encoded}:{row_id}`). Integer values use ");
    report.push_str("offset encoding for lexicographic sort order. For selective queries, index ");
    report.push_str("scan avoids full table scan. For broad scans, the columnar cache path ");
    report.push_str("is preferred.\n\n");

    report.push_str("## Metrics\n\n");
    report.push_str("Prometheus metrics are exposed on `:9090/metrics` including:\n");
    report.push_str("- `snowdrop_queries_total{type}` -- query count by type\n");
    report.push_str("- `snowdrop_query_duration_seconds{type}` -- latency histogram\n");
    report.push_str("- `snowdrop_index_scans_total` / `snowdrop_full_scans_total`\n");
    report.push_str("- `snowdrop_rows_scanned_total` / `snowdrop_rows_returned_total`\n");
    report.push_str("- `snowdrop_active_connections`\n\n");

    report.push_str("## Running\n\n");
    report.push_str("```bash\n");
    report.push_str("docker compose up -d\n");
    report.push_str("cargo run --release -p tpch-bench -- --scale 1 --with-indexes --output results.json\n");
    report.push_str("```\n\n");

    report.push_str("## Flamegraph\n\n");
    report.push_str("```bash\n");
    report.push_str("cargo install flamegraph\n");
    report.push_str("cargo flamegraph --bin tpch-bench -- --scale 1\n");
    report.push_str("```\n");

    let report_path = "docs/benchmark-report.md";
    std::fs::write(report_path, &report).unwrap_or_else(|e| eprintln!("Could not write report: {}", e));
    println!("\nBenchmark report written to {}", report_path);
}

async fn create_schema(
    client: &tokio_postgres::Client,
    is_snowdrop: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if is_snowdrop {
        let _ = client.simple_query("CREATE DATABASE tpch;").await;
        let _ = client.simple_query("DROP TABLE tpch.lineitem;").await;
        let _ = client.simple_query("DROP TABLE tpch.orders;").await;
        let _ = client.simple_query("DROP TABLE tpch.customer;").await;
        let _ = client.simple_query("DROP TABLE tpch.part;").await;

        client.simple_query(
            "CREATE TABLE tpch.customer (c_custkey BIGINT PRIMARY KEY, c_name VARCHAR(255), c_mktsegment VARCHAR(255));"
        ).await?;
        client.simple_query(
            "CREATE TABLE tpch.orders (o_orderkey BIGINT PRIMARY KEY, o_custkey BIGINT, o_orderstatus VARCHAR(1), o_totalprice FLOAT, o_orderdate VARCHAR(255), o_shippriority INT);"
        ).await?;
        client.simple_query(
            "CREATE TABLE tpch.lineitem (l_id BIGINT PRIMARY KEY, l_orderkey BIGINT, l_partkey BIGINT, l_quantity FLOAT, l_extendedprice FLOAT, l_discount FLOAT, l_tax FLOAT, l_returnflag VARCHAR(1), l_linestatus VARCHAR(1), l_shipdate VARCHAR(255));"
        ).await?;
        client.simple_query(
            "CREATE TABLE tpch.part (p_partkey BIGINT PRIMARY KEY, p_name VARCHAR(255), p_type VARCHAR(255), p_retailprice FLOAT);"
        ).await?;
    } else {
        let _ = client.simple_query("DROP TABLE IF EXISTS lineitem, orders, customer, part CASCADE;").await;

        client.simple_query(
            "CREATE TABLE customer (c_custkey BIGINT PRIMARY KEY, c_name VARCHAR(255), c_mktsegment VARCHAR(255));"
        ).await?;
        client.simple_query(
            "CREATE TABLE orders (o_orderkey BIGINT PRIMARY KEY, o_custkey BIGINT, o_orderstatus VARCHAR(1), o_totalprice FLOAT, o_orderdate VARCHAR(255), o_shippriority INT);"
        ).await?;
        client.simple_query(
            "CREATE TABLE lineitem (l_id BIGINT PRIMARY KEY, l_orderkey BIGINT, l_partkey BIGINT, l_quantity FLOAT, l_extendedprice FLOAT, l_discount FLOAT, l_tax FLOAT, l_returnflag VARCHAR(1), l_linestatus VARCHAR(1), l_shipdate VARCHAR(255));"
        ).await?;
        client.simple_query(
            "CREATE TABLE part (p_partkey BIGINT PRIMARY KEY, p_name VARCHAR(255), p_type VARCHAR(255), p_retailprice FLOAT);"
        ).await?;
    }
    Ok(())
}

async fn load_data(
    client: &tokio_postgres::Client,
    scale: usize,
    is_snowdrop: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut rng = rand::thread_rng();
    let prefix = if is_snowdrop { "tpch." } else { "" };
    let batch_size = 100;

    let num_customers = scale * CUSTOMERS;
    for batch_start in (1..=num_customers).step_by(batch_size) {
        let batch_end = (batch_start + batch_size - 1).min(num_customers);
        let vals: Vec<String> = (batch_start..=batch_end)
            .map(|i| {
                let seg = SEGMENTS[i % SEGMENTS.len()];
                format!("({}, 'Customer#{:06}', '{}')", i, i, seg)
            })
            .collect();
        let sql = format!("INSERT INTO {}customer (c_custkey, c_name, c_mktsegment) VALUES {};", prefix, vals.join(", "));
        client.simple_query(&sql).await?;
    }

    let num_parts = scale * PARTS;
    for batch_start in (1..=num_parts).step_by(batch_size) {
        let batch_end = (batch_start + batch_size - 1).min(num_parts);
        let vals: Vec<String> = (batch_start..=batch_end)
            .map(|i| {
                let ptype = PART_TYPES[i % PART_TYPES.len()];
                let price = 900.0 + (i as f64 * 0.01);
                format!("({}, 'Part#{:06}', '{}', {:.2})", i, i, ptype, price)
            })
            .collect();
        let sql = format!("INSERT INTO {}part (p_partkey, p_name, p_type, p_retailprice) VALUES {};", prefix, vals.join(", "));
        client.simple_query(&sql).await?;
    }

    let num_orders = scale * ORDERS;
    for batch_start in (1..=num_orders).step_by(batch_size) {
        let batch_end = (batch_start + batch_size - 1).min(num_orders);
        let vals: Vec<String> = (batch_start..=batch_end)
            .map(|i| {
                let custkey = (i % num_customers) + 1;
                let status = if i % 2 == 0 { "O" } else { "F" };
                let total = 1000.0 + (i as f64 * 1.5);
                let year = 1992 + (i % 7);
                let month = (i % 12) + 1;
                let day = (i % 28) + 1;
                let date = format!("{:04}-{:02}-{:02}", year, month, day);
                let priority = i % 5;
                format!("({}, {}, '{}', {:.2}, '{}', {})", i, custkey, status, total, date, priority)
            })
            .collect();
        let sql = format!("INSERT INTO {}orders (o_orderkey, o_custkey, o_orderstatus, o_totalprice, o_orderdate, o_shippriority) VALUES {};", prefix, vals.join(", "));
        client.simple_query(&sql).await?;
    }

    let mut li_id = 1u64;
    for order_id in 1..=num_orders {
        let num_items: usize = rng.gen_range(1..=LINEITEMS_PER_ORDER * 2);
        let mut batch_vals: Vec<String> = Vec::new();

        for _ in 0..num_items {
            let partkey = rng.gen_range(1..=num_parts);
            let qty: f64 = rng.gen_range(1.0..50.0);
            let price: f64 = rng.gen_range(900.0..100000.0);
            let discount: f64 = (rng.gen_range(0u32..11) as f64) / 100.0;
            let tax: f64 = (rng.gen_range(0u32..9) as f64) / 100.0;
            let rflag = RETURN_FLAGS[rng.gen_range(0..3)];
            let lstatus = LINE_STATUSES[rng.gen_range(0..2)];
            let year = 1992 + rng.gen_range(0..7);
            let month = rng.gen_range(1..=12);
            let day = rng.gen_range(1..=28);
            let shipdate = format!("{:04}-{:02}-{:02}", year, month, day);

            batch_vals.push(format!(
                "({}, {}, {}, {:.2}, {:.2}, {:.2}, {:.2}, '{}', '{}', '{}')",
                li_id, order_id, partkey, qty, price, discount, tax, rflag, lstatus, shipdate
            ));
            li_id += 1;

            if batch_vals.len() >= batch_size {
                let sql = format!(
                    "INSERT INTO {}lineitem (l_id, l_orderkey, l_partkey, l_quantity, l_extendedprice, l_discount, l_tax, l_returnflag, l_linestatus, l_shipdate) VALUES {};",
                    prefix, batch_vals.join(", ")
                );
                client.simple_query(&sql).await?;
                batch_vals.clear();
            }
        }

        if !batch_vals.is_empty() {
            let sql = format!(
                "INSERT INTO {}lineitem (l_id, l_orderkey, l_partkey, l_quantity, l_extendedprice, l_discount, l_tax, l_returnflag, l_linestatus, l_shipdate) VALUES {};",
                prefix, batch_vals.join(", ")
            );
            client.simple_query(&sql).await?;
        }
    }

    println!("Loaded {} lineitem rows", li_id - 1);
    Ok(())
}
