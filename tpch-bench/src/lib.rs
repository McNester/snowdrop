use anyhow::{anyhow, Result};
use rand::{rngs::StdRng, Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tokio_postgres::{NoTls, SimpleQueryMessage};

const SCALE: usize = 1;

const CUSTOMERS: usize = 150 * SCALE;
const ORDERS: usize = 1500 * SCALE;
const LINEITEMS_PER_ORDER: usize = 4;
const PARTS: usize = 200 * SCALE;

const SEGMENTS: [&str; 5] = [
    "AUTOMOBILE",
    "BUILDING",
    "FURNITURE",
    "MACHINERY",
    "HOUSEHOLD",
];
const RETURN_FLAGS: [&str; 3] = ["A", "N", "R"];
const LINE_STATUSES: [&str; 2] = ["F", "O"];
const PART_TYPES: [&str; 6] = [
    "PROMO BURNISHED COPPER",
    "STANDARD POLISHED TIN",
    "PROMO ANODIZED STEEL",
    "ECONOMY BRUSHED BRASS",
    "SMALL PLATED NICKEL",
    "MEDIUM BURNISHED IRON",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub query: String,
    pub system: String,
    pub scale: usize,
    pub phase: String,
    pub runs: Vec<RunResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub run: u32,
    pub latency_ms: f64,
    pub row_count: usize,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub dbname: String,
    pub label: String,
    pub password: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BenchmarkOptions {
    pub scale: usize,
    pub with_indexes: bool,
    pub output_path: Option<String>,
    pub snowdrop_host: String,
    pub pg_host: String,
    pub snowdrop_port: u16,
    pub pg_port: u16,
    pub runs: u32,
    pub seed: u64,
}

impl Default for BenchmarkOptions {
    fn default() -> Self {
        Self {
            scale: 1,
            with_indexes: false,
            output_path: None,
            snowdrop_host: "localhost".to_string(),
            pg_host: "localhost".to_string(),
            snowdrop_port: 5432,
            pg_port: 5433,
            runs: 3,
            seed: 42,
        }
    }
}

pub fn parse_args(args: &[String]) -> BenchmarkOptions {
    let mut options = BenchmarkOptions::default();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--scale" | "-s" => {
                i += 1;
                options.scale = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(1);
            }
            "--with-indexes" => {
                options.with_indexes = true;
            }
            "--output" | "-o" => {
                i += 1;
                options.output_path = args.get(i).cloned();
            }
            "--host" => {
                i += 1;
                options.snowdrop_host = args.get(i).cloned().unwrap_or_else(|| "localhost".into());
            }
            "--pg-host" => {
                i += 1;
                options.pg_host = args.get(i).cloned().unwrap_or_else(|| "localhost".into());
            }
            "--port" => {
                i += 1;
                options.snowdrop_port = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(5432);
            }
            "--pg-port" => {
                i += 1;
                options.pg_port = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(5433);
            }
            "--runs" | "-r" => {
                i += 1;
                options.runs = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(3);
            }
            "--seed" => {
                i += 1;
                options.seed = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(42);
            }
            other => {
                options.scale = other.parse().unwrap_or(1);
            }
        }
        i += 1;
    }
    options
}

pub async fn run_benchmark_suite(options: &BenchmarkOptions) -> Result<Vec<QueryResult>> {
    let total_lineitems = options.scale * ORDERS * LINEITEMS_PER_ORDER;
    println!(
        "TPC-H Benchmark (scale={}, ~{} lineitem rows, runs={}, seed={})",
        options.scale, total_lineitems, options.runs, options.seed
    );
    if options.with_indexes {
        println!("Index mode: will create indexes after baseline");
    }

    let snowdrop = Config {
        host: options.snowdrop_host.clone(),
        port: options.snowdrop_port,
        user: "snowdrop".into(),
        dbname: "snowdrop".into(),
        label: "Snowdrop".into(),
        password: None,
    };

    let postgres = Config {
        host: options.pg_host.clone(),
        port: options.pg_port,
        user: "postgres".into(),
        dbname: "tpch".into(),
        label: "PostgreSQL 16".into(),
        password: Some(require_postgres_password()?),
    };

    let mut all_results = Vec::new();

    println!("\n--- Loading data into {} ---", snowdrop.label);
    match run_full_benchmark(
        &snowdrop,
        options.scale,
        true,
        options.with_indexes,
        options.runs,
        options.seed,
    )
    .await
    {
        Ok(results) => all_results.extend(results),
        Err(e) => eprintln!("Snowdrop error: {}", e),
    }

    println!("\n--- Loading data into {} ---", postgres.label);
    match run_full_benchmark(
        &postgres,
        options.scale,
        false,
        false,
        options.runs,
        options.seed,
    )
    .await
    {
        Ok(results) => all_results.extend(results),
        Err(e) => eprintln!("PostgreSQL error: {}", e),
    }

    print_summary_table(&all_results, options.scale);
    Ok(all_results)
}

