use anyhow::{anyhow, Result};
use chrono::Utc;
use rand::{rngs::StdRng, Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;
use tokio_postgres::{Client, NoTls, SimpleQueryMessage};

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
const DB_CONNECT_RETRIES: u32 = 60;
const DB_CONNECT_RETRY_DELAY: Duration = Duration::from_secs(1);

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
    pub warmup: bool,
    pub latency_ms: f64,
    pub row_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuerySummary {
    pub query: String,
    pub system: String,
    pub phase: String,
    pub measured_runs: usize,
    pub median_ms: f64,
    pub p90_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub avg_ms: f64,
    pub stddev_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkEnvironment {
    pub generated_at_unix_ms: u128,
    pub generated_at_rfc3339: String,
    pub git_branch: Option<String>,
    pub git_commit: Option<String>,
    pub os: String,
    pub arch: String,
    pub cpu_count: usize,
    pub containerized: bool,
    pub container_mode: String,
    pub docker_version: Option<String>,
    pub memory_limit_bytes: Option<u64>,
    pub snowdrop_endpoint: String,
    pub postgres_endpoint: String,
    pub scale: usize,
    pub seed: u64,
    pub measured_runs: u32,
    pub warmup_runs: u32,
    pub snowdrop_indexes: bool,
    pub postgres_indexes: bool,
    pub postgres_analyze: bool,
    pub load_batch_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkJsonReport {
    pub schema_version: u32,
    pub environment: BenchmarkEnvironment,
    pub summaries: Vec<QuerySummary>,
    pub results: Vec<QueryResult>,
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
    pub pg_indexes: bool,
    pub pg_analyze: bool,
    pub output_path: Option<String>,
    pub snowdrop_host: String,
    pub pg_host: String,
    pub snowdrop_port: u16,
    pub pg_port: u16,
    pub runs: u32,
    pub warmup_runs: u32,
    pub seed: u64,
    pub markdown_output_path: Option<String>,
    pub load_batch_size: usize,
}

impl Default for BenchmarkOptions {
    fn default() -> Self {
        Self {
            scale: 1,
            with_indexes: false,
            pg_indexes: false,
            pg_analyze: false,
            output_path: None,
            snowdrop_host: "localhost".to_string(),
            pg_host: "localhost".to_string(),
            snowdrop_port: 5432,
            pg_port: 5433,
            runs: 3,
            warmup_runs: 2,
            seed: 42,
            markdown_output_path: None,
            load_batch_size: 100,
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
            "--pg-indexes" => {
                options.pg_indexes = true;
            }
            "--pg-analyze" => {
                options.pg_analyze = true;
            }
            "--output" | "-o" => {
                i += 1;
                options.output_path = args.get(i).cloned();
            }
            "--markdown-output" => {
                i += 1;
                options.markdown_output_path = args.get(i).cloned();
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
            "--warmup-runs" => {
                i += 1;
                options.warmup_runs = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(2);
            }
            "--seed" => {
                i += 1;
                options.seed = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(42);
            }
            "--load-batch-size" => {
                i += 1;
                options.load_batch_size = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(100);
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
        "TPC-H Benchmark (scale={}, ~{} lineitem rows, warmup_runs={}, measured_runs={}, seed={})",
        options.scale, total_lineitems, options.warmup_runs, options.runs, options.seed
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
    let snowdrop_results = run_full_benchmark(
        &snowdrop,
        options.scale,
        true,
        options.with_indexes,
        options.runs,
        options.warmup_runs,
        options.seed,
        false,
        false,
        options.load_batch_size,
    )
    .await
    .map_err(|err| anyhow!("Snowdrop benchmark failed: {err}"))?;
    all_results.extend(snowdrop_results);

    println!("\n--- Loading data into {} ---", postgres.label);
    let postgres_results = run_full_benchmark(
        &postgres,
        options.scale,
        false,
        options.pg_indexes,
        options.runs,
        options.warmup_runs,
        options.seed,
        options.pg_indexes,
        options.pg_analyze,
        options.load_batch_size,
    )
    .await
    .map_err(|err| anyhow!("PostgreSQL benchmark failed: {err}"))?;
    all_results.extend(postgres_results);

    print_summary_table(&all_results, options.scale);
    Ok(all_results)
}

fn format_db_error(err: &tokio_postgres::Error) -> String {
    if let Some(db_err) = err.as_db_error() {
        return format!("{}: {}", db_err.severity(), db_err.message());
    }
    err.to_string()
}

pub fn write_json_output(
    results: &[QueryResult],
    options: &BenchmarkOptions,
    output_path: &str,
) -> Result<()> {
    let report = build_json_report(results, options);
    write_json_report(&report, output_path)
}

pub fn write_json_report(report: &BenchmarkJsonReport, output_path: &str) -> Result<()> {
    let json = serde_json::to_string_pretty(&report)?;
    std::fs::write(output_path, json)?;
    println!("\nResults saved to {}", output_path);
    Ok(())
}

pub fn build_json_report(
    results: &[QueryResult],
    options: &BenchmarkOptions,
) -> BenchmarkJsonReport {
    BenchmarkJsonReport {
        schema_version: 2,
        environment: benchmark_environment(options),
        summaries: summarize_query_results(results),
        results: results.to_vec(),
    }
}

pub fn read_benchmark_json_report(path: &str) -> Result<BenchmarkJsonReport> {
    let raw = std::fs::read_to_string(path)?;
    let report: BenchmarkJsonReport = serde_json::from_str(&raw)
        .map_err(|err| anyhow!("could not parse schema_version=2 benchmark JSON: {err}"))?;
    if report.schema_version != 2 {
        return Err(anyhow!(
            "unsupported benchmark JSON schema_version {}; expected 2",
            report.schema_version
        ));
    }
    Ok(report)
}

pub fn write_markdown_output(
    report: &BenchmarkJsonReport,
    output_path: &str,
    artifact_path: &str,
) -> Result<()> {
    std::fs::write(
        output_path,
        render_benchmark_results_markdown(report, artifact_path),
    )?;
    println!("Markdown benchmark report saved to {}", output_path);
    Ok(())
}

pub fn render_benchmark_results_markdown(
    report: &BenchmarkJsonReport,
    artifact_path: &str,
) -> String {
    let env = &report.environment;
    let memory_limit = env
        .memory_limit_bytes
        .map(format_bytes)
        .unwrap_or_else(|| "unlimited / unknown".to_string());
    let docker_version = env
        .docker_version
        .clone()
        .unwrap_or_else(|| "not detected".to_string());
    let git_branch = env
        .git_branch
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let git_commit = env
        .git_commit
        .clone()
        .unwrap_or_else(|| "unknown".to_string());

    let mut out = String::new();
    out.push_str("# Snowdrop Benchmark Results\n\n");
    out.push_str("This document is the compact benchmark snapshot for presentations, PR review, and thesis updates. It is generated from the latest `tpch-bench` JSON artifact so old historical numbers do not get mixed with the current run.\n\n");
    out.push_str("## Last Benchmark Run\n\n");
    out.push_str("| Field | Value |\n");
    out.push_str("|---|---|\n");
    out.push_str(&format!("| Timestamp | `{}` |\n", env.generated_at_rfc3339));
    out.push_str(&format!("| Git branch | `{}` |\n", git_branch));
    out.push_str(&format!("| Git commit | `{}` |\n", git_commit));
    out.push_str(&format!(
        "| Host OS / arch | `{}` / `{}` |\n",
        env.os, env.arch
    ));
    out.push_str(&format!("| CPU count | `{}` |\n", env.cpu_count));
    out.push_str(&format!("| Container mode | `{}` |\n", env.container_mode));
    out.push_str(&format!("| Docker version | `{}` |\n", docker_version));
    out.push_str(&format!("| Memory limit | `{}` |\n", memory_limit));
    out.push_str(&format!(
        "| Endpoints | Snowdrop `{}` / PostgreSQL `{}` |\n",
        env.snowdrop_endpoint, env.postgres_endpoint
    ));
    out.push_str(&format!(
        "| Scale / seed | `{}` / `{}` |\n",
        env.scale, env.seed
    ));
    out.push_str(&format!(
        "| Runs / warmup | `{}` measured / `{}` warmup |\n",
        env.measured_runs, env.warmup_runs
    ));
    out.push_str(&format!(
        "| Preparation | Snowdrop indexes `{}` / PostgreSQL indexes `{}` / PostgreSQL ANALYZE `{}` |\n",
        env.snowdrop_indexes, env.postgres_indexes, env.postgres_analyze
    ));
    out.push_str(&format!(
        "| Load batch size | `{}` rows per INSERT statement |\n",
        env.load_batch_size
    ));
    out.push_str(&format!("| Output artifact | `{}` |\n", artifact_path));

    out.push_str("\n## Current TPC-H Snapshot\n\n");
    out.push_str("Headline values are measured-run medians; warmup runs are excluded. Compare Snowdrop and PostgreSQL only inside the same Docker run because absolute milliseconds vary by host CPU, Docker backend, filesystem, and thermal state.\n\n");
    out.push_str("| Query | Snowdrop baseline median | Snowdrop indexed median | PostgreSQL median | Snowdrop result | p90 / stddev signal |\n");
    out.push_str("|---|---:|---:|---:|---|---|\n");
    for query in ["Q6", "Q1", "Q14", "Q3"] {
        let sd_baseline = summary(&report.summaries, query, "Snowdrop", "baseline");
        let sd_indexed = summary(&report.summaries, query, "Snowdrop", "indexed");
        let pg = summary(&report.summaries, query, "PostgreSQL 16", "baseline");
        let chosen_sd = best_summary(sd_baseline, sd_indexed);
        let result = match (chosen_sd, pg) {
            (Some(sd), Some(pg)) => snowdrop_result_label(sd.median_ms, pg.median_ms),
            _ => "-".to_string(),
        };
        let variance = match (chosen_sd, pg) {
            (Some(sd), Some(pg)) => format!(
                "SD p90 `{}` / stddev `{}`; PG p90 `{}` / stddev `{}`",
                format_ms(sd.p90_ms),
                format_ms(sd.stddev_ms),
                format_ms(pg.p90_ms),
                format_ms(pg.stddev_ms)
            ),
            (Some(sd), None) => format!(
                "SD p90 `{}` / stddev `{}`",
                format_ms(sd.p90_ms),
                format_ms(sd.stddev_ms)
            ),
            _ => "-".to_string(),
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            query,
            summary_median(sd_baseline),
            summary_median(sd_indexed),
            summary_median(pg),
            result,
            variance
        ));
    }

    out.push_str("\n## Query Meaning\n\n");
    out.push_str("- `Q6`: scan-heavy filtered aggregate; currently accelerated by a narrow TPC-H columnar fast path.\n");
    out.push_str("- `Q1`: grouped multi-aggregate over one large table; currently accelerated by a narrow TPC-H columnar fast path, with reusable aggregate planning still pending.\n");
    out.push_str("- `Q14`: two-table join with CASE/filter; currently accelerated by a narrow TPC-H fast path, with reusable hash join and pruning still pending.\n");
    out.push_str("- `Q3`: three-table join with GROUP BY/ORDER/LIMIT; currently accelerated by a narrow TPC-H fast path, with reusable join ordering still pending.\n\n");

    out.push_str("## Historical Snapshot\n\n");
    out.push_str("The old warm-cache snapshot below is retained as historical context only. New claims should use the generated `Last Benchmark Run` block above.\n\n");
    out.push_str("| Query | Snowdrop (cold) | Snowdrop (warm) | PostgreSQL 16 | Warm vs PG |\n");
    out.push_str("|---|---:|---:|---:|---|\n");
    out.push_str("| Q6 (SUM + date range) | 8.2ms | **0.8ms** | 1.5ms | **1.9x faster** |\n");
    out.push_str("| Q1 (GROUP BY + 6 aggregates) | 15.9ms | **10.9ms** | 4.6ms | 2.4x slower |\n");
    out.push_str("| Q14 (2-table JOIN + CASE) | 42.2ms | **26.1ms** | 1.5ms | 17x slower |\n");
    out.push_str("| Q3 (3-table JOIN + GROUP BY) | 62.6ms | **44.3ms** | 1.6ms | 28x slower |\n\n");

    out.push_str("## Interpretation\n\n");
    out.push_str("- Snowdrop performance should be described as workload-specific, not as a universal PostgreSQL replacement claim.\n");
    out.push_str("- The stable headline is the same-run ratio and median latency, not raw milliseconds copied between laptops.\n");
    out.push_str("- Current acceleration work should proceed in this order: stabilize migration/load, reduce write/DDL round-trips, then generalize the Q1/Q14/Q3 fast paths into reusable planner rules.\n\n");

    out.push_str("## Write/DDL Companion Result\n\n");
    out.push_str("The current write/DDL baseline is tracked separately because setup/load time can dominate benchmark usability before query execution starts. See [write-ddl-results.md](/e:/snowdrop-1/reports/benchmarks/write-ddl-results.md:1) for the latest `CREATE TABLE`, `ALTER TABLE`, single insert, row-by-row insert, and multi-values insert numbers.\n\n");
    out.push_str("## Refresh Command\n\n");
    out.push_str("Use the Docker-only command in [benchmarking.md](/e:/snowdrop-1/docs/benchmarking.md:1) and include `--markdown-output docs/benchmark-results.md`, or regenerate from an existing JSON artifact with `snowdrop-testkit benchmark-report`.\n");
    out
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
        "| Query | Snowdrop median | SD Indexed median | PostgreSQL median | vs PG (baseline) |\n",
    );
    section.push_str("|---|---|---|---|---|\n");

    for name in ["Q6", "Q1", "Q14", "Q3"] {
        let sd = measured_median(results, name, "Snowdrop", "baseline");
        let sdi = measured_median(results, name, "Snowdrop", "indexed");
        let pg = measured_median(results, name, "PostgreSQL 16", "baseline");

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

    section.push_str("### Reproducibility Notes\n\n");
    section.push_str("- Headline values use measured-run medians; warmup runs are excluded.\n");
    section.push_str("- Compare Snowdrop and PostgreSQL on the same machine/run; absolute milliseconds vary by Docker resources and host hardware.\n\n");

    section
}

async fn run_full_benchmark(
    config: &Config,
    scale: usize,
    is_snowdrop: bool,
    with_indexes: bool,
    runs: u32,
    warmup_runs: u32,
    seed: u64,
    pg_indexes: bool,
    pg_analyze: bool,
    load_batch_size: usize,
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
    let client = connect_with_retry(&pg_config, &config.label).await?;

    create_schema(&client, is_snowdrop).await?;

    let load_start = Instant::now();
    load_data(&client, scale, is_snowdrop, seed, load_batch_size).await?;
    println!("Load time: {:.1}s", load_start.elapsed().as_secs_f64());

    if !is_snowdrop {
        if pg_indexes {
            create_postgres_indexes(&client).await?;
        }
        if pg_analyze {
            println!("\n--- Running PostgreSQL ANALYZE ---");
            client.simple_query("ANALYZE;").await?;
        }
    }

    let mut all_results = Vec::new();
    println!("\n=== Baseline: {} ===\n", config.label);
    let queries = build_queries(is_snowdrop);
    let results = run_benchmark(
        &client,
        &queries,
        &config.label,
        scale,
        "baseline",
        warmup_runs,
        runs,
    )
    .await?;
    all_results.extend(results);

    if with_indexes && is_snowdrop {
        println!("\n--- Creating indexes ---");
        let idx_start = Instant::now();
        if let Err(err) = client
            .simple_query("CREATE INDEX idx_shipdate ON tpch.lineitem(l_shipdate);")
            .await
        {
            eprintln!("  Failed to create idx_shipdate: {}", format_db_error(&err));
            eprintln!(
                "  Continuing with baseline-only Snowdrop results; indexed phase is unavailable."
            );
            return Ok(all_results);
        }
        println!(
            "  idx_shipdate created in {:.1}ms",
            idx_start.elapsed().as_secs_f64() * 1000.0
        );

        let idx_start = Instant::now();
        if let Err(err) = client
            .simple_query("CREATE INDEX idx_partkey ON tpch.lineitem(l_partkey);")
            .await
        {
            eprintln!("  Failed to create idx_partkey: {}", format_db_error(&err));
            eprintln!(
                "  Continuing with baseline-only Snowdrop results; indexed phase is unavailable."
            );
            return Ok(all_results);
        }
        println!(
            "  idx_partkey created in {:.1}ms",
            idx_start.elapsed().as_secs_f64() * 1000.0
        );

        println!("\n=== Indexed: {} ===\n", config.label);
        let results = run_benchmark(
            &client,
            &queries,
            &config.label,
            scale,
            "indexed",
            warmup_runs,
            runs,
        )
        .await?;
        all_results.extend(results);
    }

    Ok(all_results)
}

async fn connect_with_retry(pg_config: &tokio_postgres::Config, label: &str) -> Result<Client> {
    let mut last_error = None;
    for attempt in 1..=DB_CONNECT_RETRIES {
        match pg_config.connect(NoTls).await {
            Ok((client, connection)) => {
                tokio::spawn(async move {
                    let _ = connection.await;
                });
                if attempt > 1 {
                    println!(
                        "{} accepted connections after {} attempt(s)",
                        label, attempt
                    );
                }
                return Ok(client);
            }
            Err(err) => {
                if attempt == 1 {
                    println!(
                        "Waiting for {} to accept PostgreSQL protocol connections...",
                        label
                    );
                }
                last_error = Some(err.to_string());
                sleep(DB_CONNECT_RETRY_DELAY).await;
            }
        }
    }

    Err(anyhow!(
        "could not connect to {} after {}s: {}",
        label,
        DB_CONNECT_RETRIES,
        last_error.unwrap_or_else(|| "unknown connection error".to_string())
    ))
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
    warmup_runs: u32,
    num_runs: u32,
) -> Result<Vec<QueryResult>> {
    let mut results = Vec::new();

    for (name, sql) in queries {
        println!("--- {} ({}) ---", name, tpch_query_description(name));
        let mut query_runs = Vec::new();

        let total_runs = warmup_runs + num_runs;
        for run in 1..=total_runs {
            let warmup = run <= warmup_runs;
            let start = Instant::now();
            match client.simple_query(sql).await {
                Ok(msgs) => {
                    let elapsed = start.elapsed();
                    let row_count = msgs
                        .iter()
                        .filter(|m| matches!(m, SimpleQueryMessage::Row(_)))
                        .count();

                    if run == warmup_runs + 1 {
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
                    let label = if warmup { "warmup" } else { "measured" };
                    println!(
                        "  Run {} [{}]: {:.1}ms ({} rows)",
                        run, label, latency_ms, row_count
                    );
                    query_runs.push(RunResult {
                        run,
                        warmup,
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
        let sd_baseline = measured_median(results, name, "Snowdrop", "baseline");
        let sd_indexed = measured_median(results, name, "Snowdrop", "indexed");
        let pg = measured_median(results, name, "PostgreSQL 16", "baseline");

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
            let measured: Vec<f64> = r
                .runs
                .iter()
                .filter(|r| !r.warmup)
                .map(|r| r.latency_ms)
                .collect();
            if measured.is_empty() {
                0.0
            } else {
                measured.iter().sum::<f64>() / measured.len() as f64
            }
        })
        .unwrap_or(0.0)
}

pub fn measured_median(results: &[QueryResult], query: &str, system: &str, phase: &str) -> f64 {
    results
        .iter()
        .find(|r| r.query == query && r.system == system && r.phase == phase)
        .map(|result| {
            let values = measured_latencies(result);
            percentile(&values, 0.5)
        })
        .unwrap_or(0.0)
}

pub fn summarize_query_results(results: &[QueryResult]) -> Vec<QuerySummary> {
    results
        .iter()
        .map(|result| {
            let values = measured_latencies(result);
            QuerySummary {
                query: result.query.clone(),
                system: result.system.clone(),
                phase: result.phase.clone(),
                measured_runs: values.len(),
                median_ms: percentile(&values, 0.5),
                p90_ms: percentile(&values, 0.9),
                min_ms: values.iter().copied().reduce(f64::min).unwrap_or(0.0),
                max_ms: values.iter().copied().reduce(f64::max).unwrap_or(0.0),
                avg_ms: average(&values),
                stddev_ms: stddev(&values),
            }
        })
        .collect()
}

fn measured_latencies(result: &QueryResult) -> Vec<f64> {
    result
        .runs
        .iter()
        .filter(|run| !run.warmup)
        .map(|run| run.latency_ms)
        .collect()
}

fn percentile(values: &[f64], quantile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((sorted.len() - 1) as f64 * quantile).round() as usize;
    sorted[idx]
}

fn average(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn stddev(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let avg = average(values);
    let variance = values
        .iter()
        .map(|value| {
            let diff = value - avg;
            diff * diff
        })
        .sum::<f64>()
        / values.len() as f64;
    variance.sqrt()
}

fn summary<'a>(
    summaries: &'a [QuerySummary],
    query: &str,
    system: &str,
    phase: &str,
) -> Option<&'a QuerySummary> {
    summaries
        .iter()
        .find(|item| item.query == query && item.system == system && item.phase == phase)
}

fn summary_median(summary: Option<&QuerySummary>) -> String {
    summary
        .map(|item| format!("`{}`", format_ms(item.median_ms)))
        .unwrap_or_else(|| "-".to_string())
}

fn best_summary<'a>(
    baseline: Option<&'a QuerySummary>,
    indexed: Option<&'a QuerySummary>,
) -> Option<&'a QuerySummary> {
    match (baseline, indexed) {
        (Some(base), Some(idx)) => {
            if idx.median_ms < base.median_ms {
                Some(idx)
            } else {
                Some(base)
            }
        }
        (Some(base), None) => Some(base),
        (None, Some(idx)) => Some(idx),
        (None, None) => None,
    }
}

fn snowdrop_result_label(snowdrop_ms: f64, postgres_ms: f64) -> String {
    if snowdrop_ms <= 0.0 || postgres_ms <= 0.0 {
        return "-".to_string();
    }
    let ratio = snowdrop_ms / postgres_ms;
    if ratio < 1.0 {
        format!("Snowdrop is `{:.2}x faster`", 1.0 / ratio)
    } else {
        format!("Snowdrop is `{:.2}x slower`", ratio)
    }
}

fn format_ms(value: f64) -> String {
    if value >= 100.0 {
        format!("{value:.0}ms")
    } else if value >= 10.0 {
        format!("{value:.1}ms")
    } else {
        format!("{value:.2}ms")
    }
}

fn format_bytes(value: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    if value as f64 >= GIB {
        format!("{:.1} GiB", value as f64 / GIB)
    } else if value as f64 >= MIB {
        format!("{:.1} MiB", value as f64 / MIB)
    } else {
        format!("{value} bytes")
    }
}

pub fn benchmark_environment(options: &BenchmarkOptions) -> BenchmarkEnvironment {
    let containerized = std::path::Path::new("/.dockerenv").exists();
    BenchmarkEnvironment {
        generated_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0),
        generated_at_rfc3339: Utc::now().to_rfc3339(),
        git_branch: command_output("git", &["rev-parse", "--abbrev-ref", "HEAD"]),
        git_commit: command_output("git", &["rev-parse", "--short", "HEAD"]),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        cpu_count: std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1),
        containerized,
        container_mode: if containerized { "docker" } else { "host" }.to_string(),
        docker_version: command_output("docker", &["--version"]),
        memory_limit_bytes: read_cgroup_memory_limit(),
        snowdrop_endpoint: format!("{}:{}", options.snowdrop_host, options.snowdrop_port),
        postgres_endpoint: format!("{}:{}", options.pg_host, options.pg_port),
        scale: options.scale,
        seed: options.seed,
        measured_runs: options.runs,
        warmup_runs: options.warmup_runs,
        snowdrop_indexes: options.with_indexes,
        postgres_indexes: options.pg_indexes,
        postgres_analyze: options.pg_analyze,
        load_batch_size: options.load_batch_size,
    }
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn read_cgroup_memory_limit() -> Option<u64> {
    for path in [
        "/sys/fs/cgroup/memory.max",
        "/sys/fs/cgroup/memory/memory.limit_in_bytes",
    ] {
        let Ok(raw) = std::fs::read_to_string(path) else {
            continue;
        };
        let trimmed = raw.trim();
        if trimmed == "max" {
            return None;
        }
        if let Ok(value) = trimmed.parse::<u64>() {
            return Some(value);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_accepts_markdown_output() {
        let args = vec![
            "tpch-bench".to_string(),
            "--output".to_string(),
            "reports/benchmarks/tpch-results.json".to_string(),
            "--markdown-output".to_string(),
            "docs/benchmark-results.md".to_string(),
        ];
        let options = parse_args(&args);
        assert_eq!(
            options.output_path.as_deref(),
            Some("reports/benchmarks/tpch-results.json")
        );
        assert_eq!(
            options.markdown_output_path.as_deref(),
            Some("docs/benchmark-results.md")
        );
    }

    #[test]
    fn render_markdown_labels_snowdrop_slower_or_faster() {
        let options = BenchmarkOptions::default();
        let results = vec![
            QueryResult {
                query: "Q6".into(),
                system: "Snowdrop".into(),
                scale: 1,
                phase: "indexed".into(),
                runs: vec![RunResult {
                    run: 1,
                    warmup: false,
                    latency_ms: 2.0,
                    row_count: 1,
                }],
            },
            QueryResult {
                query: "Q6".into(),
                system: "PostgreSQL 16".into(),
                scale: 1,
                phase: "baseline".into(),
                runs: vec![RunResult {
                    run: 1,
                    warmup: false,
                    latency_ms: 4.0,
                    row_count: 1,
                }],
            },
            QueryResult {
                query: "Q1".into(),
                system: "Snowdrop".into(),
                scale: 1,
                phase: "baseline".into(),
                runs: vec![RunResult {
                    run: 1,
                    warmup: false,
                    latency_ms: 12.0,
                    row_count: 1,
                }],
            },
            QueryResult {
                query: "Q1".into(),
                system: "PostgreSQL 16".into(),
                scale: 1,
                phase: "baseline".into(),
                runs: vec![RunResult {
                    run: 1,
                    warmup: false,
                    latency_ms: 3.0,
                    row_count: 1,
                }],
            },
        ];
        let report = build_json_report(&results, &options);
        let markdown = render_benchmark_results_markdown(&report, "reports/benchmarks/tpch.json");
        assert!(markdown.contains("Snowdrop is `2.00x faster`"));
        assert!(markdown.contains("Snowdrop is `4.00x slower`"));
        assert!(markdown.contains("Last Benchmark Run"));
    }
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

async fn create_postgres_indexes(client: &tokio_postgres::Client) -> Result<()> {
    println!("\n--- Creating PostgreSQL comparison indexes ---");
    for sql in [
        "CREATE INDEX IF NOT EXISTS idx_pg_lineitem_shipdate ON lineitem(l_shipdate);",
        "CREATE INDEX IF NOT EXISTS idx_pg_lineitem_partkey ON lineitem(l_partkey);",
        "CREATE INDEX IF NOT EXISTS idx_pg_lineitem_orderkey ON lineitem(l_orderkey);",
        "CREATE INDEX IF NOT EXISTS idx_pg_orders_custkey ON orders(o_custkey);",
        "CREATE INDEX IF NOT EXISTS idx_pg_orders_orderdate ON orders(o_orderdate);",
        "CREATE INDEX IF NOT EXISTS idx_pg_customer_segment ON customer(c_mktsegment);",
    ] {
        let start = Instant::now();
        client.simple_query(sql).await?;
        println!(
            "  {} ({:.1}ms)",
            sql.trim_end_matches(';'),
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
    Ok(())
}

async fn load_data(
    client: &tokio_postgres::Client,
    scale: usize,
    is_snowdrop: bool,
    seed: u64,
    batch_size: usize,
) -> Result<()> {
    let mut rng = StdRng::seed_from_u64(seed);
    let prefix = if is_snowdrop { "tpch." } else { "" };

    let num_customers = scale * CUSTOMERS;
    let stage_start = Instant::now();
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
    println!(
        "  loaded customer rows={} in {:.1}ms",
        num_customers,
        stage_start.elapsed().as_secs_f64() * 1000.0
    );

    let num_parts = scale * PARTS;
    let stage_start = Instant::now();
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
    println!(
        "  loaded part rows={} in {:.1}ms",
        num_parts,
        stage_start.elapsed().as_secs_f64() * 1000.0
    );

    let num_orders = scale * ORDERS;
    let stage_start = Instant::now();
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
    println!(
        "  loaded order rows={} in {:.1}ms",
        num_orders,
        stage_start.elapsed().as_secs_f64() * 1000.0
    );

    let mut li_id = 1u64;
    let stage_start = Instant::now();
    let mut batch_vals = Vec::new();
    for order_id in 1..=num_orders {
        let num_items: usize = rng.gen_range(1..=LINEITEMS_PER_ORDER * 2);

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
    }

    if !batch_vals.is_empty() {
        let sql = format!(
            "INSERT INTO {}lineitem (l_id, l_orderkey, l_partkey, l_quantity, l_extendedprice, l_discount, l_tax, l_returnflag, l_linestatus, l_shipdate) VALUES {};",
            prefix,
            batch_vals.join(", ")
        );
        client.simple_query(&sql).await?;
    }

    println!(
        "  loaded lineitem rows={} in {:.1}ms",
        li_id - 1,
        stage_start.elapsed().as_secs_f64() * 1000.0
    );
    println!("Loaded {} lineitem rows", li_id - 1);
    Ok(())
}
