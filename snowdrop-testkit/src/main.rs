use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_postgres::{Client, NoTls, SimpleQueryMessage};
use tpch_bench::{tpch_markdown_section, BenchmarkOptions, QueryResult};

const RAW_RESULTS_PATH: &str = "target/snowdrop-testkit/results.json";
const REPORT_PATH: &str = "docs/test-report.md";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeConfig {
    snowdrop_host: String,
    snowdrop_port: u16,
    postgres_host: String,
    postgres_port: u16,
    health_host: String,
    health_port: u16,
    metrics_host: String,
    metrics_port: u16,
}

impl RuntimeConfig {
    fn from_env() -> Self {
        Self {
            snowdrop_host: std::env::var("SNOWDROP_HOST").unwrap_or_else(|_| "localhost".into()),
            snowdrop_port: env_u16("SNOWDROP_PORT", 5432),
            postgres_host: std::env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".into()),
            postgres_port: env_u16("POSTGRES_PORT", 5433),
            health_host: std::env::var("HEALTH_HOST").unwrap_or_else(|_| "localhost".into()),
            health_port: env_u16("HEALTH_PORT", 8080),
            metrics_host: std::env::var("METRICS_HOST").unwrap_or_else(|_| "localhost".into()),
            metrics_port: env_u16("METRICS_PORT", 9090),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DatasetManifest {
    name: String,
    database: String,
    description: String,
    purpose: String,
    tables: Vec<String>,
    suites: Vec<String>,
    postgres_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GoldenCase {
    name: String,
    sql: String,
    expected_rows: Option<Vec<Vec<String>>>,
    expected_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DifferentialCase {
    name: String,
    sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TapSuiteFile {
    name: String,
    dataset: Option<String>,
    checks: Vec<TapCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum TapCheck {
    #[serde(rename = "health")]
    Health {
        name: String,
        path: String,
        contains: Option<String>,
    },
    #[serde(rename = "metrics")]
    Metrics {
        name: String,
        path: String,
        contains: Option<String>,
    },
    #[serde(rename = "sql")]
    Sql {
        name: String,
        sql: String,
        expected_rows: Option<Vec<Vec<String>>>,
        expected_error: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PostgresStyleGroupManifest {
    name: String,
    description: String,
    dataset: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SqlLogicFile {
    path: String,
    dataset: String,
    commands: Vec<SqlLogicCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum SqlLogicCommand {
    StatementOk { sql: String },
    StatementError { sql: String },
    Query { sql: String, expected_rows: Vec<Vec<String>> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UnifiedResults {
    generated_at: String,
    runtime: RuntimeConfig,
    datasets: Vec<DatasetManifest>,
    suite_runs: Vec<SuiteRun>,
    tpch_results: Vec<QueryResult>,
    tpch_options: Option<TpchRunMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TpchRunMeta {
    scale: usize,
    runs: u32,
    seed: u64,
    with_indexes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SuiteRun {
    suite: String,
    cases: Vec<CaseOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CaseOutcome {
    suite: String,
    dataset: Option<String>,
    group: String,
    case_name: String,
    status: String,
    details: String,
    expected: Option<String>,
    actual: Option<String>,
}

enum Command {
    ListDatasets,
    Load { dataset: String },
    Run { dataset: Option<String>, suite: String },
    RunAll { with_tpch: bool },
    Report,
}

#[tokio::main]
async fn main() {
    if let Err(err) = async_main().await {
        eprintln!("snowdrop-testkit error: {err:#}");
        std::process::exit(1);
    }
}

async fn async_main() -> Result<()> {
    let runtime = RuntimeConfig::from_env();
    let command = parse_command(&std::env::args().collect::<Vec<_>>())?;
    let repo_root = std::env::current_dir().context("could not resolve current directory")?;

    match command {
        Command::ListDatasets => {
            for dataset in load_dataset_manifests(&repo_root)? {
                println!(
                    "{} | db={} | postgres={} | suites={}",
                    dataset.name,
                    dataset.database,
                    dataset.postgres_enabled,
                    dataset.suites.join(",")
                );
                println!("  {}", dataset.description);
            }
        }
        Command::Load { dataset } => {
            let manifest = dataset_by_name(&repo_root, &dataset)?;
            load_dataset(&runtime, &manifest, manifest.postgres_enabled).await?;
            println!("Loaded dataset {}", manifest.name);
        }
        Command::Run { dataset, suite } => {
            let datasets = load_dataset_manifests(&repo_root)?;
            let run = run_named_suite(&repo_root, &runtime, &suite, dataset.as_deref(), &datasets).await?;
            let results = UnifiedResults {
                generated_at: Utc::now().to_rfc3339(),
                runtime,
                datasets,
                suite_runs: vec![run],
                tpch_results: Vec::new(),
                tpch_options: None,
            };
            write_results_artifacts(&repo_root, &results)?;
        }
        Command::RunAll { with_tpch } => {
            let datasets = load_dataset_manifests(&repo_root)?;
            let mut suite_runs = Vec::new();
            for suite in ["golden", "differential", "sqllogictest", "tap", "postgres-style"] {
                suite_runs.push(run_named_suite(&repo_root, &runtime, suite, None, &datasets).await?);
            }

            let (tpch_results, tpch_options) = if with_tpch {
                let options = BenchmarkOptions {
                    scale: 1,
                    with_indexes: true,
                    output_path: None,
                    snowdrop_host: runtime.snowdrop_host.clone(),
                    pg_host: runtime.postgres_host.clone(),
                    snowdrop_port: runtime.snowdrop_port,
                    pg_port: runtime.postgres_port,
                    runs: 3,
                    seed: 42,
                };
                let results = tpch_bench::run_benchmark_suite(&options).await?;
                let meta = TpchRunMeta {
                    scale: options.scale,
                    runs: options.runs,
                    seed: options.seed,
                    with_indexes: options.with_indexes,
                };
                (results, Some(meta))
            } else {
                (Vec::new(), None)
            };

            let results = UnifiedResults {
                generated_at: Utc::now().to_rfc3339(),
                runtime,
                datasets,
                suite_runs,
                tpch_results,
                tpch_options,
            };
            write_results_artifacts(&repo_root, &results)?;
        }
        Command::Report => {
            let results = read_results(&repo_root)?;
            let report = render_report(&results);
            fs::write(repo_root.join(REPORT_PATH), report)?;
            println!("Wrote {}", REPORT_PATH);
        }
    }

    Ok(())
}

fn parse_command(args: &[String]) -> Result<Command> {
    match args.get(1).map(String::as_str) {
        Some("list-datasets") => Ok(Command::ListDatasets),
        Some("load") => {
            let dataset = read_flag(args, "--dataset").ok_or_else(|| anyhow!("missing --dataset"))?;
            Ok(Command::Load { dataset })
        }
        Some("run") => {
            let suite = read_flag(args, "--suite").ok_or_else(|| anyhow!("missing --suite"))?;
            let dataset = read_flag(args, "--dataset");
            Ok(Command::Run { dataset, suite })
        }
        Some("run-all") => Ok(Command::RunAll {
            with_tpch: args.iter().any(|arg| arg == "--with-tpch"),
        }),
        Some("report") => Ok(Command::Report),
        _ => Err(anyhow!(
            "usage: snowdrop-testkit <list-datasets|load|run|run-all|report> [flags]"
        )),
    }
}

fn read_flag(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|idx| args.get(idx + 1))
        .cloned()
}

fn env_u16(name: &str, default: u16) -> u16 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn tests_root(repo_root: &Path) -> PathBuf {
    repo_root.join("tests")
}

fn load_dataset_manifests(repo_root: &Path) -> Result<Vec<DatasetManifest>> {
    let mut manifests = Vec::new();
    for entry in fs::read_dir(tests_root(repo_root).join("datasets"))? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let manifest_path = entry.path().join("manifest.json");
        let manifest: DatasetManifest = serde_json::from_str(&fs::read_to_string(&manifest_path)?)?;
        manifests.push(manifest);
    }
    manifests.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(manifests)
}

fn dataset_by_name(repo_root: &Path, name: &str) -> Result<DatasetManifest> {
    load_dataset_manifests(repo_root)?
        .into_iter()
        .find(|dataset| dataset.name == name)
        .ok_or_else(|| anyhow!("unknown dataset {}", name))
}

async fn run_named_suite(
    repo_root: &Path,
    runtime: &RuntimeConfig,
    suite: &str,
    dataset_filter: Option<&str>,
    datasets: &[DatasetManifest],
) -> Result<SuiteRun> {
    match normalize_suite_name(suite) {
        "golden" => run_golden_suite(repo_root, runtime, dataset_filter, datasets).await,
        "differential" => run_differential_suite(repo_root, runtime, dataset_filter, datasets).await,
        "sqllogictest" => run_sqllogictest_suite(repo_root, runtime, dataset_filter, datasets).await,
        "tap" => run_tap_suite(repo_root, runtime, dataset_filter, datasets).await,
        "postgres-style" => run_postgres_style_suite(repo_root, runtime, dataset_filter, datasets).await,
        other => Err(anyhow!("unknown suite {}", other)),
    }
}

fn normalize_suite_name(suite: &str) -> &str {
    match suite {
        "postgres_style" => "postgres-style",
        other => other,
    }
}

async fn run_golden_suite(
    repo_root: &Path,
    runtime: &RuntimeConfig,
    dataset_filter: Option<&str>,
    datasets: &[DatasetManifest],
) -> Result<SuiteRun> {
    let root = tests_root(repo_root).join("golden");
    let mut cases = Vec::new();

    for dataset in datasets.iter().filter(|d| dataset_filter.map(|f| f == d.name).unwrap_or(true)) {
        let dataset_dir = root.join(&dataset.name);
        if !dataset_dir.exists() {
            continue;
        }

        load_dataset(runtime, dataset, false).await?;
        let client = open_snowdrop_dataset(runtime, &dataset.database).await?;
        for file in sorted_files(&dataset_dir, "json")? {
            let case: GoldenCase = serde_json::from_str(&fs::read_to_string(&file)?)?;
            let query = execute_query(&client, &case.sql).await;
            let outcome = match query {
                Ok(rows) => {
                    if let Some(expected) = case.expected_rows.clone() {
                        let expected_rows = normalize_rows(expected);
                        if expected_rows == rows {
                            pass("golden", &dataset.name, file_stem(&file), &case.name, "matched expected rows")
                        } else {
                            mismatch(
                                "golden",
                                &dataset.name,
                                file_stem(&file),
                                &case.name,
                                rows_to_text(&expected_rows),
                                rows_to_text(&rows),
                            )
                        }
                    } else {
                        fail(
                            "golden",
                            &dataset.name,
                            file_stem(&file),
                            &case.name,
                            "query succeeded but expected an error".into(),
                        )
                    }
                }
                Err(err) => {
                    if let Some(expected_error) = case.expected_error.clone() {
                        if expected_error == "*" || err.contains(&expected_error) {
                            pass("golden", &dataset.name, file_stem(&file), &case.name, "matched expected error")
                        } else {
                            mismatch(
                                "golden",
                                &dataset.name,
                                file_stem(&file),
                                &case.name,
                                expected_error,
                                err,
                            )
                        }
                    } else {
                        fail(
                            "golden",
                            &dataset.name,
                            file_stem(&file),
                            &case.name,
                            err,
                        )
                    }
                }
            };
            cases.push(outcome);
        }
    }

    Ok(SuiteRun {
        suite: "golden".into(),
        cases,
    })
}

async fn run_differential_suite(
    repo_root: &Path,
    runtime: &RuntimeConfig,
    dataset_filter: Option<&str>,
    datasets: &[DatasetManifest],
) -> Result<SuiteRun> {
    let root = tests_root(repo_root).join("differential");
    let mut cases = Vec::new();

    for dataset in datasets.iter().filter(|d| dataset_filter.map(|f| f == d.name).unwrap_or(true)) {
        let dataset_dir = root.join(&dataset.name);
        if !dataset_dir.exists() {
            continue;
        }

        if !dataset.postgres_enabled {
            cases.push(skip_case(
                "differential",
                &dataset.name,
                "dataset",
                "dataset",
                "dataset is not enabled for PostgreSQL differential runs",
            ));
            continue;
        }

        load_dataset(runtime, dataset, true).await?;
        let snowdrop = open_snowdrop_dataset(runtime, &dataset.database).await?;
        let postgres = open_postgres_dataset(runtime, &dataset.database).await?;

        for file in sorted_files(&dataset_dir, "json")? {
            let case: DifferentialCase = serde_json::from_str(&fs::read_to_string(&file)?)?;
            let pg = execute_query(&postgres, &case.sql).await;
            let sd = execute_query(&snowdrop, &case.sql).await;
            let outcome = match (pg, sd) {
                (Ok(pg_rows), Ok(sd_rows)) => {
                    if pg_rows == sd_rows {
                        pass(
                            "differential",
                            &dataset.name,
                            file_stem(&file),
                            &case.name,
                            "Snowdrop matched PostgreSQL",
                        )
                    } else {
                        mismatch(
                            "differential",
                            &dataset.name,
                            file_stem(&file),
                            &case.name,
                            rows_to_text(&pg_rows),
                            rows_to_text(&sd_rows),
                        )
                    }
                }
                (Ok(_), Err(sd_err)) if looks_unsupported(&sd_err) => skip_case(
                    "differential",
                    &dataset.name,
                    file_stem(&file),
                    &case.name,
                    sd_err,
                ),
                (Ok(_), Err(sd_err)) => fail(
                    "differential",
                    &dataset.name,
                    file_stem(&file),
                    &case.name,
                    format!("Snowdrop error: {}", sd_err),
                ),
                (Err(pg_err), Ok(_)) => fail(
                    "differential",
                    &dataset.name,
                    file_stem(&file),
                    &case.name,
                    format!("PostgreSQL error: {}", pg_err),
                ),
                (Err(pg_err), Err(sd_err)) => fail(
                    "differential",
                    &dataset.name,
                    file_stem(&file),
                    &case.name,
                    format!("Both systems errored. pg={}, snowdrop={}", pg_err, sd_err),
                ),
            };
            cases.push(outcome);
        }
    }

    Ok(SuiteRun {
        suite: "differential".into(),
        cases,
    })
}

async fn run_sqllogictest_suite(
    repo_root: &Path,
    runtime: &RuntimeConfig,
    dataset_filter: Option<&str>,
    datasets: &[DatasetManifest],
) -> Result<SuiteRun> {
    let root = tests_root(repo_root).join("sqllogictest");
    let mut cases = Vec::new();

    for file in sorted_files(&root, "test")? {
        let parsed = parse_sqllogictest_file(&file)?;
        if dataset_filter.map(|f| f != parsed.dataset).unwrap_or(false) {
            continue;
        }

        let dataset = datasets
            .iter()
            .find(|d| d.name == parsed.dataset)
            .ok_or_else(|| anyhow!("unknown dataset {} in {}", parsed.dataset, parsed.path))?;
        load_dataset(runtime, dataset, false).await?;
        let client = open_snowdrop_dataset(runtime, &dataset.database).await?;

        for (idx, command) in parsed.commands.iter().enumerate() {
            let name = format!("command_{}", idx + 1);
            let outcome = match command {
                SqlLogicCommand::StatementOk { sql } => match execute_query(&client, sql).await {
                    Ok(_) => pass("sqllogictest", &dataset.name, &parsed.path, &name, "statement ok"),
                    Err(err) => fail("sqllogictest", &dataset.name, &parsed.path, &name, err),
                },
                SqlLogicCommand::StatementError { sql } => match execute_query(&client, sql).await {
                    Ok(_) => fail(
                        "sqllogictest",
                        &dataset.name,
                        &parsed.path,
                        &name,
                        "statement succeeded but expected error".into(),
                    ),
                    Err(_) => pass("sqllogictest", &dataset.name, &parsed.path, &name, "statement error"),
                },
                SqlLogicCommand::Query { sql, expected_rows } => match execute_query(&client, sql).await {
                    Ok(rows) => {
                        let expected = normalize_rows(expected_rows.clone());
                        if expected == rows {
                            pass("sqllogictest", &dataset.name, &parsed.path, &name, "query matched")
                        } else {
                            mismatch(
                                "sqllogictest",
                                &dataset.name,
                                &parsed.path,
                                &name,
                                rows_to_text(&expected),
                                rows_to_text(&rows),
                            )
                        }
                    }
                    Err(err) => fail("sqllogictest", &dataset.name, &parsed.path, &name, err),
                },
            };
            cases.push(outcome);
        }
    }

    Ok(SuiteRun {
        suite: "sqllogictest".into(),
        cases,
    })
}

async fn run_tap_suite(
    repo_root: &Path,
    runtime: &RuntimeConfig,
    dataset_filter: Option<&str>,
    datasets: &[DatasetManifest],
) -> Result<SuiteRun> {
    let root = tests_root(repo_root).join("tap");
    let tap_out_root = repo_root.join("target/snowdrop-testkit/tap");
    fs::create_dir_all(&tap_out_root)?;
    let mut cases = Vec::new();

    for file in sorted_files(&root, "json")? {
        let suite: TapSuiteFile = serde_json::from_str(&fs::read_to_string(&file)?)?;
        if dataset_filter
            .map(|f| suite.dataset.as_deref().map(|d| d != f).unwrap_or(true))
            .unwrap_or(false)
        {
            continue;
        }

        let mut tap_lines = Vec::new();
        tap_lines.push(format!("1..{}", suite.checks.len()));

        let client = if let Some(dataset_name) = suite.dataset.as_deref() {
            let dataset = datasets
                .iter()
                .find(|d| d.name == dataset_name)
                .ok_or_else(|| anyhow!("unknown dataset {}", dataset_name))?;
            load_dataset(runtime, dataset, false).await?;
            Some(open_snowdrop_dataset(runtime, &dataset.database).await?)
        } else {
            None
        };

        for (idx, check) in suite.checks.iter().enumerate() {
            let number = idx + 1;
            let outcome = match check {
                TapCheck::Health { name, path, contains } => {
                    let body = http_get_text(&runtime.health_host, runtime.health_port, path).await?;
                    if contains.as_ref().map(|c| body.contains(c)).unwrap_or(true) {
                        tap_lines.push(format!("ok {} - {}", number, name));
                        pass("tap", suite.dataset.as_deref().unwrap_or("runtime"), &suite.name, name, "health ok")
                    } else {
                        tap_lines.push(format!("not ok {} - {}", number, name));
                        fail("tap", suite.dataset.as_deref().unwrap_or("runtime"), &suite.name, name, body)
                    }
                }
                TapCheck::Metrics { name, path, contains } => {
                    let body = http_get_text(&runtime.metrics_host, runtime.metrics_port, path).await?;
                    if contains.as_ref().map(|c| body.contains(c)).unwrap_or(true) {
                        tap_lines.push(format!("ok {} - {}", number, name));
                        pass("tap", suite.dataset.as_deref().unwrap_or("runtime"), &suite.name, name, "metrics ok")
                    } else {
                        tap_lines.push(format!("not ok {} - {}", number, name));
                        fail("tap", suite.dataset.as_deref().unwrap_or("runtime"), &suite.name, name, body)
                    }
                }
                TapCheck::Sql {
                    name,
                    sql,
                    expected_rows,
                    expected_error,
                } => {
                    let client = client
                        .as_ref()
                        .ok_or_else(|| anyhow!("tap SQL checks require a dataset"))?;
                    match execute_query(client, sql).await {
                        Ok(rows) => {
                            if let Some(expected) = expected_rows {
                                let expected = normalize_rows(expected.clone());
                                if expected == rows {
                                    tap_lines.push(format!("ok {} - {}", number, name));
                                    pass("tap", suite.dataset.as_deref().unwrap_or("runtime"), &suite.name, name, "sql matched")
                                } else {
                                    tap_lines.push(format!("not ok {} - {}", number, name));
                                    mismatch(
                                        "tap",
                                        suite.dataset.as_deref().unwrap_or("runtime"),
                                        &suite.name,
                                        name,
                                        rows_to_text(&expected),
                                        rows_to_text(&rows),
                                    )
                                }
                            } else if expected_error.is_some() {
                                tap_lines.push(format!("not ok {} - {}", number, name));
                                fail(
                                    "tap",
                                    suite.dataset.as_deref().unwrap_or("runtime"),
                                    &suite.name,
                                    name,
                                    "query succeeded but expected error".into(),
                                )
                            } else {
                                tap_lines.push(format!("ok {} - {}", number, name));
                                pass("tap", suite.dataset.as_deref().unwrap_or("runtime"), &suite.name, name, "sql ok")
                            }
                        }
                        Err(err) => {
                            if expected_error.as_deref() == Some("*")
                                || expected_error
                                    .as_ref()
                                    .map(|needle| err.contains(needle))
                                    .unwrap_or(false)
                            {
                                tap_lines.push(format!("ok {} - {}", number, name));
                                pass("tap", suite.dataset.as_deref().unwrap_or("runtime"), &suite.name, name, "expected error")
                            } else {
                                tap_lines.push(format!("not ok {} - {}", number, name));
                                fail("tap", suite.dataset.as_deref().unwrap_or("runtime"), &suite.name, name, err)
                            }
                        }
                    }
                }
            };
            cases.push(outcome);
        }

        fs::write(tap_out_root.join(format!("{}.tap", suite.name)), tap_lines.join("\n"))?;
    }

    Ok(SuiteRun {
        suite: "tap".into(),
        cases,
    })
}

async fn run_postgres_style_suite(
    repo_root: &Path,
    runtime: &RuntimeConfig,
    dataset_filter: Option<&str>,
    datasets: &[DatasetManifest],
) -> Result<SuiteRun> {
    let root = tests_root(repo_root).join("postgres_style");
    let mut cases = Vec::new();

    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let group_root = entry.path();
        let manifest: PostgresStyleGroupManifest =
            serde_json::from_str(&fs::read_to_string(group_root.join("manifest.json"))?)?;
        if dataset_filter.map(|f| f != manifest.dataset).unwrap_or(false) {
            continue;
        }

        let dataset = datasets
            .iter()
            .find(|d| d.name == manifest.dataset)
            .ok_or_else(|| anyhow!("unknown dataset {}", manifest.dataset))?;
        load_dataset(runtime, dataset, false).await?;
        let client = open_snowdrop_dataset(runtime, &dataset.database).await?;

        for sql_file in sorted_files(&group_root.join("sql"), "sql")? {
            let base = file_stem(&sql_file);
            let expected_file = group_root.join("expected").join(format!("{}.out", base));
            let sql = fs::read_to_string(&sql_file)?;
            let expected = fs::read_to_string(expected_file)?.trim().to_string();
            let outcome = match execute_query(&client, &sql).await {
                Ok(rows) => {
                    if expected.starts_with("ERROR:") {
                        fail(
                            "postgres-style",
                            &dataset.name,
                            &manifest.name,
                            &base,
                            "query succeeded but expected error".into(),
                        )
                    } else {
                        let expected_rows = parse_expected_rows(&expected);
                        if expected_rows == rows {
                            pass("postgres-style", &dataset.name, &manifest.name, &base, "matched expected output")
                        } else {
                            mismatch(
                                "postgres-style",
                                &dataset.name,
                                &manifest.name,
                                &base,
                                rows_to_text(&expected_rows),
                                rows_to_text(&rows),
                            )
                        }
                    }
                }
                Err(err) => {
                    if expected == "ERROR:" || expected.strip_prefix("ERROR:").map(|suffix| err.contains(suffix.trim())).unwrap_or(false) {
                        pass("postgres-style", &dataset.name, &manifest.name, &base, "matched expected error")
                    } else {
                        fail("postgres-style", &dataset.name, &manifest.name, &base, err)
                    }
                }
            };
            cases.push(outcome);
        }
    }

    Ok(SuiteRun {
        suite: "postgres-style".into(),
        cases,
    })
}

async fn load_dataset(runtime: &RuntimeConfig, dataset: &DatasetManifest, load_postgres: bool) -> Result<()> {
    let dataset_dir = tests_root(Path::new(".")).join("datasets").join(&dataset.name);
    let schema = fs::read_to_string(dataset_dir.join("schema.sql"))?;
    let seed = fs::read_to_string(dataset_dir.join("seed.sql"))?;

    let snowdrop = open_snowdrop_admin(runtime).await?;
    // Recreate the Snowdrop database from scratch so suites that create
    // auxiliary tables (for example sqllogictest scratch tables) do not leak
    // state into later runs.
    let _ = snowdrop
        .simple_query(&format!("DROP DATABASE {};", dataset.database))
        .await;
    snowdrop
        .simple_query(&format!("CREATE DATABASE {};", dataset.database))
        .await?;
    snowdrop.simple_query(&format!("USE {};", dataset.database)).await?;
    snowdrop.simple_query(&schema).await?;
    snowdrop.simple_query(&seed).await?;

    if load_postgres && dataset.postgres_enabled {
        let admin = open_postgres_admin(runtime).await?;
        admin
            .simple_query(&format!("DROP DATABASE IF EXISTS {};", dataset.database))
            .await?;
        admin
            .simple_query(&format!("CREATE DATABASE {};", dataset.database))
            .await?;
        let pg = open_postgres_dataset(runtime, &dataset.database).await?;
        pg.simple_query(&schema).await?;
        pg.simple_query(&seed).await?;
    }

    Ok(())
}

async fn open_snowdrop_admin(runtime: &RuntimeConfig) -> Result<Client> {
    connect_client(
        &runtime.snowdrop_host,
        runtime.snowdrop_port,
        "snowdrop",
        "snowdrop",
        None,
    )
    .await
}

async fn open_snowdrop_dataset(runtime: &RuntimeConfig, database: &str) -> Result<Client> {
    let client = open_snowdrop_admin(runtime).await?;
    client.simple_query(&format!("USE {};", database)).await?;
    Ok(client)
}

async fn open_postgres_admin(runtime: &RuntimeConfig) -> Result<Client> {
    let password = require_postgres_password()?;
    connect_client(
        &runtime.postgres_host,
        runtime.postgres_port,
        "postgres",
        "postgres",
        Some(password),
    )
    .await
}

async fn open_postgres_dataset(runtime: &RuntimeConfig, database: &str) -> Result<Client> {
    let password = require_postgres_password()?;
    connect_client(
        &runtime.postgres_host,
        runtime.postgres_port,
        "postgres",
        database,
        Some(password),
    )
    .await
}

async fn connect_client(
    host: &str,
    port: u16,
    user: &str,
    dbname: &str,
    password: Option<String>,
) -> Result<Client> {
    let mut config = tokio_postgres::Config::new();
    config.host(host).port(port).user(user).dbname(dbname);
    if let Some(password) = password.as_deref() {
        config.password(password);
    }
    let (client, connection) = config.connect(NoTls).await?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    Ok(client)
}

async fn execute_query(client: &Client, sql: &str) -> std::result::Result<Vec<Vec<String>>, String> {
    match client.simple_query(sql).await {
        Ok(messages) => Ok(messages
            .iter()
            .filter_map(|msg| match msg {
                SimpleQueryMessage::Row(row) => Some((0..row.len())
                    .map(|idx| normalize_cell(row.get(idx).unwrap_or("NULL")))
                    .collect::<Vec<_>>()),
                _ => None,
            })
            .collect()),
        Err(err) => Err(err.to_string()),
    }
}

fn normalize_rows(rows: Vec<Vec<String>>) -> Vec<Vec<String>> {
    rows.into_iter()
        .map(|row| row.into_iter().map(normalize_cell).collect())
        .collect()
}

fn normalize_cell(value: impl AsRef<str>) -> String {
    let raw = value.as_ref().trim();
    if raw.eq_ignore_ascii_case("null") {
        return "NULL".into();
    }
    if let Ok(num) = raw.parse::<i64>() {
        return num.to_string();
    }
    if let Ok(num) = raw.parse::<f64>() {
        return format!("{:.6}", num);
    }
    raw.to_string()
}

fn require_postgres_password() -> Result<String> {
    std::env::var("PGPASSWORD")
        .or_else(|_| std::env::var("POSTGRES_PASSWORD"))
        .map_err(|_| {
            anyhow!(
                "PostgreSQL password is required for differential and PostgreSQL-style suites; set PGPASSWORD or POSTGRES_PASSWORD"
            )
        })
}

fn looks_unsupported(message: &str) -> bool {
    let lower = message.to_lowercase();
    ["unsupported", "not implemented", "only inner join", "not supported"]
        .iter()
        .any(|needle| lower.contains(needle))
}

fn pass(suite: &str, dataset: &str, group: impl Into<String>, case_name: &str, details: impl Into<String>) -> CaseOutcome {
    CaseOutcome {
        suite: suite.into(),
        dataset: Some(dataset.into()),
        group: group.into(),
        case_name: case_name.into(),
        status: "pass".into(),
        details: details.into(),
        expected: None,
        actual: None,
    }
}

fn fail(suite: &str, dataset: &str, group: impl Into<String>, case_name: &str, details: String) -> CaseOutcome {
    CaseOutcome {
        suite: suite.into(),
        dataset: Some(dataset.into()),
        group: group.into(),
        case_name: case_name.into(),
        status: "fail".into(),
        details,
        expected: None,
        actual: None,
    }
}

fn mismatch(
    suite: &str,
    dataset: &str,
    group: impl Into<String>,
    case_name: &str,
    expected: String,
    actual: String,
) -> CaseOutcome {
    CaseOutcome {
        suite: suite.into(),
        dataset: Some(dataset.into()),
        group: group.into(),
        case_name: case_name.into(),
        status: "mismatch".into(),
        details: "actual output diverged from expected output".into(),
        expected: Some(expected),
        actual: Some(actual),
    }
}

fn skip_case(
    suite: &str,
    dataset: &str,
    group: impl Into<String>,
    case_name: &str,
    details: impl Into<String>,
) -> CaseOutcome {
    CaseOutcome {
        suite: suite.into(),
        dataset: Some(dataset.into()),
        group: group.into(),
        case_name: case_name.into(),
        status: "skip".into(),
        details: details.into(),
        expected: None,
        actual: None,
    }
}

fn rows_to_text(rows: &[Vec<String>]) -> String {
    rows.iter()
        .map(|row| row.join("|"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_expected_rows(text: &str) -> Vec<Vec<String>> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.split('|').map(normalize_cell).collect())
        .collect()
}

fn sorted_files(dir: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut files = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some(extension))
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("case")
        .to_string()
}

fn write_results_artifacts(repo_root: &Path, results: &UnifiedResults) -> Result<()> {
    let raw_path = repo_root.join(RAW_RESULTS_PATH);
    if let Some(parent) = raw_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&raw_path, serde_json::to_string_pretty(results)?)?;

    let report = render_report(results);
    fs::write(repo_root.join(REPORT_PATH), report)?;
    println!("Wrote {}", raw_path.display());
    println!("Wrote {}", REPORT_PATH);
    Ok(())
}

fn read_results(repo_root: &Path) -> Result<UnifiedResults> {
    let raw = fs::read_to_string(repo_root.join(RAW_RESULTS_PATH))
        .with_context(|| format!("could not read {}", RAW_RESULTS_PATH))?;
    Ok(serde_json::from_str(&raw)?)
}

fn render_report(results: &UnifiedResults) -> String {
    let mut out = String::new();
    out.push_str("# Snowdrop Test Report\n\n");
    out.push_str(&format!("Generated at: `{}`\n\n", results.generated_at));

    out.push_str("## Environment\n\n");
    out.push_str("| Component | Host | Port |\n");
    out.push_str("|---|---|---|\n");
    out.push_str(&format!(
        "| Snowdrop pgwire | {} | {} |\n",
        results.runtime.snowdrop_host, results.runtime.snowdrop_port
    ));
    out.push_str(&format!(
        "| PostgreSQL | {} | {} |\n",
        results.runtime.postgres_host, results.runtime.postgres_port
    ));
    out.push_str(&format!(
        "| Health | {} | {} |\n",
        results.runtime.health_host, results.runtime.health_port
    ));
    out.push_str(&format!(
        "| Metrics | {} | {} |\n\n",
        results.runtime.metrics_host, results.runtime.metrics_port
    ));

    out.push_str("## Dataset Inventory\n\n");
    out.push_str("| Dataset | Database | PostgreSQL | Purpose | Suites |\n");
    out.push_str("|---|---|---|---|---|\n");
    for dataset in &results.datasets {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            dataset.name,
            dataset.database,
            if dataset.postgres_enabled { "yes" } else { "no" },
            dataset.purpose,
            dataset.suites.join(", ")
        ));
    }
    out.push('\n');

    out.push_str("## Suite Summary By Dataset\n\n");
    out.push_str("| Dataset | Suite | Pass | Fail | Mismatch | Skip |\n");
    out.push_str("|---|---|---|---|---|---|\n");
    for ((dataset, suite), counts) in summarize_by_dataset(&results.suite_runs) {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            dataset,
            suite,
            counts.get("pass").copied().unwrap_or(0),
            counts.get("fail").copied().unwrap_or(0),
            counts.get("mismatch").copied().unwrap_or(0),
            counts.get("skip").copied().unwrap_or(0),
        ));
    }
    out.push('\n');

    for suite_name in ["golden", "differential", "sqllogictest", "tap", "postgres-style"] {
        if let Some(run) = results.suite_runs.iter().find(|run| run.suite == suite_name) {
            out.push_str(&format!("## {} Results\n\n", prettify_suite_name(suite_name)));
            out.push_str("| Dataset | Group | Case | Status | Details |\n");
            out.push_str("|---|---|---|---|---|\n");
            for case in &run.cases {
                out.push_str(&format!(
                    "| {} | {} | {} | {} | {} |\n",
                    case.dataset.as_deref().unwrap_or("-"),
                    case.group,
                    case.case_name,
                    case.status,
                    sanitize_table_cell(&case.details)
                ));
            }
            out.push('\n');
        }
    }

    let skipped: Vec<&CaseOutcome> = results
        .suite_runs
        .iter()
        .flat_map(|run| run.cases.iter())
        .filter(|case| case.status == "skip")
        .collect();
    if !skipped.is_empty() {
        out.push_str("## Skipped Or Unsupported Cases\n\n");
        out.push_str("| Suite | Dataset | Case | Reason |\n");
        out.push_str("|---|---|---|---|\n");
        for case in skipped {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                case.suite,
                case.dataset.as_deref().unwrap_or("-"),
                case.case_name,
                sanitize_table_cell(&case.details)
            ));
        }
        out.push('\n');
    }

    if !results.tpch_results.is_empty() {
        let tpch_meta = results.tpch_options.as_ref().map(|meta| {
            format!(
                "Runs: `{}`. Seed: `{}`. Indexed phase: `{}`.\n\n",
                meta.runs,
                meta.seed,
                if meta.with_indexes { "enabled" } else { "disabled" }
            )
        });
        if let Some(meta) = tpch_meta {
            out.push_str(&meta);
        }
        let scale = results.tpch_options.as_ref().map(|meta| meta.scale).unwrap_or(1);
        let with_indexes = results
            .tpch_options
            .as_ref()
            .map(|meta| meta.with_indexes)
            .unwrap_or(false);
        out.push_str(&tpch_markdown_section(
            &results.tpch_results,
            scale,
            with_indexes,
        ));
        out.push('\n');
    }

    let totals = summarize_totals(&results.suite_runs);
    out.push_str("## Conclusions\n\n");
    out.push_str(&format!(
        "- Total cases: `{}`\n- Passed: `{}`\n- Failed: `{}`\n- Mismatched: `{}`\n- Skipped: `{}`\n",
        totals.get("total").copied().unwrap_or(0),
        totals.get("pass").copied().unwrap_or(0),
        totals.get("fail").copied().unwrap_or(0),
        totals.get("mismatch").copied().unwrap_or(0),
        totals.get("skip").copied().unwrap_or(0),
    ));

    out
}

fn prettify_suite_name(name: &str) -> &str {
    match name {
        "sqllogictest" => "SQLLogicTest",
        "tap" => "TAP-style",
        "postgres-style" => "PostgreSQL-style",
        other => other,
    }
}

fn sanitize_table_cell(text: &str) -> String {
    text.replace('\n', "<br>")
}

fn summarize_by_dataset(suite_runs: &[SuiteRun]) -> BTreeMap<(String, String), BTreeMap<String, usize>> {
    let mut map = BTreeMap::new();
    for run in suite_runs {
        for case in &run.cases {
            let key = (
                case.dataset.clone().unwrap_or_else(|| "-".into()),
                run.suite.clone(),
            );
            let counts = map.entry(key).or_insert_with(BTreeMap::new);
            *counts.entry(case.status.clone()).or_insert(0) += 1;
        }
    }
    map
}

fn summarize_totals(suite_runs: &[SuiteRun]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for run in suite_runs {
        for case in &run.cases {
            *counts.entry("total".into()).or_insert(0) += 1;
            *counts.entry(case.status.clone()).or_insert(0) += 1;
        }
    }
    counts
}

fn parse_sqllogictest_file(path: &Path) -> Result<SqlLogicFile> {
    let content = fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().collect();
    let mut dataset = None;
    let mut commands = Vec::new();
    let mut idx = 0;

    while idx < lines.len() {
        let line = lines[idx].trim();
        if line.is_empty() {
            idx += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix("# dataset:") {
            dataset = Some(rest.trim().to_string());
            idx += 1;
            continue;
        }
        if line.starts_with('#') {
            idx += 1;
            continue;
        }

        if line == "statement ok" {
            idx += 1;
            let (sql, next_idx) = read_block_until_blank(&lines, idx);
            commands.push(SqlLogicCommand::StatementOk { sql });
            idx = next_idx;
            continue;
        }
        if line == "statement error" {
            idx += 1;
            let (sql, next_idx) = read_block_until_blank(&lines, idx);
            commands.push(SqlLogicCommand::StatementError { sql });
            idx = next_idx;
            continue;
        }
        if line.starts_with("query ") {
            idx += 1;
            let mut sql_lines = Vec::new();
            while idx < lines.len() && lines[idx].trim() != "----" {
                sql_lines.push(lines[idx]);
                idx += 1;
            }
            if idx >= lines.len() {
                return Err(anyhow!("missing ---- in {}", path.display()));
            }
            idx += 1;
            let mut expected_lines = Vec::new();
            while idx < lines.len() && !lines[idx].trim().is_empty() {
                expected_lines.push(lines[idx]);
                idx += 1;
            }
            let expected_rows = expected_lines
                .into_iter()
                .map(|line| {
                    if line.contains('\t') {
                        line.split('\t').map(normalize_cell).collect::<Vec<_>>()
                    } else {
                        line.split_whitespace().map(normalize_cell).collect::<Vec<_>>()
                    }
                })
                .collect();
            commands.push(SqlLogicCommand::Query {
                sql: sql_lines.join("\n"),
                expected_rows,
            });
            continue;
        }

        return Err(anyhow!(
            "unsupported sqllogictest directive '{}' in {}",
            line,
            path.display()
        ));
    }

    Ok(SqlLogicFile {
        path: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("suite.test")
            .to_string(),
        dataset: dataset.ok_or_else(|| anyhow!("missing '# dataset:' header in {}", path.display()))?,
        commands,
    })
}

fn read_block_until_blank(lines: &[&str], start_idx: usize) -> (String, usize) {
    let mut idx = start_idx;
    let mut block = Vec::new();
    while idx < lines.len() && !lines[idx].trim().is_empty() {
        block.push(lines[idx]);
        idx += 1;
    }
    (block.join("\n"), idx)
}

async fn http_get_text(host: &str, port: u16, path: &str) -> Result<String> {
    let mut stream = TcpStream::connect((host, port)).await?;
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, host
    );
    stream.write_all(request.as_bytes()).await?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    let response = String::from_utf8_lossy(&buf);
    if let Some((_, body)) = response.split_once("\r\n\r\n") {
        Ok(body.to_string())
    } else {
        Err(anyhow!("invalid HTTP response from {}:{}", host, port))
    }
}