pub fn write_json_output(results: &[QueryResult], output_path: &str) -> Result<()> {
    let json = serde_json::to_string_pretty(results)?;
    std::fs::write(output_path, json)?;
    println!("\nResults saved to {}", output_path);
    Ok(())
}

pub fn tpch_markdown_section(results: &[QueryResult], scale: usize, with_indexes: bool) -> String {
    let mut section = String::new();
    section.push_str("## TPC-H Benchmark\n\n");
    section.push_str(&format!(
        "Scale factor: {} (~{} lineitem rows)\n\n",
        scale,
        scale * ORDERS * LINEITEMS_PER_ORDER
    ));
    section.push_str(
        "| Query | Snowdrop (warm) | SD Indexed (warm) | PostgreSQL 16 | vs PG (baseline) |\n",
    );
    section.push_str("|---|---|---|---|---|\n");

    for name in ["Q6", "Q1", "Q14", "Q3"] {
        let sd = warm_avg(results, name, "Snowdrop", "baseline");
        let sdi = warm_avg(results, name, "Snowdrop", "indexed");
        let pg = warm_avg(results, name, "PostgreSQL 16", "baseline");

        let vs = if pg > 0.0 && sd > 0.0 {
            let ratio = sd / pg;
            if ratio < 1.0 {
                format!("{:.1}x faster", 1.0 / ratio)
            } else {
                format!("{:.1}x slower", ratio)
            }
        } else {
            "-".into()
        };

        let sd_str = if sd > 0.0 {
            format!("{:.1}ms", sd)
        } else {
            "-".into()
        };
        let sdi_str = if sdi > 0.0 {
            format!("{:.1}ms", sdi)
        } else {
            "-".into()
        };
        let pg_str = if pg > 0.0 {
            format!("{:.1}ms", pg)
        } else {
            "-".into()
        };

        section.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            name, sd_str, sdi_str, pg_str, vs
        ));
    }

    section.push_str("\n### Query Descriptions\n\n");
    section.push_str("- **Q6**: Revenue from discounted shipments (SUM + 5-predicate WHERE)\n");
    section.push_str("- **Q1**: Pricing summary (GROUP BY + 7 aggregates)\n");
    section.push_str("- **Q14**: Promotion effect (2-table JOIN + CASE WHEN)\n");
    section
        .push_str("- **Q3**: Shipping priority (3-table JOIN + GROUP BY + ORDER BY + LIMIT)\n\n");

    if with_indexes {
        section.push_str("### Index Configuration\n\n");
        section.push_str("- `idx_shipdate` on `lineitem(l_shipdate)`\n");
        section.push_str("- `idx_partkey` on `lineitem(l_partkey)`\n\n");
    }

    section
}

async fn run_full_benchmark(
    config: &Config,
    scale: usize,
    is_snowdrop: bool,
    with_indexes: bool,
    runs: u32,
    seed: u64,
) -> Result<Vec<QueryResult>> {
    let mut pg_config = tokio_postgres::Config::new();
    pg_config
        .host(&config.host)
        .port(config.port)
        .user(&config.user)
        .dbname(&config.dbname);
    if let Some(password) = config.password.as_deref() {
        pg_config.password(password);
    }
    let (client, connection) = pg_config.connect(NoTls).await?;
    tokio::spawn(async move {
        let _ = connection.await;
    });

    create_schema(&client, is_snowdrop).await?;

    let load_start = Instant::now();
    load_data(&client, scale, is_snowdrop, seed).await?;
    println!("Load time: {:.1}s", load_start.elapsed().as_secs_f64());

    let mut all_results = Vec::new();
    println!("\n=== Baseline: {} ===\n", config.label);
    let queries = build_queries(is_snowdrop);
    let results = run_benchmark(&client, &queries, &config.label, scale, "baseline", runs).await?;
    all_results.extend(results);

    if with_indexes && is_snowdrop {
        println!("\n--- Creating indexes ---");
        let idx_start = Instant::now();
        client
            .simple_query("CREATE INDEX idx_shipdate ON tpch.lineitem(l_shipdate);")
            .await?;
        println!(
            "  idx_shipdate created in {:.1}ms",
            idx_start.elapsed().as_secs_f64() * 1000.0
        );

        let idx_start = Instant::now();
        client
            .simple_query("CREATE INDEX idx_partkey ON tpch.lineitem(l_partkey);")
            .await?;
        println!(
            "  idx_partkey created in {:.1}ms",
            idx_start.elapsed().as_secs_f64() * 1000.0
        );

        println!("\n=== Indexed: {} ===\n", config.label);
        let results =
            run_benchmark(&client, &queries, &config.label, scale, "indexed", runs).await?;
        all_results.extend(results);
    }

    Ok(all_results)
}

fn require_postgres_password() -> Result<String> {
    std::env::var("PGPASSWORD")
        .or_else(|_| std::env::var("POSTGRES_PASSWORD"))
        .map_err(|_| {
            anyhow!(
                "PostgreSQL password is required for benchmark comparison; set PGPASSWORD or POSTGRES_PASSWORD"
            )
        })
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

    vec![("Q6", q6), ("Q1", q1), ("Q14", q14), ("Q3", q3)]
}

async fn run_benchmark(
    client: &tokio_postgres::Client,
    queries: &[(&str, String)],
    system: &str,
    scale: usize,
    phase: &str,
    num_runs: u32,
) -> Result<Vec<QueryResult>> {
    let mut results = Vec::new();

    for (name, sql) in queries {
        println!("--- {} ({}) ---", name, tpch_query_description(name));
        let mut query_runs = Vec::new();

        for run in 1..=num_runs {
            let start = Instant::now();
            match client.simple_query(sql).await {
                Ok(msgs) => {
                    let elapsed = start.elapsed();
                    let row_count = msgs
                        .iter()
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
                    query_runs.push(RunResult {
                        run,
                        latency_ms,
                        row_count,
                    });
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

pub fn print_summary_table(results: &[QueryResult], scale: usize) {
    println!("\n===== Summary (scale={}) =====", scale);
    println!(
        "{:<44} {:>12} {:>12} {:>12} {:>10}",
        "Query", "Snowdrop", "SD Indexed", "PostgreSQL", "vs PG"
    );
    println!(
        "{:-<44} {:-^12} {:-^12} {:-^12} {:-^10}",
        "", "", "", "", ""
    );

    for name in ["Q6", "Q1", "Q14", "Q3"] {
        let sd_baseline = warm_avg(results, name, "Snowdrop", "baseline");
        let sd_indexed = warm_avg(results, name, "Snowdrop", "indexed");
        let pg = warm_avg(results, name, "PostgreSQL 16", "baseline");

        let vs_pg = if pg > 0.0 && sd_baseline > 0.0 {
            let ratio = sd_baseline / pg;
            if ratio < 1.0 {
                format!("{:.1}x faster", 1.0 / ratio)
            } else {
                format!("{:.1}x slower", ratio)
            }
        } else {
            "-".to_string()
        };

        let sd_str = if sd_baseline > 0.0 {
            format!("{:.1}ms", sd_baseline)
        } else {
            "-".into()
        };
        let sdi_str = if sd_indexed > 0.0 {
            format!("{:.1}ms", sd_indexed)
        } else {
            "-".into()
        };
        let pg_str = if pg > 0.0 {
            format!("{:.1}ms", pg)
        } else {
            "-".into()
        };

        println!(
            "{:<44} {:>12} {:>12} {:>12} {:>10}",
            format!("{} - {}", name, tpch_query_description(name)),
            sd_str,
            sdi_str,
            pg_str,
            vs_pg
        );
    }
}

pub fn tpch_query_description(name: &str) -> &'static str {
    match name {
        "Q6" => "Revenue from discounted shipments",
        "Q1" => "Pricing summary",
        "Q14" => "Promotion effect",
        "Q3" => "Shipping priority",
        _ => "Unknown query",
    }
}

pub fn warm_avg(results: &[QueryResult], query: &str, system: &str, phase: &str) -> f64 {
    results
        .iter()
        .find(|r| r.query == query && r.system == system && r.phase == phase)
        .map(|r| {
            let warm: Vec<f64> = r.runs.iter().skip(1).map(|r| r.latency_ms).collect();
            if warm.is_empty() {
                0.0
            } else {
                warm.iter().sum::<f64>() / warm.len() as f64
            }
        })
        .unwrap_or(0.0)
}

async fn create_schema(client: &tokio_postgres::Client, is_snowdrop: bool) -> Result<()> {
    if is_snowdrop {
        let _ = client.simple_query("CREATE DATABASE tpch;").await;
        let _ = client.simple_query("DROP TABLE tpch.lineitem;").await;
        let _ = client.simple_query("DROP TABLE tpch.orders;").await;
        let _ = client.simple_query("DROP TABLE tpch.customer;").await;
        let _ = client.simple_query("DROP TABLE tpch.part;").await;

        client
            .simple_query(
                "CREATE TABLE tpch.customer (c_custkey BIGINT PRIMARY KEY, c_name VARCHAR(255), c_mktsegment VARCHAR(255));",
            )
            .await?;
        client
            .simple_query(
                "CREATE TABLE tpch.orders (o_orderkey BIGINT PRIMARY KEY, o_custkey BIGINT, o_orderstatus VARCHAR(1), o_totalprice FLOAT, o_orderdate VARCHAR(255), o_shippriority INT);",
            )
            .await?;
        client
            .simple_query(
                "CREATE TABLE tpch.lineitem (l_id BIGINT PRIMARY KEY, l_orderkey BIGINT, l_partkey BIGINT, l_quantity FLOAT, l_extendedprice FLOAT, l_discount FLOAT, l_tax FLOAT, l_returnflag VARCHAR(1), l_linestatus VARCHAR(1), l_shipdate VARCHAR(255));",
            )
            .await?;
        client
            .simple_query(
                "CREATE TABLE tpch.part (p_partkey BIGINT PRIMARY KEY, p_name VARCHAR(255), p_type VARCHAR(255), p_retailprice FLOAT);",
            )
            .await?;
    } else {
        let _ = client
            .simple_query("DROP TABLE IF EXISTS lineitem, orders, customer, part CASCADE;")
            .await;

        client
            .simple_query(
                "CREATE TABLE customer (c_custkey BIGINT PRIMARY KEY, c_name VARCHAR(255), c_mktsegment VARCHAR(255));",
            )
            .await?;
        client
            .simple_query(
                "CREATE TABLE orders (o_orderkey BIGINT PRIMARY KEY, o_custkey BIGINT, o_orderstatus VARCHAR(1), o_totalprice FLOAT, o_orderdate VARCHAR(255), o_shippriority INT);",
            )
            .await?;
        client
            .simple_query(
                "CREATE TABLE lineitem (l_id BIGINT PRIMARY KEY, l_orderkey BIGINT, l_partkey BIGINT, l_quantity FLOAT, l_extendedprice FLOAT, l_discount FLOAT, l_tax FLOAT, l_returnflag VARCHAR(1), l_linestatus VARCHAR(1), l_shipdate VARCHAR(255));",
            )
            .await?;
        client
            .simple_query(
                "CREATE TABLE part (p_partkey BIGINT PRIMARY KEY, p_name VARCHAR(255), p_type VARCHAR(255), p_retailprice FLOAT);",
            )
            .await?;
    }
    Ok(())
}

async fn load_data(
    client: &tokio_postgres::Client,
    scale: usize,
    is_snowdrop: bool,
    seed: u64,
) -> Result<()> {
    let mut rng = StdRng::seed_from_u64(seed);
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
        let sql = format!(
            "INSERT INTO {}customer (c_custkey, c_name, c_mktsegment) VALUES {};",
            prefix,
            vals.join(", ")
        );
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
        let sql = format!(
            "INSERT INTO {}part (p_partkey, p_name, p_type, p_retailprice) VALUES {};",
            prefix,
            vals.join(", ")
        );
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
                format!(
                    "({}, {}, '{}', {:.2}, '{}', {})",
                    i, custkey, status, total, date, priority
                )
            })
            .collect();
        let sql = format!(
            "INSERT INTO {}orders (o_orderkey, o_custkey, o_orderstatus, o_totalprice, o_orderdate, o_shippriority) VALUES {};",
            prefix,
            vals.join(", ")
        );
        client.simple_query(&sql).await?;
    }

    let mut li_id = 1u64;
    for order_id in 1..=num_orders {
        let num_items: usize = rng.gen_range(1..=LINEITEMS_PER_ORDER * 2);
        let mut batch_vals = Vec::new();

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
                    prefix,
                    batch_vals.join(", ")
                );
                client.simple_query(&sql).await?;
                batch_vals.clear();
            }
        }

        if !batch_vals.is_empty() {
            let sql = format!(
                "INSERT INTO {}lineitem (l_id, l_orderkey, l_partkey, l_quantity, l_extendedprice, l_discount, l_tax, l_returnflag, l_linestatus, l_shipdate) VALUES {};",
                prefix,
                batch_vals.join(", ")
            );
            client.simple_query(&sql).await?;
        }
    }

    println!("Loaded {} lineitem rows", li_id - 1);
    Ok(())
}
