use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_postgres::{Client, NoTls, SimpleQueryMessage};
use tpch_bench::{tpch_markdown_section, BenchmarkOptions, QueryResult};

const RAW_RESULTS_PATH: &str = "reports/raw/snowdrop-testkit-results.json";
const REPORT_PATH: &str = "reports/test-report.md";
const LEGACY_RAW_RESULTS_PATH: &str = "target/snowdrop-testkit/results.json";
const MIGRATION_REPORT_DIR: &str = "reports/migration";
const PHASE3_RUNTIME_PROBE_DB: &str = "__snowdrop_phase3_probe";
const SKIP_RUNTIME_PROBE_ENV: &str = "SNOWDROP_SKIP_RUNTIME_PROBE";

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
    StatementOk {
        sql: String,
    },
    StatementError {
        sql: String,
    },
    Query {
        sql: String,
        expected_rows: Vec<Vec<String>>,
    },
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationPreflightReport {
    generated_at: String,
    runtime: RuntimeConfig,
    source_database: String,
    source_schema_filter: Option<String>,
    summary: MigrationSummary,
    tables: Vec<MigrationTableReport>,
    findings: Vec<MigrationFinding>,
    unsupported_inventory: BTreeMap<String, Vec<String>>,
    partial_inventory: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationWorkflowReport {
    generated_at: String,
    runtime: RuntimeConfig,
    source_database: String,
    target_database: String,
    source_schema_filter: Option<String>,
    preflight: MigrationPreflightReport,
    summary: MigrationWorkflowSummary,
    table_runs: Vec<MigrationTableRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationAutoReport {
    generated_at: String,
    runtime: RuntimeConfig,
    source_mode: String,
    source_database: String,
    source_schema_filter: Option<String>,
    target_database: Option<String>,
    success: bool,
    summary: MigrationAutoSummary,
    blockers: Vec<MigrationFinding>,
    skipped_objects: Vec<String>,
    workflow: Option<MigrationWorkflowReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationSummary {
    table_count: usize,
    column_count: usize,
    constraint_count: usize,
    index_count: usize,
    supported_tables: usize,
    partial_tables: usize,
    unsupported_tables: usize,
    ddl_ready_tables: usize,
    ddl_partial_tables: usize,
    ddl_blocked_tables: usize,
    findings_by_status: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationWorkflowSummary {
    planned_tables: usize,
    blocked_tables: usize,
    ddl_applied_tables: usize,
    loaded_tables: usize,
    verified_tables: usize,
    row_count_matches: usize,
    sample_matches: usize,
    aggregate_matches: usize,
    replay_queries_run: usize,
    replay_queries_matched: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationAutoSummary {
    supported_tables: usize,
    partial_tables: usize,
    blocked_tables: usize,
    verification_passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationTableReport {
    schema_name: String,
    table_name: String,
    status: String,
    translation_status: String,
    columns: Vec<MigrationColumnReport>,
    constraints: Vec<MigrationConstraintReport>,
    indexes: Vec<MigrationIndexReport>,
    notes: Vec<String>,
    translation_notes: Vec<String>,
    translated_ddl: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationColumnReport {
    column_name: String,
    source_type: String,
    snowdrop_type: Option<String>,
    nullable: bool,
    default: Option<String>,
    status: String,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationConstraintReport {
    name: String,
    kind: String,
    columns: Vec<String>,
    status: String,
    reference: Option<String>,
    delete_rule: Option<String>,
    update_rule: Option<String>,
    check_clause: Option<String>,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationIndexReport {
    name: String,
    definition: String,
    status: String,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationFinding {
    status: String,
    object_kind: String,
    object_name: String,
    category: String,
    message: String,
    recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationTableRun {
    schema_name: String,
    table_name: String,
    translation_status: String,
    ddl_applied: bool,
    load_status: String,
    source_row_count: Option<i64>,
    target_row_count: Option<i64>,
    row_count_match: Option<bool>,
    sample_rows_match: Option<bool>,
    aggregate_match: Option<bool>,
    replay_checks: Vec<MigrationReplayCheck>,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationReplayCheck {
    name: String,
    source_sql: String,
    target_sql: String,
    matched: Option<bool>,
    note: Option<String>,
}

#[derive(Debug, Clone)]
struct PgTable {
    schema_name: String,
    table_name: String,
}

#[derive(Debug, Clone)]
struct PgColumn {
    schema_name: String,
    table_name: String,
    column_name: String,
    nullable: bool,
    data_type: String,
    udt_name: String,
    char_length: Option<i32>,
    numeric_precision: Option<i32>,
    numeric_scale: Option<i32>,
    datetime_precision: Option<i32>,
    default: Option<String>,
}

#[derive(Debug, Clone)]
struct PgConstraint {
    schema_name: String,
    table_name: String,
    constraint_name: String,
    constraint_type: String,
    columns: Vec<String>,
    ref_schema_name: Option<String>,
    ref_table_name: Option<String>,
    ref_columns: Vec<String>,
    update_rule: Option<String>,
    delete_rule: Option<String>,
    check_clause: Option<String>,
}

#[derive(Debug, Clone)]
struct PgIndex {
    schema_name: String,
    table_name: String,
    index_name: String,
    index_def: String,
}

#[derive(Debug, Clone)]
struct LoadOrderPlan {
    ordered: Vec<usize>,
    cyclic: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MigrationSourceArg {
    LiveDatabase(String),
    DumpDir {
        dump_dir: PathBuf,
        source_database: String,
    },
}

#[derive(Debug, Clone)]
struct PreparedMigrationSource {
    logical_name: String,
    postgres_database: String,
    cleanup_database: Option<String>,
}

enum Command {
    ListDatasets,
    Load {
        dataset: String,
    },
    Run {
        dataset: Option<String>,
        suite: String,
    },
    RunAll {
        with_tpch: bool,
    },
    Report,
    MigrationPreflight {
        source: MigrationSourceArg,
        schema: Option<String>,
    },
    MigrationRun {
        source: MigrationSourceArg,
        schema: Option<String>,
        target_database: Option<String>,
    },
    MigrationAuto {
        source: MigrationSourceArg,
        schema: Option<String>,
        target_database: Option<String>,
    },
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
            load_dataset(&repo_root, &runtime, &manifest, manifest.postgres_enabled).await?;
            println!("Loaded dataset {}", manifest.name);
        }
        Command::Run { dataset, suite } => {
            ensure_phase3_runtime(&runtime).await?;
            let datasets = load_dataset_manifests(&repo_root)?;
            let run = run_named_suite(&repo_root, &runtime, &suite, dataset.as_deref(), &datasets)
                .await?;
            let results = UnifiedResults {
                generated_at: Utc::now().to_rfc3339(),
                runtime,
                datasets,
                suite_runs: vec![run],
                tpch_results: Vec::new(),
                tpch_options: None,
            };
            write_results_artifacts(&repo_root, &results)?;
            print_console_summary(&results);
        }
        Command::RunAll { with_tpch } => {
            ensure_phase3_runtime(&runtime).await?;
            let datasets = load_dataset_manifests(&repo_root)?;
            let mut suite_runs = Vec::new();
            for suite in [
                "golden",
                "differential",
                "sqllogictest",
                "tap",
                "postgres-style",
            ] {
                suite_runs
                    .push(run_named_suite(&repo_root, &runtime, suite, None, &datasets).await?);
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
            print_console_summary(&results);
        }
        Command::Report => {
            let results = read_results(&repo_root)?;
            let report = render_report(&results);
            fs::write(repo_root.join(REPORT_PATH), report)?;
            println!("Wrote {}", REPORT_PATH);
        }
        Command::MigrationPreflight { source, schema } => {
            let report =
                run_migration_preflight(&repo_root, &runtime, &source, schema.as_deref()).await?;
            print_migration_preflight_summary(&report);
        }
        Command::MigrationRun {
            source,
            schema,
            target_database,
        } => {
            let report = run_migration_workflow(
                &repo_root,
                &runtime,
                &source,
                schema.as_deref(),
                target_database.as_deref(),
            )
            .await?;
            print_migration_workflow_summary(&report);
        }
        Command::MigrationAuto {
            source,
            schema,
            target_database,
        } => {
            let report = run_migration_auto(
                &repo_root,
                &runtime,
                &source,
                schema.as_deref(),
                target_database.as_deref(),
            )
            .await?;
            print_migration_auto_summary(&report);
        }
    }

    Ok(())
}

async fn ensure_phase3_runtime(runtime: &RuntimeConfig) -> Result<()> {
    if std::env::var_os(SKIP_RUNTIME_PROBE_ENV).is_some() {
        return Ok(());
    }

    let admin = open_snowdrop_admin(runtime)
        .await
        .context("could not connect to Snowdrop for the Phase 3 runtime preflight")?;
    let _ = admin
        .simple_query(&format!("DROP DATABASE {};", PHASE3_RUNTIME_PROBE_DB))
        .await;
    admin
        .simple_query(&format!("CREATE DATABASE {};", PHASE3_RUNTIME_PROBE_DB))
        .await
        .with_context(|| {
            format!(
                "could not create Phase 3 runtime probe database '{}'",
                PHASE3_RUNTIME_PROBE_DB
            )
        })?;

    let probe_result = async {
        let client = open_snowdrop_dataset(runtime, PHASE3_RUNTIME_PROBE_DB).await?;
        client
            .simple_query("CREATE TABLE probe (id BIGINT PRIMARY KEY, grp VARCHAR(16), val BIGINT);")
            .await?;
        client
            .simple_query(
                "INSERT INTO probe (id, grp, val) VALUES (1, 'a', 10), (2, 'a', 20), (3, 'b', 30);",
            )
            .await?;

        for (family, sql) in phase3_runtime_probes() {
            match execute_query(&client, sql).await {
                Ok(_) => {}
                Err(err) if looks_like_stale_phase3_runtime(&err) => {
                    return Err(anyhow!(
                        "Snowdrop runtime preflight detected an outdated Phase 3 server while probing {}: {}. Rebuild and restart the orchestrator, then rerun the harness. Suggested command: `docker compose up -d --build orchestrator`. If you intentionally want to skip this guard, set {}=1.",
                        family,
                        err,
                        SKIP_RUNTIME_PROBE_ENV
                    ));
                }
                Err(err) => {
                    return Err(anyhow!(
                        "Snowdrop runtime preflight failed while probing {}: {}",
                        family,
                        err
                    ));
                }
            }
        }

        Ok::<(), anyhow::Error>(())
    }
    .await;

    let _ = admin
        .simple_query(&format!("DROP DATABASE {};", PHASE3_RUNTIME_PROBE_DB))
        .await;
    probe_result
}

fn phase3_runtime_probes() -> [(&'static str, &'static str); 4] {
    [
        (
            "subqueries",
            "SELECT id FROM probe WHERE id IN (SELECT id FROM probe WHERE val >= 20) ORDER BY id;",
        ),
        (
            "derived tables",
            "SELECT d.id FROM (SELECT id FROM probe WHERE val >= 20) AS d ORDER BY d.id;",
        ),
        (
            "CTEs",
            "WITH picked AS (SELECT id FROM probe WHERE val >= 20) SELECT id FROM picked ORDER BY id;",
        ),
        (
            "compound queries",
            "SELECT id FROM probe WHERE id = 1 UNION ALL SELECT id FROM probe WHERE id = 2 ORDER BY id;",
        ),
    ]
}

fn parse_command(args: &[String]) -> Result<Command> {
    match args.get(1).map(String::as_str) {
        Some("list-datasets") => Ok(Command::ListDatasets),
        Some("load") => {
            let dataset =
                read_flag(args, "--dataset").ok_or_else(|| anyhow!("missing --dataset"))?;
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
        Some("migration-preflight") => {
            let source = parse_migration_source(args)?;
            let schema = read_flag(args, "--schema");
            Ok(Command::MigrationPreflight { source, schema })
        }
        Some("migration-run") => {
            let source = parse_migration_source(args)?;
            let schema = read_flag(args, "--schema");
            let target_database = read_flag(args, "--target-database");
            Ok(Command::MigrationRun {
                source,
                schema,
                target_database,
            })
        }
        Some("migration-auto") => {
            let source = parse_migration_source(args)?;
            let schema = read_flag(args, "--schema");
            let target_database = read_flag(args, "--target-database");
            Ok(Command::MigrationAuto {
                source,
                schema,
                target_database,
            })
        }
        _ => Err(anyhow!(
            "usage: snowdrop-testkit <list-datasets|load|run|run-all|report|migration-preflight|migration-run|migration-auto> [flags]"
        )),
    }
}

fn parse_migration_source(args: &[String]) -> Result<MigrationSourceArg> {
    let database = read_flag(args, "--database");
    let dump_dir = read_flag(args, "--dump-dir");
    let source_database = read_flag(args, "--source-database");

    match (database, dump_dir) {
        (Some(database), None) => Ok(MigrationSourceArg::LiveDatabase(database)),
        (None, Some(dump_dir)) => {
            let dump_dir = PathBuf::from(dump_dir);
            let source_database = source_database
                .filter(|value| !value.trim().is_empty())
                .or_else(|| {
                    dump_dir
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                })
                .ok_or_else(|| {
                    anyhow!(
                        "dump-based migration requires --source-database or a dump directory with a readable final path segment"
                    )
                })?;
            Ok(MigrationSourceArg::DumpDir {
                dump_dir,
                source_database,
            })
        }
        (Some(_), Some(_)) => Err(anyhow!(
            "specify either --database or --dump-dir for migration commands, but not both"
        )),
        (None, None) => Err(anyhow!(
            "migration commands require either --database <name> or --dump-dir <path>"
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
        "differential" => {
            run_differential_suite(repo_root, runtime, dataset_filter, datasets).await
        }
        "sqllogictest" => {
            run_sqllogictest_suite(repo_root, runtime, dataset_filter, datasets).await
        }
        "tap" => run_tap_suite(repo_root, runtime, dataset_filter, datasets).await,
        "postgres-style" => {
            run_postgres_style_suite(repo_root, runtime, dataset_filter, datasets).await
        }
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

    for dataset in datasets
        .iter()
        .filter(|d| dataset_filter.map(|f| f == d.name).unwrap_or(true))
    {
        let dataset_dir = root.join(&dataset.name);
        if !dataset_dir.exists() {
            continue;
        }

        for file in sorted_files(&dataset_dir, "json")? {
            let case: GoldenCase = serde_json::from_str(&fs::read_to_string(&file)?)?;
            load_dataset(repo_root, runtime, dataset, false).await?;
            let client = open_snowdrop_dataset(runtime, &dataset.database).await?;
            let query = execute_query(&client, &case.sql).await;
            let outcome = match query {
                Ok(rows) => {
                    if let Some(expected) = case.expected_rows.clone() {
                        let expected_rows = normalize_rows(expected);
                        if expected_rows == rows {
                            pass(
                                "golden",
                                &dataset.name,
                                file_stem(&file),
                                &case.name,
                                "matched expected rows",
                            )
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
                            pass(
                                "golden",
                                &dataset.name,
                                file_stem(&file),
                                &case.name,
                                "matched expected error",
                            )
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
                        fail("golden", &dataset.name, file_stem(&file), &case.name, err)
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

    for dataset in datasets
        .iter()
        .filter(|d| dataset_filter.map(|f| f == d.name).unwrap_or(true))
    {
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

        for file in sorted_files(&dataset_dir, "json")? {
            let case: DifferentialCase = serde_json::from_str(&fs::read_to_string(&file)?)?;
            load_dataset(repo_root, runtime, dataset, true).await?;
            let snowdrop = open_snowdrop_dataset(runtime, &dataset.database).await?;
            let postgres = open_postgres_dataset(runtime, &dataset.database).await?;
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
        load_dataset(repo_root, runtime, dataset, false).await?;
        let client = open_snowdrop_dataset(runtime, &dataset.database).await?;

        for (idx, command) in parsed.commands.iter().enumerate() {
            let name = format!("command_{}", idx + 1);
            let outcome = match command {
                SqlLogicCommand::StatementOk { sql } => match execute_query(&client, sql).await {
                    Ok(_) => pass(
                        "sqllogictest",
                        &dataset.name,
                        &parsed.path,
                        &name,
                        "statement ok",
                    ),
                    Err(err) => fail("sqllogictest", &dataset.name, &parsed.path, &name, err),
                },
                SqlLogicCommand::StatementError { sql } => {
                    match execute_query(&client, sql).await {
                        Ok(_) => fail(
                            "sqllogictest",
                            &dataset.name,
                            &parsed.path,
                            &name,
                            "statement succeeded but expected error".into(),
                        ),
                        Err(_) => pass(
                            "sqllogictest",
                            &dataset.name,
                            &parsed.path,
                            &name,
                            "statement error",
                        ),
                    }
                }
                SqlLogicCommand::Query { sql, expected_rows } => {
                    match execute_query(&client, sql).await {
                        Ok(rows) => {
                            let expected = normalize_rows(expected_rows.clone());
                            if expected == rows {
                                pass(
                                    "sqllogictest",
                                    &dataset.name,
                                    &parsed.path,
                                    &name,
                                    "query matched",
                                )
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
                    }
                }
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
    let tap_out_root = repo_root.join("reports/tap");
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

        for (idx, check) in suite.checks.iter().enumerate() {
            let number = idx + 1;
            let outcome = match check {
                TapCheck::Health {
                    name,
                    path,
                    contains,
                } => {
                    let body =
                        http_get_text(&runtime.health_host, runtime.health_port, path).await?;
                    if contains.as_ref().map(|c| body.contains(c)).unwrap_or(true) {
                        tap_lines.push(format!("ok {} - {}", number, name));
                        pass(
                            "tap",
                            suite.dataset.as_deref().unwrap_or("runtime"),
                            &suite.name,
                            name,
                            "health ok",
                        )
                    } else {
                        tap_lines.push(format!("not ok {} - {}", number, name));
                        fail(
                            "tap",
                            suite.dataset.as_deref().unwrap_or("runtime"),
                            &suite.name,
                            name,
                            body,
                        )
                    }
                }
                TapCheck::Metrics {
                    name,
                    path,
                    contains,
                } => {
                    let body =
                        http_get_text(&runtime.metrics_host, runtime.metrics_port, path).await?;
                    if contains.as_ref().map(|c| body.contains(c)).unwrap_or(true) {
                        tap_lines.push(format!("ok {} - {}", number, name));
                        pass(
                            "tap",
                            suite.dataset.as_deref().unwrap_or("runtime"),
                            &suite.name,
                            name,
                            "metrics ok",
                        )
                    } else {
                        tap_lines.push(format!("not ok {} - {}", number, name));
                        fail(
                            "tap",
                            suite.dataset.as_deref().unwrap_or("runtime"),
                            &suite.name,
                            name,
                            body,
                        )
                    }
                }
                TapCheck::Sql {
                    name,
                    sql,
                    expected_rows,
                    expected_error,
                } => {
                    let dataset_name = suite
                        .dataset
                        .as_deref()
                        .ok_or_else(|| anyhow!("tap SQL checks require a dataset"))?;
                    let dataset = datasets
                        .iter()
                        .find(|d| d.name == dataset_name)
                        .ok_or_else(|| anyhow!("unknown dataset {}", dataset_name))?;
                    load_dataset(repo_root, runtime, dataset, false).await?;
                    let client = open_snowdrop_dataset(runtime, &dataset.database).await?;
                    match execute_query(&client, sql).await {
                        Ok(rows) => {
                            if let Some(expected) = expected_rows {
                                let expected = normalize_rows(expected.clone());
                                if expected == rows {
                                    tap_lines.push(format!("ok {} - {}", number, name));
                                    pass(
                                        "tap",
                                        suite.dataset.as_deref().unwrap_or("runtime"),
                                        &suite.name,
                                        name,
                                        "sql matched",
                                    )
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
                                pass(
                                    "tap",
                                    suite.dataset.as_deref().unwrap_or("runtime"),
                                    &suite.name,
                                    name,
                                    "sql ok",
                                )
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
                                pass(
                                    "tap",
                                    suite.dataset.as_deref().unwrap_or("runtime"),
                                    &suite.name,
                                    name,
                                    "expected error",
                                )
                            } else {
                                tap_lines.push(format!("not ok {} - {}", number, name));
                                fail(
                                    "tap",
                                    suite.dataset.as_deref().unwrap_or("runtime"),
                                    &suite.name,
                                    name,
                                    err,
                                )
                            }
                        }
                    }
                }
            };
            cases.push(outcome);
        }

        fs::write(
            tap_out_root.join(format!("{}.tap", suite.name)),
            tap_lines.join("\n"),
        )?;
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
        if dataset_filter
            .map(|f| f != manifest.dataset)
            .unwrap_or(false)
        {
            continue;
        }

        let dataset = datasets
            .iter()
            .find(|d| d.name == manifest.dataset)
            .ok_or_else(|| anyhow!("unknown dataset {}", manifest.dataset))?;
        for sql_file in sorted_files(&group_root.join("sql"), "sql")? {
            let base = file_stem(&sql_file);
            let expected_file = group_root.join("expected").join(format!("{}.out", base));
            let sql = fs::read_to_string(&sql_file)?;
            let expected = fs::read_to_string(expected_file)?.trim().to_string();
            load_dataset(repo_root, runtime, dataset, false).await?;
            let client = open_snowdrop_dataset(runtime, &dataset.database).await?;
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
                            pass(
                                "postgres-style",
                                &dataset.name,
                                &manifest.name,
                                &base,
                                "matched expected output",
                            )
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
                    if expected == "ERROR:"
                        || expected
                            .strip_prefix("ERROR:")
                            .map(|suffix| err.contains(suffix.trim()))
                            .unwrap_or(false)
                    {
                        pass(
                            "postgres-style",
                            &dataset.name,
                            &manifest.name,
                            &base,
                            "matched expected error",
                        )
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

async fn load_dataset(
    repo_root: &Path,
    runtime: &RuntimeConfig,
    dataset: &DatasetManifest,
    load_postgres: bool,
) -> Result<()> {
    let dataset_dir = tests_root(repo_root).join("datasets").join(&dataset.name);
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
    snowdrop
        .simple_query(&format!("USE {};", dataset.database))
        .await?;
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

async fn prepare_migration_source(
    runtime: &RuntimeConfig,
    source: &MigrationSourceArg,
) -> Result<PreparedMigrationSource> {
    match source {
        MigrationSourceArg::LiveDatabase(database) => Ok(PreparedMigrationSource {
            logical_name: database.clone(),
            postgres_database: database.clone(),
            cleanup_database: None,
        }),
        MigrationSourceArg::DumpDir {
            dump_dir,
            source_database,
        } => {
            let staging_database = staging_postgres_database_name(source_database);
            let admin = open_postgres_admin(runtime).await?;
            let _ = admin
                .simple_query(&format!("DROP DATABASE {};", staging_database))
                .await;
            admin
                .simple_query(&format!("CREATE DATABASE {};", staging_database))
                .await
                .with_context(|| {
                    format!(
                        "could not create PostgreSQL staging database '{}' for dump import",
                        staging_database
                    )
                })?;

            let dataset = open_postgres_dataset(runtime, &staging_database).await?;
            apply_dump_sql_file(
                &dataset,
                &dump_dir.join("schema.sql"),
                "schema.sql",
                source_database,
            )
            .await?;
            if dump_dir.join("seed.sql").exists() {
                apply_dump_sql_file(
                    &dataset,
                    &dump_dir.join("seed.sql"),
                    "seed.sql",
                    source_database,
                )
                .await?;
            }
            drop(dataset);

            Ok(PreparedMigrationSource {
                logical_name: source_database.clone(),
                postgres_database: staging_database.clone(),
                cleanup_database: Some(staging_database),
            })
        }
    }
}

async fn cleanup_prepared_migration_source(
    runtime: &RuntimeConfig,
    prepared: &PreparedMigrationSource,
) -> Result<()> {
    let Some(database) = prepared.cleanup_database.as_deref() else {
        return Ok(());
    };
    let admin = open_postgres_admin(runtime).await?;
    let _ = admin
        .simple_query(&format!("DROP DATABASE {};", database))
        .await;
    Ok(())
}

async fn apply_dump_sql_file(
    client: &Client,
    path: &Path,
    label: &str,
    source_database: &str,
) -> Result<()> {
    let sql = fs::read_to_string(path)
        .with_context(|| format!("could not read dump file '{}'", path.display()))?;
    if sql.trim().is_empty() {
        return Ok(());
    }
    client.simple_query(&sql).await.with_context(|| {
        format!(
            "could not apply {} for dump-based migration source '{}'",
            label, source_database
        )
    })?;
    Ok(())
}

fn staging_postgres_database_name(source_database: &str) -> String {
    format!(
        "stage_{}_{}",
        sanitize_report_slug(source_database).replace('-', "_"),
        Utc::now().timestamp_millis()
    )
}

async fn run_migration_preflight(
    repo_root: &Path,
    runtime: &RuntimeConfig,
    source: &MigrationSourceArg,
    schema_filter: Option<&str>,
) -> Result<MigrationPreflightReport> {
    let prepared = prepare_migration_source(runtime, source).await?;
    let result = run_migration_preflight_for_source(
        repo_root,
        runtime,
        &prepared.logical_name,
        &prepared.postgres_database,
        schema_filter,
    )
    .await;
    cleanup_prepared_migration_source(runtime, &prepared).await?;
    result
}

async fn run_migration_preflight_for_source(
    repo_root: &Path,
    runtime: &RuntimeConfig,
    logical_source_database: &str,
    postgres_database: &str,
    schema_filter: Option<&str>,
) -> Result<MigrationPreflightReport> {
    let client = open_postgres_dataset(runtime, postgres_database).await?;
    let report =
        build_migration_preflight_report(&client, runtime, logical_source_database, schema_filter)
            .await?;
    write_migration_preflight_artifacts(repo_root, &report)?;
    Ok(report)
}

async fn run_migration_workflow(
    repo_root: &Path,
    runtime: &RuntimeConfig,
    source: &MigrationSourceArg,
    schema_filter: Option<&str>,
    target_database: Option<&str>,
) -> Result<MigrationWorkflowReport> {
    let prepared = prepare_migration_source(runtime, source).await?;
    let result = run_migration_workflow_for_source(
        repo_root,
        runtime,
        &prepared.logical_name,
        &prepared.postgres_database,
        schema_filter,
        target_database,
    )
    .await;
    cleanup_prepared_migration_source(runtime, &prepared).await?;
    result
}

async fn run_migration_workflow_for_source(
    repo_root: &Path,
    runtime: &RuntimeConfig,
    logical_source_database: &str,
    postgres_database: &str,
    schema_filter: Option<&str>,
    target_database: Option<&str>,
) -> Result<MigrationWorkflowReport> {
    let postgres = open_postgres_dataset(runtime, postgres_database).await?;
    let preflight = build_migration_preflight_report(
        &postgres,
        runtime,
        logical_source_database,
        schema_filter,
    )
    .await?;
    write_migration_preflight_artifacts(repo_root, &preflight)?;

    let target_database = migration_target_database(logical_source_database, target_database)?;
    let admin = open_snowdrop_admin(runtime).await?;
    let _ = admin
        .simple_query(&format!("DROP DATABASE {};", target_database))
        .await;
    admin
        .simple_query(&format!("CREATE DATABASE {};", target_database))
        .await
        .with_context(|| {
            format!(
                "could not create target Snowdrop database '{}'",
                target_database
            )
        })?;
    let snowdrop = open_snowdrop_dataset(runtime, &target_database).await?;

    let load_plan = order_tables_for_load(&preflight.tables);
    let mut table_runs = Vec::new();

    for table_idx in load_plan.ordered {
        let table = &preflight.tables[table_idx];
        let mut run = MigrationTableRun {
            schema_name: table.schema_name.clone(),
            table_name: table.table_name.clone(),
            translation_status: table.translation_status.clone(),
            ddl_applied: false,
            load_status: "pending".into(),
            source_row_count: None,
            target_row_count: None,
            row_count_match: None,
            sample_rows_match: None,
            aggregate_match: None,
            replay_checks: Vec::new(),
            notes: Vec::new(),
        };

        let Some(ddl) = table.translated_ddl.as_deref() else {
            run.load_status = "blocked".into();
            run.notes
                .push("table has no safe translation preview yet, so migration was skipped".into());
            table_runs.push(run);
            continue;
        };

        if let Err(err) = snowdrop.simple_query(ddl).await {
            run.load_status = "ddl_failed".into();
            run.notes
                .push(format!("DDL application failed: {}", format_db_error(&err)));
            table_runs.push(run);
            continue;
        }
        run.ddl_applied = true;

        match load_table_rows(&postgres, &snowdrop, table).await {
            Ok(source_row_count) => {
                run.load_status = "loaded".into();
                run.source_row_count = Some(source_row_count);
            }
            Err(err) => {
                run.load_status = "load_failed".into();
                run.notes.push(format!("data load failed: {err}"));
                table_runs.push(run);
                continue;
            }
        }

        match verify_table_migration(&postgres, &snowdrop, table).await {
            Ok((
                target_row_count,
                row_count_match,
                sample_rows_match,
                aggregate_match,
                replay_checks,
                notes,
            )) => {
                run.target_row_count = Some(target_row_count);
                run.row_count_match = Some(row_count_match);
                run.sample_rows_match = Some(sample_rows_match);
                run.aggregate_match = aggregate_match;
                run.replay_checks = replay_checks;
                run.notes.extend(notes);
                run.load_status = "verified".into();
            }
            Err(err) => {
                run.load_status = "verification_failed".into();
                run.notes.push(format!("verification failed: {err}"));
            }
        }

        table_runs.push(run);
    }

    for table_idx in load_plan.cyclic {
        let table = &preflight.tables[table_idx];
        table_runs.push(MigrationTableRun {
            schema_name: table.schema_name.clone(),
            table_name: table.table_name.clone(),
            translation_status: "blocked".into(),
            ddl_applied: false,
            load_status: "blocked".into(),
            source_row_count: None,
            target_row_count: None,
            row_count_match: None,
            sample_rows_match: None,
            aggregate_match: None,
            replay_checks: Vec::new(),
            notes: vec![
                "table participates in a foreign-key dependency cycle, so automatic load ordering is not currently available".into(),
            ],
        });
    }

    let summary = MigrationWorkflowSummary {
        planned_tables: table_runs
            .iter()
            .filter(|run| run.translation_status != "blocked")
            .count(),
        blocked_tables: table_runs
            .iter()
            .filter(|run| run.translation_status == "blocked")
            .count(),
        ddl_applied_tables: table_runs.iter().filter(|run| run.ddl_applied).count(),
        loaded_tables: table_runs
            .iter()
            .filter(|run| matches!(run.load_status.as_str(), "loaded" | "verified"))
            .count(),
        verified_tables: table_runs
            .iter()
            .filter(|run| run.row_count_match.is_some())
            .count(),
        row_count_matches: table_runs
            .iter()
            .filter(|run| run.row_count_match == Some(true))
            .count(),
        sample_matches: table_runs
            .iter()
            .filter(|run| run.sample_rows_match == Some(true))
            .count(),
        aggregate_matches: table_runs
            .iter()
            .filter(|run| run.aggregate_match == Some(true))
            .count(),
        replay_queries_run: table_runs.iter().map(|run| run.replay_checks.len()).sum(),
        replay_queries_matched: table_runs
            .iter()
            .flat_map(|run| run.replay_checks.iter())
            .filter(|check| check.matched == Some(true))
            .count(),
    };

    let report = MigrationWorkflowReport {
        generated_at: Utc::now().to_rfc3339(),
        runtime: runtime.clone(),
        source_database: logical_source_database.to_string(),
        target_database,
        source_schema_filter: schema_filter.map(str::to_string),
        preflight,
        summary,
        table_runs,
    };
    write_migration_workflow_artifacts(repo_root, &report)?;
    Ok(report)
}

async fn run_migration_auto(
    repo_root: &Path,
    runtime: &RuntimeConfig,
    source: &MigrationSourceArg,
    schema_filter: Option<&str>,
    target_database: Option<&str>,
) -> Result<MigrationAutoReport> {
    let prepared = prepare_migration_source(runtime, source).await?;
    let result = run_migration_auto_for_source(
        repo_root,
        runtime,
        source,
        &prepared,
        schema_filter,
        target_database,
    )
    .await;
    cleanup_prepared_migration_source(runtime, &prepared).await?;
    result
}

async fn run_migration_auto_for_source(
    repo_root: &Path,
    runtime: &RuntimeConfig,
    source: &MigrationSourceArg,
    prepared: &PreparedMigrationSource,
    schema_filter: Option<&str>,
    target_database: Option<&str>,
) -> Result<MigrationAutoReport> {
    let preflight = run_migration_preflight_for_source(
        repo_root,
        runtime,
        &prepared.logical_name,
        &prepared.postgres_database,
        schema_filter,
    )
    .await?;
    let blockers = collect_automatic_migration_blockers(&preflight);
    let skipped_objects = collect_automatic_migration_skips(&preflight);

    if !blockers.is_empty() {
        let report = MigrationAutoReport {
            generated_at: Utc::now().to_rfc3339(),
            runtime: runtime.clone(),
            source_mode: migration_source_mode(source).to_string(),
            source_database: prepared.logical_name.clone(),
            source_schema_filter: schema_filter.map(str::to_string),
            target_database: target_database.map(str::to_string),
            success: false,
            summary: MigrationAutoSummary {
                supported_tables: preflight.summary.supported_tables,
                partial_tables: preflight.summary.partial_tables,
                blocked_tables: preflight.summary.unsupported_tables
                    + preflight.summary.ddl_blocked_tables,
                verification_passed: false,
            },
            blockers: blockers.clone(),
            skipped_objects,
            workflow: None,
        };
        write_migration_auto_artifacts(repo_root, &report)?;
        return Err(anyhow!(
            "automatic migration is blocked for source '{}' because {} unsafe compatibility issues remain",
            prepared.logical_name,
            blockers.len()
        ));
    }

    let workflow = run_migration_workflow_for_source(
        repo_root,
        runtime,
        &prepared.logical_name,
        &prepared.postgres_database,
        schema_filter,
        target_database,
    )
    .await?;
    let success = workflow.summary.planned_tables == workflow.summary.verified_tables
        && workflow.summary.planned_tables == workflow.summary.row_count_matches
        && workflow.summary.planned_tables == workflow.summary.sample_matches;
    let report = MigrationAutoReport {
        generated_at: Utc::now().to_rfc3339(),
        runtime: runtime.clone(),
        source_mode: migration_source_mode(source).to_string(),
        source_database: prepared.logical_name.clone(),
        source_schema_filter: schema_filter.map(str::to_string),
        target_database: Some(workflow.target_database.clone()),
        success,
        summary: MigrationAutoSummary {
            supported_tables: workflow.preflight.summary.supported_tables,
            partial_tables: workflow.preflight.summary.partial_tables,
            blocked_tables: workflow.preflight.summary.unsupported_tables
                + workflow.preflight.summary.ddl_blocked_tables,
            verification_passed: success,
        },
        blockers: Vec::new(),
        skipped_objects,
        workflow: Some(workflow),
    };
    write_migration_auto_artifacts(repo_root, &report)?;
    Ok(report)
}

async fn build_migration_preflight_report(
    client: &Client,
    runtime: &RuntimeConfig,
    database: &str,
    schema_filter: Option<&str>,
) -> Result<MigrationPreflightReport> {
    let tables = introspect_pg_tables(client, schema_filter).await?;
    let columns = introspect_pg_columns(client, schema_filter).await?;
    let constraints = introspect_pg_constraints(client, schema_filter).await?;
    let indexes = introspect_pg_indexes(client, schema_filter).await?;
    let namespace_collisions = detect_table_name_collisions(&tables);

    let mut findings = Vec::new();
    let mut table_reports = Vec::new();

    for table in tables {
        let mut table_report = MigrationTableReport {
            schema_name: table.schema_name.clone(),
            table_name: table.table_name.clone(),
            status: "supported".into(),
            translation_status: "ready".into(),
            columns: Vec::new(),
            constraints: Vec::new(),
            indexes: Vec::new(),
            notes: Vec::new(),
            translation_notes: Vec::new(),
            translated_ddl: None,
        };

        for column in columns.iter().filter(|column| {
            column.schema_name == table.schema_name && column.table_name == table.table_name
        }) {
            let column_report =
                analyze_pg_column(&table.schema_name, &table.table_name, column, &mut findings);
            escalate_status(&mut table_report.status, &column_report.status);
            table_report.columns.push(column_report);
        }

        for constraint in constraints.iter().filter(|constraint| {
            constraint.schema_name == table.schema_name && constraint.table_name == table.table_name
        }) {
            if is_synthesized_not_null_check(constraint) {
                continue;
            }
            let constraint_report = analyze_pg_constraint(
                &table.schema_name,
                &table.table_name,
                constraint,
                &mut findings,
            );
            escalate_status(&mut table_report.status, &constraint_report.status);
            table_report.constraints.push(constraint_report);
        }

        for index in indexes.iter().filter(|index| {
            index.schema_name == table.schema_name && index.table_name == table.table_name
        }) {
            let index_report =
                analyze_pg_index(&table.schema_name, &table.table_name, index, &mut findings);
            escalate_status(&mut table_report.status, &index_report.status);
            table_report.indexes.push(index_report);
        }

        if table_report
            .columns
            .iter()
            .all(|column| column.status == "supported")
            && table_report.constraints.is_empty()
        {
            table_report
                .notes
                .push("table shape is directly compatible with the current Snowdrop subset".into());
        } else if table_report.status == "partial" {
            table_report.notes.push(
                "table needs manual review for partial features before a production migration"
                    .into(),
            );
        } else if table_report.status == "unsupported" {
            table_report.notes.push(
                "table contains unsupported PostgreSQL features for the current Snowdrop roadmap"
                    .into(),
            );
        }

        if let Some(schemas) = namespace_collisions.get(&table.table_name) {
            table_report.status = "unsupported".into();
            table_report.translation_status = "blocked".into();
            table_report.translated_ddl = None;
            table_report.translation_notes.push(format!(
                "Snowdrop uses a flat target namespace, but table name '{}' also appears in schemas: {}",
                table.table_name,
                schemas.join(", ")
            ));
            findings.push(MigrationFinding {
                status: "unsupported".into(),
                object_kind: "table".into(),
                object_name: format!("{}.{}", table.schema_name, table.table_name),
                category: "namespace".into(),
                message: "table name collides with another source schema in the flattened Snowdrop namespace".into(),
                recommendation:
                    "migrate one schema at a time or rename the colliding tables before using the automatic workflow"
                        .into(),
            });
        } else {
            let (translated_ddl, translation_status, translation_notes) =
                build_table_translation_preview(&table_report);
            table_report.translated_ddl = translated_ddl;
            table_report.translation_status = translation_status;
            table_report.translation_notes = translation_notes;
        }

        table_reports.push(table_report);
    }

    let load_plan = order_tables_for_load(&table_reports);
    for table_idx in load_plan.cyclic {
        if let Some(table_report) = table_reports.get_mut(table_idx) {
            if table_report.translated_ddl.is_some() {
                escalate_status(&mut table_report.status, "partial");
                if table_report.translation_status != "blocked" {
                    table_report.translation_status = "partial".into();
                }
                table_report.notes.push(
                    "table participates in a foreign-key dependency cycle; automatic load ordering will require manual intervention".into(),
                );
                findings.push(MigrationFinding {
                    status: "partial".into(),
                    object_kind: "table".into(),
                    object_name: format!(
                        "{}.{}",
                        table_report.schema_name, table_report.table_name
                    ),
                    category: "load-order".into(),
                    message:
                        "table participates in a foreign-key dependency cycle, so automatic table ordering is not currently available".into(),
                    recommendation:
                        "break the cycle for migration, stage data manually, or defer these tables until cycle-aware loading exists".into(),
                });
            }
        }
    }

    let mut findings_by_status = BTreeMap::new();
    for finding in &findings {
        *findings_by_status
            .entry(finding.status.clone())
            .or_insert(0) += 1;
    }

    let summary = MigrationSummary {
        table_count: table_reports.len(),
        column_count: table_reports.iter().map(|table| table.columns.len()).sum(),
        constraint_count: table_reports
            .iter()
            .map(|table| table.constraints.len())
            .sum(),
        index_count: table_reports.iter().map(|table| table.indexes.len()).sum(),
        supported_tables: table_reports
            .iter()
            .filter(|table| table.status == "supported")
            .count(),
        partial_tables: table_reports
            .iter()
            .filter(|table| table.status == "partial")
            .count(),
        unsupported_tables: table_reports
            .iter()
            .filter(|table| table.status == "unsupported")
            .count(),
        ddl_ready_tables: table_reports
            .iter()
            .filter(|table| table.translation_status == "ready")
            .count(),
        ddl_partial_tables: table_reports
            .iter()
            .filter(|table| table.translation_status == "partial")
            .count(),
        ddl_blocked_tables: table_reports
            .iter()
            .filter(|table| table.translation_status == "blocked")
            .count(),
        findings_by_status,
    };

    let (unsupported_inventory, partial_inventory) = build_finding_inventory(&findings);

    Ok(MigrationPreflightReport {
        generated_at: Utc::now().to_rfc3339(),
        runtime: runtime.clone(),
        source_database: database.to_string(),
        source_schema_filter: schema_filter.map(str::to_string),
        summary,
        tables: table_reports,
        findings,
        unsupported_inventory,
        partial_inventory,
    })
}

fn build_finding_inventory(
    findings: &[MigrationFinding],
) -> (BTreeMap<String, Vec<String>>, BTreeMap<String, Vec<String>>) {
    let mut unsupported = BTreeMap::new();
    let mut partial = BTreeMap::new();
    for finding in findings {
        let target = match finding.status.as_str() {
            "unsupported" => &mut unsupported,
            "partial" => &mut partial,
            _ => continue,
        };
        target
            .entry(finding.category.clone())
            .or_insert_with(Vec::new)
            .push(format!(
                "{} {} -> {}",
                finding.object_kind, finding.object_name, finding.message
            ));
    }
    (unsupported, partial)
}

fn detect_table_name_collisions(tables: &[PgTable]) -> BTreeMap<String, Vec<String>> {
    let mut by_name: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for table in tables {
        by_name
            .entry(table.table_name.clone())
            .or_default()
            .insert(table.schema_name.clone());
    }
    by_name
        .into_iter()
        .filter_map(|(table_name, schemas)| {
            if schemas.len() > 1 {
                Some((table_name, schemas.into_iter().collect()))
            } else {
                None
            }
        })
        .collect()
}

async fn introspect_pg_tables(
    client: &Client,
    schema_filter: Option<&str>,
) -> Result<Vec<PgTable>> {
    let rows = if let Some(schema) = schema_filter {
        client
            .query(
                "SELECT table_schema, table_name
                 FROM information_schema.tables
                 WHERE table_type = 'BASE TABLE'
                   AND table_schema = $1
                 ORDER BY table_schema, table_name",
                &[&schema],
            )
            .await?
    } else {
        client
            .query(
                "SELECT table_schema, table_name
                 FROM information_schema.tables
                 WHERE table_type = 'BASE TABLE'
                   AND table_schema NOT IN ('pg_catalog', 'information_schema')
                 ORDER BY table_schema, table_name",
                &[],
            )
            .await?
    };
    Ok(rows
        .into_iter()
        .map(|row| PgTable {
            schema_name: row.get("table_schema"),
            table_name: row.get("table_name"),
        })
        .collect())
}

async fn introspect_pg_columns(
    client: &Client,
    schema_filter: Option<&str>,
) -> Result<Vec<PgColumn>> {
    let rows = if let Some(schema) = schema_filter {
        client
            .query(
                "SELECT table_schema, table_name, column_name, ordinal_position, is_nullable,
                        data_type, udt_name, character_maximum_length, numeric_precision,
                        numeric_scale, datetime_precision, column_default
                 FROM information_schema.columns
                 WHERE table_schema = $1
                 ORDER BY table_schema, table_name, ordinal_position",
                &[&schema],
            )
            .await?
    } else {
        client
            .query(
                "SELECT table_schema, table_name, column_name, ordinal_position, is_nullable,
                        data_type, udt_name, character_maximum_length, numeric_precision,
                        numeric_scale, datetime_precision, column_default
                 FROM information_schema.columns
                 WHERE table_schema NOT IN ('pg_catalog', 'information_schema')
                 ORDER BY table_schema, table_name, ordinal_position",
                &[],
            )
            .await?
    };

    Ok(rows
        .into_iter()
        .map(|row| PgColumn {
            schema_name: row.get("table_schema"),
            table_name: row.get("table_name"),
            column_name: row.get("column_name"),
            nullable: row
                .get::<_, String>("is_nullable")
                .eq_ignore_ascii_case("YES"),
            data_type: row.get("data_type"),
            udt_name: row.get("udt_name"),
            char_length: row.get("character_maximum_length"),
            numeric_precision: row.get("numeric_precision"),
            numeric_scale: row.get("numeric_scale"),
            datetime_precision: row.get("datetime_precision"),
            default: row.get("column_default"),
        })
        .collect())
}

async fn introspect_pg_constraints(
    client: &Client,
    schema_filter: Option<&str>,
) -> Result<Vec<PgConstraint>> {
    let rows = if let Some(schema) = schema_filter {
        client
            .query(
                "SELECT tc.table_schema,
                        tc.table_name,
                        tc.constraint_name,
                        tc.constraint_type,
                        kcu.column_name,
                        kcu.ordinal_position,
                        ccu.table_schema AS foreign_table_schema,
                        ccu.table_name AS foreign_table_name,
                        ccu.column_name AS foreign_column_name,
                        rc.update_rule,
                        rc.delete_rule
                 FROM information_schema.table_constraints tc
                 LEFT JOIN information_schema.key_column_usage kcu
                   ON tc.constraint_catalog = kcu.constraint_catalog
                  AND tc.constraint_schema = kcu.constraint_schema
                  AND tc.constraint_name = kcu.constraint_name
                  AND tc.table_schema = kcu.table_schema
                  AND tc.table_name = kcu.table_name
                 LEFT JOIN information_schema.referential_constraints rc
                   ON tc.constraint_catalog = rc.constraint_catalog
                  AND tc.constraint_schema = rc.constraint_schema
                  AND tc.constraint_name = rc.constraint_name
                 LEFT JOIN information_schema.constraint_column_usage ccu
                   ON rc.unique_constraint_catalog = ccu.constraint_catalog
                  AND rc.unique_constraint_schema = ccu.constraint_schema
                  AND rc.unique_constraint_name = ccu.constraint_name
                 WHERE tc.table_schema = $1
                 ORDER BY tc.table_schema, tc.table_name, tc.constraint_name, kcu.ordinal_position, ccu.column_name",
                &[&schema],
            )
            .await?
    } else {
        client
            .query(
                "SELECT tc.table_schema,
                        tc.table_name,
                        tc.constraint_name,
                        tc.constraint_type,
                        kcu.column_name,
                        kcu.ordinal_position,
                        ccu.table_schema AS foreign_table_schema,
                        ccu.table_name AS foreign_table_name,
                        ccu.column_name AS foreign_column_name,
                        rc.update_rule,
                        rc.delete_rule
                 FROM information_schema.table_constraints tc
                 LEFT JOIN information_schema.key_column_usage kcu
                   ON tc.constraint_catalog = kcu.constraint_catalog
                  AND tc.constraint_schema = kcu.constraint_schema
                  AND tc.constraint_name = kcu.constraint_name
                  AND tc.table_schema = kcu.table_schema
                  AND tc.table_name = kcu.table_name
                 LEFT JOIN information_schema.referential_constraints rc
                   ON tc.constraint_catalog = rc.constraint_catalog
                  AND tc.constraint_schema = rc.constraint_schema
                  AND tc.constraint_name = rc.constraint_name
                 LEFT JOIN information_schema.constraint_column_usage ccu
                   ON rc.unique_constraint_catalog = ccu.constraint_catalog
                  AND rc.unique_constraint_schema = ccu.constraint_schema
                  AND rc.unique_constraint_name = ccu.constraint_name
                 WHERE tc.table_schema NOT IN ('pg_catalog', 'information_schema')
                 ORDER BY tc.table_schema, tc.table_name, tc.constraint_name, kcu.ordinal_position, ccu.column_name",
                &[],
            )
            .await?
    };

    let mut grouped: BTreeMap<(String, String, String), PgConstraint> = BTreeMap::new();
    for row in rows {
        let schema_name: String = row.get("table_schema");
        let table_name: String = row.get("table_name");
        let constraint_name: String = row.get("constraint_name");
        let key = (
            schema_name.clone(),
            table_name.clone(),
            constraint_name.clone(),
        );
        let entry = grouped.entry(key).or_insert_with(|| PgConstraint {
            schema_name: schema_name.clone(),
            table_name: table_name.clone(),
            constraint_name: constraint_name.clone(),
            constraint_type: row.get("constraint_type"),
            columns: Vec::new(),
            ref_schema_name: row.get("foreign_table_schema"),
            ref_table_name: row.get("foreign_table_name"),
            ref_columns: Vec::new(),
            update_rule: row.get("update_rule"),
            delete_rule: row.get("delete_rule"),
            check_clause: None,
        });
        let column_name: Option<String> = row.get("column_name");
        if let Some(column_name) = column_name {
            if !entry.columns.contains(&column_name) {
                entry.columns.push(column_name);
            }
        }
        let foreign_column_name: Option<String> = row.get("foreign_column_name");
        if let Some(foreign_column_name) = foreign_column_name {
            if !entry.ref_columns.contains(&foreign_column_name) {
                entry.ref_columns.push(foreign_column_name);
            }
        }
    }

    let check_rows = if let Some(schema) = schema_filter {
        client
            .query(
                "SELECT tc.table_schema, tc.table_name, tc.constraint_name, cc.check_clause
                 FROM information_schema.table_constraints tc
                 JOIN information_schema.check_constraints cc
                   ON tc.constraint_catalog = cc.constraint_catalog
                  AND tc.constraint_schema = cc.constraint_schema
                  AND tc.constraint_name = cc.constraint_name
                 WHERE tc.constraint_type = 'CHECK'
                   AND tc.table_schema = $1
                 ORDER BY tc.table_schema, tc.table_name, tc.constraint_name",
                &[&schema],
            )
            .await?
    } else {
        client
            .query(
                "SELECT tc.table_schema, tc.table_name, tc.constraint_name, cc.check_clause
                 FROM information_schema.table_constraints tc
                 JOIN information_schema.check_constraints cc
                   ON tc.constraint_catalog = cc.constraint_catalog
                  AND tc.constraint_schema = cc.constraint_schema
                  AND tc.constraint_name = cc.constraint_name
                 WHERE tc.constraint_type = 'CHECK'
                   AND tc.table_schema NOT IN ('pg_catalog', 'information_schema')
                 ORDER BY tc.table_schema, tc.table_name, tc.constraint_name",
                &[],
            )
            .await?
    };

    for row in check_rows {
        let schema_name: String = row.get("table_schema");
        let table_name: String = row.get("table_name");
        let constraint_name: String = row.get("constraint_name");
        let key = (
            schema_name.clone(),
            table_name.clone(),
            constraint_name.clone(),
        );
        grouped
            .entry(key)
            .and_modify(|constraint| {
                constraint.check_clause = Some(row.get("check_clause"));
            })
            .or_insert_with(|| PgConstraint {
                schema_name,
                table_name,
                constraint_name,
                constraint_type: "CHECK".into(),
                columns: Vec::new(),
                ref_schema_name: None,
                ref_table_name: None,
                ref_columns: Vec::new(),
                update_rule: None,
                delete_rule: None,
                check_clause: Some(row.get("check_clause")),
            });
    }

    Ok(grouped.into_values().collect())
}

async fn introspect_pg_indexes(
    client: &Client,
    schema_filter: Option<&str>,
) -> Result<Vec<PgIndex>> {
    let rows = if let Some(schema) = schema_filter {
        client
            .query(
                "SELECT schemaname, tablename, indexname, indexdef
                 FROM pg_indexes
                 WHERE schemaname = $1
                 ORDER BY schemaname, tablename, indexname",
                &[&schema],
            )
            .await?
    } else {
        client
            .query(
                "SELECT schemaname, tablename, indexname, indexdef
                 FROM pg_indexes
                 WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
                 ORDER BY schemaname, tablename, indexname",
                &[],
            )
            .await?
    };
    Ok(rows
        .into_iter()
        .map(|row| PgIndex {
            schema_name: row.get("schemaname"),
            table_name: row.get("tablename"),
            index_name: row.get("indexname"),
            index_def: row.get("indexdef"),
        })
        .collect())
}

fn analyze_pg_column(
    schema_name: &str,
    table_name: &str,
    column: &PgColumn,
    findings: &mut Vec<MigrationFinding>,
) -> MigrationColumnReport {
    let mut status = "supported".to_string();
    let mut notes = Vec::new();
    let (snowdrop_type, type_status, type_notes) = map_postgres_type(column);
    escalate_status(&mut status, type_status);
    notes.extend(type_notes);

    let default_status = classify_default(column.default.as_deref());
    if let Some((default_level, default_message, default_recommendation)) = default_status {
        escalate_status(&mut status, default_level);
        notes.push(default_message.clone());
        findings.push(MigrationFinding {
            status: default_level.to_string(),
            object_kind: "column".into(),
            object_name: format!("{}.{}.{}", schema_name, table_name, column.column_name),
            category: "default".into(),
            message: default_message,
            recommendation: default_recommendation,
        });
    }

    if status != "supported" {
        findings.push(MigrationFinding {
            status: status.clone(),
            object_kind: "column".into(),
            object_name: format!("{}.{}.{}", schema_name, table_name, column.column_name),
            category: "type".into(),
            message: format!(
                "column uses PostgreSQL type '{}' which maps to the Snowdrop status '{}'",
                format_pg_type(column),
                status
            ),
            recommendation: match status.as_str() {
                "unsupported" => {
                    "rewrite this column to a supported Snowdrop type or exclude it from automatic migration".into()
                }
                _ => "review this column during migration and confirm the translated semantics are acceptable".into(),
            },
        });
    }

    MigrationColumnReport {
        column_name: column.column_name.clone(),
        source_type: format_pg_type(column),
        snowdrop_type,
        nullable: column.nullable,
        default: column.default.clone(),
        status,
        notes,
    }
}

fn map_postgres_type(column: &PgColumn) -> (Option<String>, &'static str, Vec<String>) {
    let data_type = column.data_type.to_lowercase();
    let udt_name = column.udt_name.to_lowercase();
    let mut notes = Vec::new();
    match (data_type.as_str(), udt_name.as_str()) {
        ("smallint", "int2") => (Some("SMALLINT".into()), "supported", notes),
        ("integer", "int4") => (Some("INT".into()), "supported", notes),
        ("bigint", "int8") => (Some("BIGINT".into()), "supported", notes),
        ("real", "float4") => {
            notes.push("PostgreSQL REAL is mapped onto Snowdrop FLOAT".into());
            (Some("FLOAT".into()), "partial", notes)
        }
        ("double precision", "float8") => (Some("DOUBLE".into()), "supported", notes),
        ("numeric", _) | ("decimal", _) => match (column.numeric_precision, column.numeric_scale) {
            (Some(precision), Some(scale))
                if (0..=255).contains(&precision) && (0..=255).contains(&scale) =>
            {
                (
                    Some(format!("NUMERIC({precision},{scale})")),
                    "supported",
                    notes,
                )
            }
            (Some(precision), None) if (0..=255).contains(&precision) => {
                notes.push(
                    "numeric scale is omitted in PostgreSQL metadata; Snowdrop can store the column but the exact intended scale should be reviewed".into(),
                );
                (Some(format!("NUMERIC({precision},0)")), "partial", notes)
            }
            _ => {
                notes.push(
                    "unbounded or high-precision NUMERIC/DECIMAL needs manual normalization before migration".into(),
                );
                (None, "partial", notes)
            }
        },
        ("character varying", "varchar") => match column.char_length {
            Some(length) if length > 0 => (Some(format!("VARCHAR({length})")), "supported", notes),
            _ => {
                notes.push(
                    "lengthless VARCHAR should be normalized to TEXT or an explicit bound".into(),
                );
                (Some("TEXT".into()), "partial", notes)
            }
        },
        ("character", "bpchar") => match column.char_length {
            Some(length) if length > 0 => (Some(format!("CHAR({length})")), "supported", notes),
            _ => (Some("CHAR(1)".into()), "partial", notes),
        },
        ("text", "text") => (Some("TEXT".into()), "supported", notes),
        ("boolean", "bool") => (Some("BOOLEAN".into()), "supported", notes),
        ("date", "date") => (Some("DATE".into()), "supported", notes),
        ("timestamp without time zone", "timestamp") | ("timestamp", "timestamp") => {
            if let Some(precision) = column.datetime_precision {
                if precision > 6 {
                    notes.push(
                        "timestamp precision above microseconds will be truncated to the current Snowdrop practical slice".into(),
                    );
                    return (Some("TIMESTAMP".into()), "partial", notes);
                }
            }
            (Some("TIMESTAMP".into()), "supported", notes)
        }
        ("time without time zone", "time") | ("time", "time") => {
            notes.push(
                "Snowdrop currently has practical runtime TIME support but no first-class schema TIME type yet".into(),
            );
            (None, "unsupported", notes)
        }
        ("timestamp with time zone", "timestamptz") => {
            notes.push(
                "timezone-aware timestamps are not part of the current Snowdrop schema surface"
                    .into(),
            );
            (None, "unsupported", notes)
        }
        ("time with time zone", "timetz") => {
            notes.push(
                "timezone-aware time types are not part of the current Snowdrop schema surface"
                    .into(),
            );
            (None, "unsupported", notes)
        }
        (_, "uuid") => {
            notes.push("UUID is still unsupported in the current Snowdrop roadmap".into());
            (None, "unsupported", notes)
        }
        (_, "json") | (_, "jsonb") => {
            notes.push("JSON / JSONB are still unsupported in the current Snowdrop roadmap".into());
            (None, "unsupported", notes)
        }
        (_, "bytea") => {
            notes.push("BYTEA is not supported by the current Snowdrop type layer".into());
            (None, "unsupported", notes)
        }
        (_, "interval") => {
            notes.push("INTERVAL is outside the current Snowdrop temporal subset".into());
            (None, "unsupported", notes)
        }
        _ if data_type.contains("array") || udt_name.starts_with('_') => {
            notes.push("array types are not supported by the current Snowdrop type layer".into());
            (None, "unsupported", notes)
        }
        _ => {
            notes.push(format!(
                "type '{}' is not mapped by the current Snowdrop migration preflight",
                format_pg_type(column)
            ));
            (None, "unsupported", notes)
        }
    }
}

fn format_pg_type(column: &PgColumn) -> String {
    let data_type = column.data_type.to_lowercase();
    match data_type.as_str() {
        "character varying" => column
            .char_length
            .map(|length| format!("varchar({length})"))
            .unwrap_or_else(|| "varchar".into()),
        "character" => column
            .char_length
            .map(|length| format!("char({length})"))
            .unwrap_or_else(|| "char".into()),
        "numeric" | "decimal" => match (column.numeric_precision, column.numeric_scale) {
            (Some(precision), Some(scale)) => format!("{}({precision},{scale})", column.data_type),
            (Some(precision), None) => format!("{}({precision})", column.data_type),
            _ => column.data_type.clone(),
        },
        _ => column.data_type.clone(),
    }
}

fn classify_default(default: Option<&str>) -> Option<(&'static str, String, String)> {
    let Some(default) = default.map(str::trim).filter(|value| !value.is_empty()) else {
        return None;
    };
    let lower = default.to_lowercase();
    if lower.contains("nextval(") {
        return Some((
            "partial",
            "sequence-backed defaults need manual handling because Snowdrop does not expose PostgreSQL serial/sequence behavior directly".into(),
            "rewrite this default into explicit application-managed keys or a Snowdrop-compatible loading strategy".into(),
        ));
    }
    if lower.contains("now()")
        || lower.contains("current_timestamp")
        || lower.contains("clock_timestamp()")
        || lower.contains("uuid_generate")
        || lower.contains("gen_random_uuid")
    {
        return Some((
            "partial",
            "function-backed default expression needs manual review before migration".into(),
            "replace this default with a supported literal or add an application-side default during migration".into(),
        ));
    }
    None
}

fn normalize_preview_expression(expr: &str) -> String {
    let mut normalized = expr.trim().to_string();
    for cast in [
        "::timestamp without time zone",
        "::timestamp with time zone",
        "::character varying",
        "::double precision",
        "::smallint",
        "::integer",
        "::bigint",
        "::numeric",
        "::decimal",
        "::boolean",
        "::varchar",
        "::text",
        "::date",
        "::time",
    ] {
        normalized = normalized.replace(cast, "");
    }
    normalized
}

fn analyze_pg_constraint(
    schema_name: &str,
    table_name: &str,
    constraint: &PgConstraint,
    findings: &mut Vec<MigrationFinding>,
) -> MigrationConstraintReport {
    let kind = constraint.constraint_type.to_uppercase();
    let mut status = "supported".to_string();
    let mut notes = Vec::new();
    let mut reference = None;

    match kind.as_str() {
        "PRIMARY KEY" => {
            if constraint.columns.len() > 1 {
                notes.push(
                    "composite primary keys are supported in the current narrow Phase 4 slice"
                        .into(),
                );
            }
        }
        "UNIQUE" => {
            if constraint.columns.len() > 1 {
                notes.push(
                    "multi-column UNIQUE is supported in the current narrow Phase 4 slice".into(),
                );
            }
        }
        "FOREIGN KEY" => {
            let delete_rule = constraint
                .delete_rule
                .clone()
                .unwrap_or_else(|| "NO ACTION".into());
            let update_rule = constraint
                .update_rule
                .clone()
                .unwrap_or_else(|| "NO ACTION".into());
            let supported_rule = matches!(delete_rule.as_str(), "NO ACTION" | "RESTRICT")
                && matches!(update_rule.as_str(), "NO ACTION" | "RESTRICT");
            if supported_rule {
                if constraint.columns.len() > 1 {
                    notes.push(
                        "composite-key foreign keys are only supported inside the current narrow Phase 4 subset and should be validated carefully".into(),
                    );
                    escalate_status(&mut status, "partial");
                }
            } else {
                notes.push(format!(
                    "foreign-key action uses delete_rule='{}' update_rule='{}', which is outside the current Snowdrop subset",
                    delete_rule, update_rule
                ));
                status = "unsupported".into();
            }
            reference = match (&constraint.ref_schema_name, &constraint.ref_table_name) {
                (Some(ref_schema), Some(ref_table)) => Some(format!(
                    "{}.{}({})",
                    ref_schema,
                    ref_table,
                    constraint.ref_columns.join(", ")
                )),
                _ => None,
            };
        }
        "CHECK" => {
            notes.push(
                "CHECK constraints are supported only for the current safe row-local scalar subset"
                    .into(),
            );
            status = "partial".into();
            if let Some(check_clause) = constraint.check_clause.as_deref() {
                let clause = check_clause.to_lowercase();
                if clause.contains("select ") {
                    notes.push("subqueries inside CHECK are unsupported".into());
                    status = "unsupported".into();
                }
            }
        }
        _ => {
            status = "unsupported".into();
            notes.push(format!(
                "constraint type '{}' is not implemented in the current Snowdrop roadmap",
                constraint.constraint_type
            ));
        }
    }

    if status != "supported" {
        findings.push(MigrationFinding {
            status: status.clone(),
            object_kind: "constraint".into(),
            object_name: format!("{}.{}.{}", schema_name, table_name, constraint.constraint_name),
            category: "constraint".into(),
            message: format!(
                "{} constraint needs manual review during migration",
                constraint.constraint_type
            ),
            recommendation: match status.as_str() {
                "unsupported" => {
                    "rewrite or drop this constraint before using the automatic migration path".into()
                }
                _ => {
                    "validate this constraint against the current Snowdrop subset before applying it automatically".into()
                }
            },
        });
    }

    MigrationConstraintReport {
        name: constraint.constraint_name.clone(),
        kind: constraint.constraint_type.clone(),
        columns: constraint.columns.clone(),
        status,
        reference,
        delete_rule: constraint.delete_rule.clone(),
        update_rule: constraint.update_rule.clone(),
        check_clause: constraint.check_clause.clone(),
        notes,
    }
}

fn is_synthesized_not_null_check(constraint: &PgConstraint) -> bool {
    if !constraint.constraint_type.eq_ignore_ascii_case("CHECK") {
        return false;
    }

    let Some(check_clause) = constraint.check_clause.as_deref() else {
        return false;
    };

    constraint.constraint_name.ends_with("_not_null")
        && check_clause.to_lowercase().contains(" is not null")
}

fn analyze_pg_index(
    schema_name: &str,
    table_name: &str,
    index: &PgIndex,
    findings: &mut Vec<MigrationFinding>,
) -> MigrationIndexReport {
    let mut status = "partial".to_string();
    let mut notes = vec![
        "secondary-index compatibility is still narrower than PostgreSQL and should be recreated selectively".into(),
    ];
    let lower = index.index_def.to_lowercase();
    if lower.contains(" using gin ")
        || lower.contains(" using gist ")
        || lower.contains(" using brin ")
        || lower.contains(" using hash ")
    {
        status = "unsupported".into();
        notes.push(
            "index uses a PostgreSQL-specific access method outside the current Snowdrop subset"
                .into(),
        );
    }
    findings.push(MigrationFinding {
        status: status.clone(),
        object_kind: "index".into(),
        object_name: format!("{}.{}.{}", schema_name, table_name, index.index_name),
        category: "index".into(),
        message: "index definition needs review during migration".into(),
        recommendation: match status.as_str() {
            "unsupported" => "recreate this index manually with a Snowdrop-supported index strategy".into(),
            _ => "review whether this index is required for the first migration cut and recreate it after load if needed".into(),
        },
    });
    MigrationIndexReport {
        name: index.index_name.clone(),
        definition: index.index_def.clone(),
        status,
        notes,
    }
}

fn build_table_translation_preview(
    table: &MigrationTableReport,
) -> (Option<String>, String, Vec<String>) {
    let mut translation_status = "ready".to_string();
    let mut translation_notes = Vec::new();
    let mut lines = Vec::new();
    let mut column_lines = Vec::new();
    let mut trailing_items = Vec::new();

    for column in &table.columns {
        let Some(snowdrop_type) = column.snowdrop_type.as_deref() else {
            translation_status = "blocked".into();
            translation_notes.push(format!(
                "column '{}' has no safe Snowdrop type mapping yet",
                column.column_name
            ));
            continue;
        };
        let mut line = format!("  {} {}", sql_ident(&column.column_name), snowdrop_type);
        if !column.nullable {
            line.push_str(" NOT NULL");
        }
        if let Some(default) = column.default.as_deref() {
            if classify_default(Some(default)).is_none() {
                line.push_str(" DEFAULT ");
                line.push_str(&normalize_preview_expression(default));
            } else {
                escalate_status(&mut translation_status, "partial");
                translation_notes.push(format!(
                    "default for column '{}' needs manual rewrite and is omitted from preview",
                    column.column_name
                ));
            }
        }
        if column.status == "partial" {
            escalate_status(&mut translation_status, "partial");
        }
        column_lines.push(line);
    }

    if translation_status == "blocked" {
        return (None, translation_status, translation_notes);
    }

    for constraint in &table.constraints {
        match constraint.kind.to_uppercase().as_str() {
            "PRIMARY KEY" => {
                trailing_items.push(format!(
                    "  PRIMARY KEY ({})",
                    constraint
                        .columns
                        .iter()
                        .map(|column| sql_ident(column))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            "UNIQUE" => {
                trailing_items.push(format!(
                    "  UNIQUE ({})",
                    constraint
                        .columns
                        .iter()
                        .map(|column| sql_ident(column))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                if constraint.status == "partial" {
                    escalate_status(&mut translation_status, "partial");
                }
            }
            "FOREIGN KEY" => {
                if constraint.status == "unsupported" {
                    escalate_status(&mut translation_status, "partial");
                    translation_notes.push(format!(
                        "foreign key '{}' is omitted because its action semantics are outside the current Snowdrop subset",
                        constraint.name
                    ));
                    continue;
                }
                let Some(reference) = constraint.reference.as_deref() else {
                    escalate_status(&mut translation_status, "partial");
                    translation_notes.push(format!(
                        "foreign key '{}' is omitted because its referenced table metadata is incomplete",
                        constraint.name
                    ));
                    continue;
                };
                let (ref_table, ref_columns) = parse_reference_target(reference);
                trailing_items.push(format!(
                    "  FOREIGN KEY ({}) REFERENCES {} ({})",
                    constraint
                        .columns
                        .iter()
                        .map(|column| sql_ident(column))
                        .collect::<Vec<_>>()
                        .join(", "),
                    sql_ident(ref_table),
                    ref_columns
                        .iter()
                        .map(|column| sql_ident(column))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                if constraint.status == "partial" {
                    escalate_status(&mut translation_status, "partial");
                    translation_notes.push(format!(
                        "foreign key '{}' should be rechecked after load because it relies on the narrow Phase 4 subset",
                        constraint.name
                    ));
                }
            }
            "CHECK" => {
                if constraint.status == "unsupported" {
                    escalate_status(&mut translation_status, "partial");
                    translation_notes.push(format!(
                        "check constraint '{}' is omitted from the preview because it is outside the current safe subset",
                        constraint.name
                    ));
                    continue;
                }
                let Some(check_clause) = constraint.check_clause.as_deref() else {
                    escalate_status(&mut translation_status, "partial");
                    translation_notes.push(format!(
                        "check constraint '{}' is omitted from the preview because its expression text is unavailable",
                        constraint.name
                    ));
                    continue;
                };
                trailing_items.push(format!(
                    "  CHECK ({})",
                    normalize_preview_expression(check_clause)
                ));
            }
            other => {
                escalate_status(&mut translation_status, "partial");
                translation_notes.push(format!(
                    "constraint '{}' of type '{}' is omitted from the preview",
                    constraint.name, other
                ));
            }
        }
    }

    if !table.indexes.is_empty() {
        escalate_status(&mut translation_status, "partial");
        for index in &table.indexes {
            translation_notes.push(format!(
                "index '{}' is omitted from the preview and should be recreated manually if still needed",
                index.name
            ));
        }
    }

    lines.push(format!(
        "-- Source table {}.{}",
        table.schema_name, table.table_name
    ));
    lines.push(format!("CREATE TABLE {} (", sql_ident(&table.table_name)));
    let mut body = column_lines;
    body.extend(trailing_items);
    lines.push(body.join(",\n"));
    lines.push(");".into());

    if !translation_notes.is_empty() {
        lines.push(String::new());
        for note in &translation_notes {
            lines.push(format!("-- NOTE: {}", note));
        }
    }

    (
        Some(lines.join("\n")),
        translation_status,
        translation_notes,
    )
}

fn collect_automatic_migration_blockers(
    report: &MigrationPreflightReport,
) -> Vec<MigrationFinding> {
    let mut blockers = report
        .findings
        .iter()
        .filter(|finding| finding.status == "unsupported")
        .cloned()
        .collect::<Vec<_>>();

    for table in &report.tables {
        if table.translation_status == "blocked" || table.translated_ddl.is_none() {
            blockers.push(MigrationFinding {
                status: "unsupported".into(),
                object_kind: "table".into(),
                object_name: format!("{}.{}", table.schema_name, table.table_name),
                category: "translation".into(),
                message: "table has no safe automatic Snowdrop DDL translation".into(),
                recommendation:
                    "rewrite the table shape into the supported subset before using migration-auto"
                        .into(),
            });
            continue;
        }

        for note in &table.translation_notes {
            if !automatic_migration_partial_note_is_safe(note) {
                blockers.push(MigrationFinding {
                    status: "unsupported".into(),
                    object_kind: "table".into(),
                    object_name: format!("{}.{}", table.schema_name, table.table_name),
                    category: "translation".into(),
                    message: note.clone(),
                    recommendation:
                        "remove this manual follow-up before relying on the automatic migration path"
                            .into(),
                });
            }
        }
    }

    blockers
}

fn collect_automatic_migration_skips(report: &MigrationPreflightReport) -> Vec<String> {
    let mut skipped = Vec::new();
    for table in &report.tables {
        for note in &table.translation_notes {
            if automatic_migration_partial_note_is_safe(note) {
                skipped.push(format!(
                    "{}.{} -> {}",
                    table.schema_name, table.table_name, note
                ));
            }
        }
    }
    skipped
}

fn automatic_migration_partial_note_is_safe(note: &str) -> bool {
    note.contains("index '") && note.contains("omitted from the preview")
}

fn parse_reference_target(reference: &str) -> (&str, Vec<&str>) {
    let mut table_part = reference;
    let mut columns = Vec::new();
    if let Some(open_idx) = reference.find('(') {
        table_part = reference[..open_idx].trim();
        if let Some(close_idx) = reference.rfind(')') {
            columns = reference[open_idx + 1..close_idx]
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .collect();
        }
    }
    let table_name = table_part
        .split('.')
        .next_back()
        .unwrap_or(table_part)
        .trim();
    (table_name, columns)
}

fn sql_ident(name: &str) -> String {
    let escaped = name.replace('"', "\"\"");
    format!("\"{}\"", escaped)
}

fn escalate_status(current: &mut String, candidate: impl AsRef<str>) {
    let rank = |status: &str| match status {
        "unsupported" => 2,
        "partial" => 1,
        _ => 0,
    };
    let candidate = candidate.as_ref();
    if rank(candidate) > rank(current.as_str()) {
        *current = candidate.to_string();
    }
}

fn write_migration_preflight_artifacts(
    repo_root: &Path,
    report: &MigrationPreflightReport,
) -> Result<()> {
    let output_root = repo_root.join(MIGRATION_REPORT_DIR);
    fs::create_dir_all(&output_root)?;
    let slug = migration_report_slug(
        &report.source_database,
        report.source_schema_filter.as_deref(),
    );
    let json_path = output_root.join(format!("{slug}.json"));
    let markdown_path = output_root.join(format!("{slug}.md"));
    let sql_path = output_root.join(format!("{slug}.sql"));
    fs::write(&json_path, serde_json::to_string_pretty(report)?)?;
    fs::write(&markdown_path, render_migration_preflight_report(report))?;
    fs::write(&sql_path, render_migration_translation_sql(report))?;
    println!("Wrote {}", json_path.display());
    println!("Wrote {}", markdown_path.display());
    println!("Wrote {}", sql_path.display());
    Ok(())
}

fn migration_report_slug(database: &str, schema_filter: Option<&str>) -> String {
    let mut slug = sanitize_report_slug(database);
    if let Some(schema) = schema_filter {
        slug.push('-');
        slug.push_str(&sanitize_report_slug(schema));
    }
    slug.push_str("-preflight");
    slug
}

fn sanitize_report_slug(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_lowercase()
}

fn render_migration_preflight_report(report: &MigrationPreflightReport) -> String {
    let mut out = String::new();
    out.push_str("# Snowdrop Migration Preflight\n\n");
    out.push_str(&format!("Generated at: `{}`\n\n", report.generated_at));
    out.push_str(&format!(
        "- Source PostgreSQL database: `{}`\n",
        report.source_database
    ));
    if let Some(schema) = report.source_schema_filter.as_deref() {
        out.push_str(&format!("- Schema filter: `{}`\n", schema));
    } else {
        out.push_str("- Schema filter: `all non-system schemas`\n");
    }
    out.push('\n');

    out.push_str("## Summary\n\n");
    out.push_str("| Metric | Value |\n");
    out.push_str("|---|---|\n");
    out.push_str(&format!("| Tables | {} |\n", report.summary.table_count));
    out.push_str(&format!("| Columns | {} |\n", report.summary.column_count));
    out.push_str(&format!(
        "| Constraints | {} |\n",
        report.summary.constraint_count
    ));
    out.push_str(&format!("| Indexes | {} |\n", report.summary.index_count));
    out.push_str(&format!(
        "| Supported tables | {} |\n",
        report.summary.supported_tables
    ));
    out.push_str(&format!(
        "| Partial tables | {} |\n",
        report.summary.partial_tables
    ));
    out.push_str(&format!(
        "| Unsupported tables | {} |\n\n",
        report.summary.unsupported_tables
    ));
    out.push_str(&format!(
        "| DDL-ready tables | {} |\n",
        report.summary.ddl_ready_tables
    ));
    out.push_str(&format!(
        "| DDL-partial tables | {} |\n",
        report.summary.ddl_partial_tables
    ));
    out.push_str(&format!(
        "| DDL-blocked tables | {} |\n\n",
        report.summary.ddl_blocked_tables
    ));

    out.push_str("## Findings By Status\n\n");
    out.push_str("| Status | Count |\n");
    out.push_str("|---|---|\n");
    for (status, count) in &report.summary.findings_by_status {
        out.push_str(&format!("| {} | {} |\n", status, count));
    }
    out.push('\n');

    out.push_str("## Table Compatibility\n\n");
    out.push_str("| Schema | Table | Status | Translation | Columns | Constraints | Indexes |\n");
    out.push_str("|---|---|---|---|---|---|---|\n");
    for table in &report.tables {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            table.schema_name,
            table.table_name,
            table.status,
            table.translation_status,
            table.columns.len(),
            table.constraints.len(),
            table.indexes.len()
        ));
    }
    out.push('\n');

    if !report.findings.is_empty() {
        out.push_str("## Compatibility Findings\n\n");
        out.push_str("| Status | Object | Category | Message | Recommendation |\n");
        out.push_str("|---|---|---|---|---|\n");
        for finding in &report.findings {
            out.push_str(&format!(
                "| {} | {} `{}` | {} | {} | {} |\n",
                finding.status,
                finding.object_kind,
                sanitize_table_cell(&finding.object_name),
                finding.category,
                sanitize_table_cell(&finding.message),
                sanitize_table_cell(&finding.recommendation)
            ));
        }
        out.push('\n');
    }

    if !report.unsupported_inventory.is_empty() {
        out.push_str("## Unsupported Feature Inventory\n\n");
        for (category, entries) in &report.unsupported_inventory {
            out.push_str(&format!("### {}\n\n", category));
            for entry in entries {
                out.push_str(&format!("- {}\n", entry));
            }
            out.push('\n');
        }
    }

    if !report.partial_inventory.is_empty() {
        out.push_str("## Partial Feature Inventory\n\n");
        for (category, entries) in &report.partial_inventory {
            out.push_str(&format!("### {}\n\n", category));
            for entry in entries {
                out.push_str(&format!("- {}\n", entry));
            }
            out.push('\n');
        }
    }

    for table in &report.tables {
        out.push_str(&format!(
            "## Table `{}`.`{}`\n\n",
            table.schema_name, table.table_name
        ));
        out.push_str(&format!("- Overall status: `{}`\n", table.status));
        out.push_str(&format!(
            "- Translation preview status: `{}`\n",
            table.translation_status
        ));
        for note in &table.notes {
            out.push_str(&format!("- {}\n", note));
        }
        for note in &table.translation_notes {
            out.push_str(&format!("- Translation note: {}\n", note));
        }
        out.push('\n');

        out.push_str("### Columns\n\n");
        out.push_str("| Column | Source type | Snowdrop type | Nullable | Status | Notes |\n");
        out.push_str("|---|---|---|---|---|---|\n");
        for column in &table.columns {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} |\n",
                column.column_name,
                sanitize_table_cell(&column.source_type),
                column.snowdrop_type.as_deref().unwrap_or("-"),
                if column.nullable { "yes" } else { "no" },
                column.status,
                sanitize_table_cell(&column.notes.join("; "))
            ));
        }
        out.push('\n');

        if !table.constraints.is_empty() {
            out.push_str("### Constraints\n\n");
            out.push_str("| Name | Kind | Columns | Status | Reference | Notes |\n");
            out.push_str("|---|---|---|---|---|---|\n");
            for constraint in &table.constraints {
                out.push_str(&format!(
                    "| {} | {} | {} | {} | {} | {} |\n",
                    constraint.name,
                    constraint.kind,
                    sanitize_table_cell(&constraint.columns.join(", ")),
                    constraint.status,
                    constraint.reference.as_deref().unwrap_or("-"),
                    sanitize_table_cell(&constraint.notes.join("; "))
                ));
            }
            out.push('\n');
        }

        if !table.indexes.is_empty() {
            out.push_str("### Indexes\n\n");
            out.push_str("| Name | Status | Definition |\n");
            out.push_str("|---|---|---|\n");
            for index in &table.indexes {
                out.push_str(&format!(
                    "| {} | {} | {} |\n",
                    index.name,
                    index.status,
                    sanitize_table_cell(&index.definition)
                ));
            }
            out.push('\n');
        }

        if let Some(ddl) = table.translated_ddl.as_deref() {
            out.push_str("### Translation Preview\n\n");
            out.push_str("```sql\n");
            out.push_str(ddl);
            out.push_str("\n```\n\n");
        }
    }

    out
}

fn render_migration_translation_sql(report: &MigrationPreflightReport) -> String {
    let mut out = String::new();
    out.push_str("-- Snowdrop migration translation preview\n");
    out.push_str(&format!(
        "-- Source PostgreSQL database: {}\n",
        report.source_database
    ));
    if let Some(schema) = report.source_schema_filter.as_deref() {
        out.push_str(&format!("-- Source schema filter: {}\n", schema));
    }
    out.push('\n');

    for table in &report.tables {
        if let Some(ddl) = table.translated_ddl.as_deref() {
            out.push_str(ddl);
            out.push_str("\n\n");
        } else {
            out.push_str(&format!(
                "-- Skipped table {}.{} because no safe DDL preview is currently available.\n\n",
                table.schema_name, table.table_name
            ));
        }
    }

    out
}

fn migration_target_database(source_database: &str, explicit: Option<&str>) -> Result<String> {
    let candidate = explicit
        .map(str::to_string)
        .unwrap_or_else(|| default_target_database(source_database));
    if candidate.is_empty() {
        return Err(anyhow!("target database name cannot be empty"));
    }
    if !candidate
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err(anyhow!(
            "target database name '{}' must contain only ASCII letters, digits, or underscores",
            candidate
        ));
    }
    Ok(candidate.to_lowercase())
}

fn default_target_database(source_database: &str) -> String {
    let mut value = sanitize_report_slug(source_database).replace('-', "_");
    if value.is_empty() {
        value = "migration".into();
    }
    if value
        .chars()
        .next()
        .map(|ch| ch.is_ascii_digit())
        .unwrap_or(false)
    {
        value.insert_str(0, "m_");
    }
    value.push_str("_snowdrop");
    value
}

fn order_tables_for_load(tables: &[MigrationTableReport]) -> LoadOrderPlan {
    let mut translated: BTreeMap<String, usize> = BTreeMap::new();
    for (idx, table) in tables.iter().enumerate() {
        if table.translated_ddl.is_some() {
            translated.insert(table.table_name.clone(), idx);
        }
    }

    let mut dependencies: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (idx, table) in tables.iter().enumerate() {
        let deps = dependencies.entry(idx).or_default();
        for constraint in &table.constraints {
            if !constraint.kind.eq_ignore_ascii_case("FOREIGN KEY") {
                continue;
            }
            let Some(reference) = constraint.reference.as_deref() else {
                continue;
            };
            let (ref_table, _) = parse_reference_target(reference);
            if let Some(dep_idx) = translated.get(ref_table) {
                if *dep_idx != idx {
                    deps.insert(*dep_idx);
                }
            }
        }
    }

    let mut remaining: BTreeSet<usize> = (0..tables.len()).collect();
    let mut ordered = Vec::with_capacity(tables.len());
    let mut cyclic = Vec::new();

    while !remaining.is_empty() {
        let ready: Vec<usize> = remaining
            .iter()
            .copied()
            .filter(|idx| {
                dependencies
                    .get(idx)
                    .map(|deps| deps.iter().all(|dep| ordered.contains(dep)))
                    .unwrap_or(true)
            })
            .collect();

        if ready.is_empty() {
            cyclic.extend(remaining.iter().copied());
            break;
        }

        for idx in ready {
            remaining.remove(&idx);
            ordered.push(idx);
        }
    }

    LoadOrderPlan { ordered, cyclic }
}

async fn load_table_rows(
    postgres: &Client,
    snowdrop: &Client,
    table: &MigrationTableReport,
) -> Result<i64> {
    let rows = export_table_rows(postgres, table).await?;
    let column_names: Vec<&str> = table
        .columns
        .iter()
        .map(|column| column.column_name.as_str())
        .collect();

    for chunk in rows.chunks(1000) {
        if chunk.is_empty() {
            continue;
        }
        let values = chunk
            .iter()
            .map(|row| {
                let literals = table
                    .columns
                    .iter()
                    .map(|column| {
                        let text = row
                            .get(&column.column_name)
                            .and_then(|value| value.as_deref());
                        snowdrop_literal(text, column.snowdrop_type.as_deref())
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({literals})")
            })
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "INSERT INTO {} ({}) VALUES {};",
            sql_ident(&table.table_name),
            column_names
                .iter()
                .map(|name| sql_ident(name))
                .collect::<Vec<_>>()
                .join(", "),
            values
        );
        snowdrop
            .simple_query(&sql)
            .await
            .map_err(|err| anyhow!(format_db_error(&err)))?;
    }

    Ok(rows.len() as i64)
}

async fn export_table_rows(
    postgres: &Client,
    table: &MigrationTableReport,
) -> Result<Vec<BTreeMap<String, Option<String>>>> {
    let object_args = table
        .columns
        .iter()
        .map(|column| {
            let ident = format!(
                "{}.{}",
                sql_ident(&table.table_name),
                sql_ident(&column.column_name)
            );
            format!(
                "'{}', CASE WHEN {} IS NULL THEN NULL ELSE {}::text END",
                column.column_name.replace('\'', "''"),
                ident,
                ident
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT jsonb_build_object({})::text AS row_json FROM {} {};",
        object_args,
        source_table_ident(table),
        sql_ident(&table.table_name)
    );
    let rows = raw_query_rows(postgres, &sql)
        .await
        .map_err(|err| anyhow!(err))?;
    let mut exported = Vec::with_capacity(rows.len());
    for row in rows {
        let payload = row
            .first()
            .and_then(|value| value.as_deref())
            .ok_or_else(|| anyhow!("PostgreSQL export returned an empty JSON payload"))?;
        let parsed: serde_json::Value = serde_json::from_str(payload)?;
        let object = parsed
            .as_object()
            .ok_or_else(|| anyhow!("PostgreSQL export row is not a JSON object"))?;
        let mut values = BTreeMap::new();
        for column in &table.columns {
            let text = object
                .get(&column.column_name)
                .and_then(|value| match value {
                    serde_json::Value::Null => None,
                    serde_json::Value::String(text) => Some(text.clone()),
                    other => Some(other.to_string()),
                });
            values.insert(column.column_name.clone(), text);
        }
        exported.push(values);
    }
    Ok(exported)
}

async fn verify_table_migration(
    postgres: &Client,
    snowdrop: &Client,
    table: &MigrationTableReport,
) -> Result<(
    i64,
    bool,
    bool,
    Option<bool>,
    Vec<MigrationReplayCheck>,
    Vec<String>,
)> {
    let source_count = query_count(
        postgres,
        &format!("SELECT COUNT(*) FROM {};", source_table_ident(table)),
    )
    .await?;
    let target_count = query_count(
        snowdrop,
        &format!("SELECT COUNT(*) FROM {};", sql_ident(&table.table_name)),
    )
    .await?;

    let projection = table
        .columns
        .iter()
        .map(|column| sql_ident(&column.column_name))
        .collect::<Vec<_>>()
        .join(", ");
    let order_by = verification_order_columns(table)
        .into_iter()
        .map(|column| sql_ident(&column))
        .collect::<Vec<_>>()
        .join(", ");
    let sample_limit = 25usize;
    let source_sample_sql = format!(
        "SELECT {} FROM {} ORDER BY {} LIMIT {};",
        projection,
        source_table_ident(table),
        order_by,
        sample_limit
    );
    let target_sample_sql = format!(
        "SELECT {} FROM {} ORDER BY {} LIMIT {};",
        projection,
        sql_ident(&table.table_name),
        order_by,
        sample_limit
    );
    let source_sample = execute_query(postgres, &source_sample_sql)
        .await
        .map_err(|err| anyhow!(err))?;
    let target_sample = execute_query(snowdrop, &target_sample_sql)
        .await
        .map_err(|err| anyhow!(err))?;
    let normalized_source_sample = normalize_rows(source_sample.clone());
    let normalized_target_sample = normalize_rows(target_sample.clone());

    let aggregate_match = verify_table_aggregates(postgres, snowdrop, table).await?;
    let aggregate_sql = if table.columns.iter().any(|column| {
        column
            .snowdrop_type
            .as_deref()
            .map(is_numeric_type)
            .unwrap_or(false)
    }) {
        Some((
            build_numeric_aggregate_sql(table, true),
            build_numeric_aggregate_sql(table, false),
        ))
    } else {
        None
    };
    let mut notes = Vec::new();
    if source_count != target_count {
        notes.push(format!(
            "row-count mismatch: source={} target={}",
            source_count, target_count
        ));
    }
    if normalized_source_sample != normalized_target_sample {
        notes.push("sample row verification diverged on the ordered first 25 rows".into());
    }
    if aggregate_match == Some(false) {
        notes.push("aggregate verification diverged for at least one numeric column".into());
    }

    let mut replay_checks = vec![
        MigrationReplayCheck {
            name: "row_count".into(),
            source_sql: format!("SELECT COUNT(*) FROM {};", source_table_ident(table)),
            target_sql: format!("SELECT COUNT(*) FROM {};", sql_ident(&table.table_name)),
            matched: Some(source_count == target_count),
            note: None,
        },
        MigrationReplayCheck {
            name: "ordered_sample".into(),
            source_sql: source_sample_sql,
            target_sql: target_sample_sql,
            matched: Some(normalized_source_sample == normalized_target_sample),
            note: None,
        },
    ];
    if let Some((source_aggregate_sql, target_aggregate_sql)) = aggregate_sql {
        replay_checks.push(MigrationReplayCheck {
            name: "numeric_aggregates".into(),
            source_sql: source_aggregate_sql,
            target_sql: target_aggregate_sql,
            matched: aggregate_match,
            note: if aggregate_match.is_none() {
                Some("table has no numeric columns, so aggregate replay was skipped".into())
            } else {
                None
            },
        });
    }

    Ok((
        target_count,
        source_count == target_count,
        normalized_source_sample == normalized_target_sample,
        aggregate_match,
        replay_checks,
        notes,
    ))
}

async fn verify_table_aggregates(
    postgres: &Client,
    snowdrop: &Client,
    table: &MigrationTableReport,
) -> Result<Option<bool>> {
    let numeric_columns: Vec<&MigrationColumnReport> = table
        .columns
        .iter()
        .filter(|column| {
            column
                .snowdrop_type
                .as_deref()
                .map(is_numeric_type)
                .unwrap_or(false)
        })
        .collect();
    if numeric_columns.is_empty() {
        return Ok(None);
    }

    let mut projections = vec!["COUNT(*)".to_string()];
    for column in numeric_columns {
        let ident = sql_ident(&column.column_name);
        projections.push(format!("COUNT({})", ident));
        projections.push(format!("SUM({})", ident));
    }
    let projection_sql = projections.join(", ");
    let source_sql = format!(
        "SELECT {} FROM {};",
        projection_sql,
        source_table_ident(table)
    );
    let target_sql = format!(
        "SELECT {} FROM {};",
        projection_sql,
        sql_ident(&table.table_name)
    );
    let source = execute_query(postgres, &source_sql)
        .await
        .map_err(|err| anyhow!(err))?;
    let target = execute_query(snowdrop, &target_sql)
        .await
        .map_err(|err| anyhow!(err))?;
    Ok(Some(normalize_rows(source) == normalize_rows(target)))
}

fn build_numeric_aggregate_sql(table: &MigrationTableReport, source_side: bool) -> String {
    let mut projections = vec!["COUNT(*)".to_string()];
    for column in table.columns.iter().filter(|column| {
        column
            .snowdrop_type
            .as_deref()
            .map(is_numeric_type)
            .unwrap_or(false)
    }) {
        let ident = sql_ident(&column.column_name);
        projections.push(format!("COUNT({})", ident));
        projections.push(format!("SUM({})", ident));
    }
    let table_sql = if source_side {
        source_table_ident(table)
    } else {
        sql_ident(&table.table_name)
    };
    format!("SELECT {} FROM {};", projections.join(", "), table_sql)
}

fn verification_order_columns(table: &MigrationTableReport) -> Vec<String> {
    for constraint in &table.constraints {
        if constraint.kind.eq_ignore_ascii_case("PRIMARY KEY") && !constraint.columns.is_empty() {
            return constraint.columns.clone();
        }
    }
    for constraint in &table.constraints {
        if constraint.kind.eq_ignore_ascii_case("UNIQUE") && !constraint.columns.is_empty() {
            return constraint.columns.clone();
        }
    }
    table
        .columns
        .iter()
        .map(|column| column.column_name.clone())
        .collect()
}

fn source_table_ident(table: &MigrationTableReport) -> String {
    format!(
        "{}.{}",
        sql_ident(&table.schema_name),
        sql_ident(&table.table_name)
    )
}

fn snowdrop_literal(value: Option<&str>, snowdrop_type: Option<&str>) -> String {
    let Some(value) = value else {
        return "NULL".into();
    };
    let kind = snowdrop_type.unwrap_or("TEXT").to_uppercase();
    if is_numeric_type(&kind) {
        return value.to_string();
    }
    if kind == "BOOLEAN" {
        return if value.eq_ignore_ascii_case("true") {
            "TRUE".into()
        } else {
            "FALSE".into()
        };
    }
    sql_string_literal(value)
}

fn is_numeric_type(kind: &str) -> bool {
    matches!(
        kind,
        "SMALLINT" | "INT" | "INTEGER" | "BIGINT" | "FLOAT" | "DOUBLE"
    ) || kind.starts_with("NUMERIC(")
        || kind.starts_with("DECIMAL(")
}

fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

async fn query_count(client: &Client, sql: &str) -> Result<i64> {
    let rows = execute_query(client, sql)
        .await
        .map_err(|err| anyhow!(err))?;
    let raw = rows
        .first()
        .and_then(|row| row.first())
        .ok_or_else(|| anyhow!("count query returned no rows"))?;
    raw.parse::<i64>()
        .with_context(|| format!("could not parse COUNT(*) result '{}'", raw))
}

async fn raw_query_rows(
    client: &Client,
    sql: &str,
) -> std::result::Result<Vec<Vec<Option<String>>>, String> {
    match client.simple_query(sql).await {
        Ok(messages) => Ok(messages
            .iter()
            .filter_map(|msg| match msg {
                SimpleQueryMessage::Row(row) => Some(
                    (0..row.len())
                        .map(|idx| row.get(idx).map(str::to_string))
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .collect()),
        Err(err) => Err(format_db_error(&err)),
    }
}

fn write_migration_workflow_artifacts(
    repo_root: &Path,
    report: &MigrationWorkflowReport,
) -> Result<()> {
    let output_root = repo_root.join(MIGRATION_REPORT_DIR);
    fs::create_dir_all(&output_root)?;
    let slug = migration_workflow_slug(
        &report.source_database,
        report.source_schema_filter.as_deref(),
        &report.target_database,
    );
    let json_path = output_root.join(format!("{slug}.json"));
    let markdown_path = output_root.join(format!("{slug}.md"));
    fs::write(&json_path, serde_json::to_string_pretty(report)?)?;
    fs::write(&markdown_path, render_migration_workflow_report(report))?;
    println!("Wrote {}", json_path.display());
    println!("Wrote {}", markdown_path.display());
    Ok(())
}

fn migration_workflow_slug(
    source_database: &str,
    schema_filter: Option<&str>,
    target_database: &str,
) -> String {
    let mut slug = sanitize_report_slug(source_database);
    if let Some(schema) = schema_filter {
        slug.push('-');
        slug.push_str(&sanitize_report_slug(schema));
    }
    slug.push('-');
    slug.push_str(&sanitize_report_slug(target_database));
    slug.push_str("-workflow");
    slug
}

fn render_migration_workflow_report(report: &MigrationWorkflowReport) -> String {
    let mut out = String::new();
    out.push_str("# Snowdrop Migration Workflow\n\n");
    out.push_str(&format!("Generated at: `{}`\n\n", report.generated_at));
    out.push_str(&format!(
        "- Source PostgreSQL database: `{}`\n- Target Snowdrop database: `{}`\n\n",
        report.source_database, report.target_database
    ));
    out.push_str("## Summary\n\n");
    out.push_str("| Metric | Value |\n|---|---|\n");
    out.push_str(&format!(
        "| Planned tables | {} |\n",
        report.summary.planned_tables
    ));
    out.push_str(&format!(
        "| Blocked tables | {} |\n",
        report.summary.blocked_tables
    ));
    out.push_str(&format!(
        "| DDL applied tables | {} |\n",
        report.summary.ddl_applied_tables
    ));
    out.push_str(&format!(
        "| Loaded tables | {} |\n",
        report.summary.loaded_tables
    ));
    out.push_str(&format!(
        "| Verified tables | {} |\n",
        report.summary.verified_tables
    ));
    out.push_str(&format!(
        "| Row-count matches | {} |\n",
        report.summary.row_count_matches
    ));
    out.push_str(&format!(
        "| Sample matches | {} |\n",
        report.summary.sample_matches
    ));
    out.push_str(&format!(
        "| Aggregate matches | {} |\n\n",
        report.summary.aggregate_matches
    ));
    out.push_str(&format!(
        "| Replay queries run | {} |\n",
        report.summary.replay_queries_run
    ));
    out.push_str(&format!(
        "| Replay queries matched | {} |\n\n",
        report.summary.replay_queries_matched
    ));

    out.push_str("## Table Runs\n\n");
    out.push_str("| Schema | Table | Translation | Load status | Source rows | Target rows | Count match | Sample match | Aggregate match |\n");
    out.push_str("|---|---|---|---|---|---|---|---|---|\n");
    for run in &report.table_runs {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            run.schema_name,
            run.table_name,
            run.translation_status,
            run.load_status,
            run.source_row_count
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".into()),
            run.target_row_count
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".into()),
            run.row_count_match
                .map(|value| if value { "yes" } else { "no" })
                .unwrap_or("-"),
            run.sample_rows_match
                .map(|value| if value { "yes" } else { "no" })
                .unwrap_or("-"),
            run.aggregate_match
                .map(|value| if value { "yes" } else { "no" })
                .unwrap_or("-")
        ));
    }
    out.push('\n');

    for run in &report.table_runs {
        out.push_str(&format!(
            "## Table `{}`.`{}`\n\n",
            run.schema_name, run.table_name
        ));
        out.push_str(&format!(
            "- Translation status: `{}`\n- Load status: `{}`\n- DDL applied: `{}`\n",
            run.translation_status,
            run.load_status,
            if run.ddl_applied { "yes" } else { "no" }
        ));
        if let Some(value) = run.source_row_count {
            out.push_str(&format!("- Source row count: `{}`\n", value));
        }
        if let Some(value) = run.target_row_count {
            out.push_str(&format!("- Target row count: `{}`\n", value));
        }
        for note in &run.notes {
            out.push_str(&format!("- {}\n", note));
        }
        out.push('\n');

        if !run.replay_checks.is_empty() {
            out.push_str("### Replay Checks\n\n");
            out.push_str("| Name | Matched | Source SQL | Target SQL | Note |\n");
            out.push_str("|---|---|---|---|---|\n");
            for check in &run.replay_checks {
                out.push_str(&format!(
                    "| {} | {} | {} | {} | {} |\n",
                    check.name,
                    check
                        .matched
                        .map(|value| if value { "yes" } else { "no" })
                        .unwrap_or("-"),
                    sanitize_table_cell(&check.source_sql),
                    sanitize_table_cell(&check.target_sql),
                    sanitize_table_cell(check.note.as_deref().unwrap_or("-"))
                ));
            }
            out.push('\n');
        }
    }

    out
}

fn write_migration_auto_artifacts(repo_root: &Path, report: &MigrationAutoReport) -> Result<()> {
    let output_root = repo_root.join(MIGRATION_REPORT_DIR);
    fs::create_dir_all(&output_root)?;
    let slug = migration_auto_slug(
        &report.source_database,
        report.source_schema_filter.as_deref(),
        report.target_database.as_deref(),
    );
    let json_path = output_root.join(format!("{slug}.json"));
    let markdown_path = output_root.join(format!("{slug}.md"));
    fs::write(&json_path, serde_json::to_string_pretty(report)?)?;
    fs::write(&markdown_path, render_migration_auto_report(report))?;
    println!("Wrote {}", json_path.display());
    println!("Wrote {}", markdown_path.display());
    Ok(())
}

fn migration_auto_slug(
    source_database: &str,
    schema_filter: Option<&str>,
    target_database: Option<&str>,
) -> String {
    let mut slug = sanitize_report_slug(source_database);
    if let Some(schema) = schema_filter {
        slug.push('-');
        slug.push_str(&sanitize_report_slug(schema));
    }
    if let Some(target_database) = target_database {
        slug.push('-');
        slug.push_str(&sanitize_report_slug(target_database));
    }
    slug.push_str("-auto");
    slug
}

fn render_migration_auto_report(report: &MigrationAutoReport) -> String {
    let mut out = String::new();
    out.push_str("# Snowdrop Automatic Migration\n\n");
    out.push_str(&format!("Generated at: `{}`\n\n", report.generated_at));
    out.push_str(&format!(
        "- Source mode: `{}`\n- Source: `{}`\n",
        report.source_mode, report.source_database
    ));
    if let Some(schema) = report.source_schema_filter.as_deref() {
        out.push_str(&format!("- Source schema filter: `{}`\n", schema));
    }
    if let Some(target) = report.target_database.as_deref() {
        out.push_str(&format!("- Target Snowdrop database: `{}`\n", target));
    }
    out.push('\n');
    out.push_str("## Summary\n\n");
    out.push_str("| Metric | Value |\n|---|---|\n");
    out.push_str(&format!("| Success | `{}` |\n", report.success));
    out.push_str(&format!(
        "| Supported tables | {} |\n",
        report.summary.supported_tables
    ));
    out.push_str(&format!(
        "| Partial tables | {} |\n",
        report.summary.partial_tables
    ));
    out.push_str(&format!(
        "| Blocked tables | {} |\n",
        report.summary.blocked_tables
    ));
    out.push_str(&format!(
        "| Verification passed | `{}` |\n\n",
        report.summary.verification_passed
    ));

    if !report.blockers.is_empty() {
        out.push_str("## Blockers\n\n");
        for blocker in &report.blockers {
            out.push_str(&format!(
                "- `{}` {}: {}. Recommendation: {}\n",
                blocker.status, blocker.object_name, blocker.message, blocker.recommendation
            ));
        }
        out.push('\n');
    }

    if !report.skipped_objects.is_empty() {
        out.push_str("## Safe Omissions\n\n");
        for skipped in &report.skipped_objects {
            out.push_str(&format!("- {}\n", skipped));
        }
        out.push('\n');
    }

    if let Some(workflow) = report.workflow.as_ref() {
        out.push_str("## Workflow\n\n");
        out.push_str(&render_migration_workflow_report(workflow));
    }

    out
}

fn migration_source_mode(source: &MigrationSourceArg) -> &'static str {
    match source {
        MigrationSourceArg::LiveDatabase(_) => "live",
        MigrationSourceArg::DumpDir { .. } => "dump-dir",
    }
}

fn print_migration_preflight_summary(report: &MigrationPreflightReport) {
    println!("\n=== Snowdrop Migration Preflight ===");
    println!("database={}", report.source_database);
    if let Some(schema) = report.source_schema_filter.as_deref() {
        println!("schema={}", schema);
    }
    println!(
        "tables={} supported={} partial={} unsupported={}",
        report.summary.table_count,
        report.summary.supported_tables,
        report.summary.partial_tables,
        report.summary.unsupported_tables
    );
    println!(
        "columns={} constraints={} indexes={}",
        report.summary.column_count, report.summary.constraint_count, report.summary.index_count
    );
    if !report.findings.is_empty() {
        println!("\nTop findings:");
        for finding in report.findings.iter().take(10) {
            println!(
                "- [{}] {} {} -> {}",
                finding.status, finding.object_kind, finding.object_name, finding.message
            );
        }
        if report.findings.len() > 10 {
            println!("- ... and {} more findings", report.findings.len() - 10);
        }
    }
}

fn print_migration_workflow_summary(report: &MigrationWorkflowReport) {
    println!("\n=== Snowdrop Migration Workflow ===");
    println!("source_database={}", report.source_database);
    println!("target_database={}", report.target_database);
    if let Some(schema) = report.source_schema_filter.as_deref() {
        println!("schema={}", schema);
    }
    println!(
        "planned_tables={} blocked_tables={} ddl_applied_tables={} loaded_tables={} verified_tables={}",
        report.summary.planned_tables,
        report.summary.blocked_tables,
        report.summary.ddl_applied_tables,
        report.summary.loaded_tables,
        report.summary.verified_tables
    );
    println!(
        "row_count_matches={} sample_matches={} aggregate_matches={} replay_queries_run={} replay_queries_matched={}",
        report.summary.row_count_matches,
        report.summary.sample_matches,
        report.summary.aggregate_matches,
        report.summary.replay_queries_run,
        report.summary.replay_queries_matched
    );
    let non_green: Vec<&MigrationTableRun> = report
        .table_runs
        .iter()
        .filter(|run| {
            run.translation_status == "blocked"
                || !matches!(run.load_status.as_str(), "verified")
                || run.row_count_match == Some(false)
                || run.sample_rows_match == Some(false)
                || run.aggregate_match == Some(false)
        })
        .collect();
    if !non_green.is_empty() {
        println!("\nTables needing attention:");
        for run in non_green.iter().take(10) {
            println!(
                "- {}.{} [{} / {}]",
                run.schema_name, run.table_name, run.translation_status, run.load_status
            );
            for note in &run.notes {
                println!("  {}", note);
            }
        }
        if non_green.len() > 10 {
            println!("- ... and {} more tables", non_green.len() - 10);
        }
    }
}

fn print_migration_auto_summary(report: &MigrationAutoReport) {
    println!("\n=== Snowdrop Automatic Migration ===");
    println!("source_mode={}", report.source_mode);
    println!("source_database={}", report.source_database);
    if let Some(target_database) = report.target_database.as_deref() {
        println!("target_database={}", target_database);
    }
    if let Some(schema) = report.source_schema_filter.as_deref() {
        println!("schema={}", schema);
    }
    println!(
        "success={} supported_tables={} partial_tables={} blocked_tables={} verification_passed={}",
        report.success,
        report.summary.supported_tables,
        report.summary.partial_tables,
        report.summary.blocked_tables,
        report.summary.verification_passed
    );
    if !report.blockers.is_empty() {
        println!("\nAutomatic migration blockers:");
        for blocker in report.blockers.iter().take(10) {
            println!(
                "- [{}] {} {} -> {}",
                blocker.status, blocker.object_kind, blocker.object_name, blocker.message
            );
        }
        if report.blockers.len() > 10 {
            println!("- ... and {} more blockers", report.blockers.len() - 10);
        }
    }
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

async fn execute_query(
    client: &Client,
    sql: &str,
) -> std::result::Result<Vec<Vec<String>>, String> {
    match client.simple_query(sql).await {
        Ok(messages) => Ok(messages
            .iter()
            .filter_map(|msg| match msg {
                SimpleQueryMessage::Row(row) => Some(
                    (0..row.len())
                        .map(|idx| {
                            row.get(idx)
                                .map(normalize_cell)
                                .unwrap_or_else(|| "NULL".to_string())
                        })
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .collect()),
        Err(err) => Err(format_db_error(&err)),
    }
}

fn format_db_error(err: &tokio_postgres::Error) -> String {
    if let Some(db_err) = err.as_db_error() {
        return format!("{}: {}", db_err.severity(), db_err.message());
    }
    err.to_string()
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
    if raw.eq_ignore_ascii_case("true") || raw.eq_ignore_ascii_case("t") {
        return "TRUE".into();
    }
    if raw.eq_ignore_ascii_case("false") || raw.eq_ignore_ascii_case("f") {
        return "FALSE".into();
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
    [
        "unsupported",
        "not implemented",
        "only inner join",
        "not supported",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn looks_like_stale_phase3_runtime(message: &str) -> bool {
    let lower = message.to_lowercase();
    [
        "with / cte queries are not supported yet",
        "compound queries like union are not supported yet",
        "subqueries in select filters are not supported yet",
        "subqueries in projection expressions are not supported yet",
        "subqueries in having are not supported yet",
        "derived tables in from are not supported yet",
        "derived tables in join clauses are not supported yet",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn pass(
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
        status: "pass".into(),
        details: details.into(),
        expected: None,
        actual: None,
    }
}

fn fail(
    suite: &str,
    dataset: &str,
    group: impl Into<String>,
    case_name: &str,
    details: String,
) -> CaseOutcome {
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
    let report_path = repo_root.join(REPORT_PATH);
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&report_path, report)?;
    println!("Wrote {}", raw_path.display());
    println!("Wrote {}", REPORT_PATH);
    Ok(())
}

fn print_console_summary(results: &UnifiedResults) {
    let totals = summarize_totals(&results.suite_runs);
    println!("\n=== Snowdrop Test Summary ===");
    println!(
        "cases={} pass={} fail={} mismatch={} skip={}",
        totals.get("total").copied().unwrap_or(0),
        totals.get("pass").copied().unwrap_or(0),
        totals.get("fail").copied().unwrap_or(0),
        totals.get("mismatch").copied().unwrap_or(0),
        totals.get("skip").copied().unwrap_or(0),
    );

    println!("\nPer-suite:");
    for run in &results.suite_runs {
        let mut counts = BTreeMap::new();
        for case in &run.cases {
            *counts.entry(case.status.clone()).or_insert(0usize) += 1;
        }
        println!(
            "- {:<15} pass={} fail={} mismatch={} skip={}",
            prettify_suite_name(&run.suite),
            counts.get("pass").copied().unwrap_or(0),
            counts.get("fail").copied().unwrap_or(0),
            counts.get("mismatch").copied().unwrap_or(0),
            counts.get("skip").copied().unwrap_or(0),
        );
    }

    println!("\nFeature buckets:");
    for (group, counts) in summarize_by_group(&results.suite_runs) {
        println!(
            "- {:<32} pass={} fail={} mismatch={} skip={}",
            group,
            counts.get("pass").copied().unwrap_or(0),
            counts.get("fail").copied().unwrap_or(0),
            counts.get("mismatch").copied().unwrap_or(0),
            counts.get("skip").copied().unwrap_or(0),
        );
    }

    let notable_cases = results
        .suite_runs
        .iter()
        .flat_map(|run| run.cases.iter())
        .filter(|case| case.status != "pass")
        .collect::<Vec<_>>();
    if !notable_cases.is_empty() {
        println!("\nNon-pass cases:");
        for case in notable_cases {
            println!(
                "- {} :: {} :: {} -> {}",
                prettify_suite_name(&case.suite),
                case.group,
                case.case_name,
                case.details
            );
        }
    }

    if !results.tpch_results.is_empty() {
        println!("\nTPC-H benchmark (warm averages):");
        for (query, label) in [
            ("Q6", "Revenue from discounted shipments"),
            ("Q1", "Pricing summary"),
            ("Q14", "Promotion effect"),
            ("Q3", "Shipping priority"),
        ] {
            let sd = tpch_bench::warm_avg(&results.tpch_results, query, "Snowdrop", "baseline");
            let sdi = tpch_bench::warm_avg(&results.tpch_results, query, "Snowdrop", "indexed");
            let pg =
                tpch_bench::warm_avg(&results.tpch_results, query, "PostgreSQL 16", "baseline");
            let sd_str = if sd > 0.0 {
                format!("{sd:.1}ms")
            } else {
                "n/a".to_string()
            };
            let sdi_str = if sdi > 0.0 {
                format!("{sdi:.1}ms")
            } else {
                "n/a".to_string()
            };
            let pg_str = if pg > 0.0 {
                format!("{pg:.1}ms")
            } else {
                "n/a".to_string()
            };
            println!(
                "- {} ({}) -> Snowdrop {} | Indexed {} | PostgreSQL {}",
                query, label, sd_str, sdi_str, pg_str
            );
        }
    }
}

fn read_results(repo_root: &Path) -> Result<UnifiedResults> {
    let raw_path = repo_root.join(RAW_RESULTS_PATH);
    let legacy_raw_path = repo_root.join(LEGACY_RAW_RESULTS_PATH);
    let source_path = if raw_path.exists() {
        raw_path
    } else {
        legacy_raw_path
    };
    let raw = fs::read_to_string(&source_path).with_context(|| {
        format!(
            "could not read {} or legacy fallback {}",
            RAW_RESULTS_PATH, LEGACY_RAW_RESULTS_PATH
        )
    })?;
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
            if dataset.postgres_enabled {
                "yes"
            } else {
                "no"
            },
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

    out.push_str("## Phase 3 Feature Summary\n\n");
    out.push_str("| Status | Feature family |\n");
    out.push_str("|---|---|\n");
    for feature in [
        "HAVING over the grouped aggregate subset",
        "non-correlated subqueries in scalar, IN, and EXISTS forms",
        "correlated EXISTS, IN, and scalar subqueries in WHERE / HAVING",
        "derived tables in FROM and JOIN with required aliases",
        "non-recursive WITH / CTE chains",
        "UNION ALL and UNION",
        "broader INSERT ... SELECT over relation-producing query sources",
        "relation-level ORDER BY after subqueries, CTEs, derived tables, and UNION outputs",
    ] {
        out.push_str(&format!("| Supported | {} |\n", feature));
    }
    for feature in [
        "GROUP BY expressions beyond plain identifiers / qualified identifiers",
        "INSERT ... SELECT currently materializes source relations in memory before insert",
        "subqueries nested inside complex scalar function arguments remain intentionally narrow",
    ] {
        out.push_str(&format!("| Partial | {} |\n", feature));
    }
    for feature in [
        "recursive CTE",
        "INTERSECT",
        "EXCEPT",
        "LATERAL",
        "window functions",
    ] {
        out.push_str(&format!("| Explicitly unsupported | {} |\n", feature));
    }
    out.push('\n');

    for suite_name in [
        "golden",
        "differential",
        "sqllogictest",
        "tap",
        "postgres-style",
    ] {
        if let Some(run) = results
            .suite_runs
            .iter()
            .find(|run| run.suite == suite_name)
        {
            out.push_str(&format!(
                "## {} Results\n\n",
                prettify_suite_name(suite_name)
            ));
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
                if meta.with_indexes {
                    "enabled"
                } else {
                    "disabled"
                }
            )
        });
        if let Some(meta) = tpch_meta {
            out.push_str(&meta);
        }
        let scale = results
            .tpch_options
            .as_ref()
            .map(|meta| meta.scale)
            .unwrap_or(1);
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

fn summarize_by_dataset(
    suite_runs: &[SuiteRun],
) -> BTreeMap<(String, String), BTreeMap<String, usize>> {
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

fn summarize_by_group(suite_runs: &[SuiteRun]) -> BTreeMap<String, BTreeMap<String, usize>> {
    let mut map = BTreeMap::new();
    for run in suite_runs {
        for case in &run.cases {
            let counts = map.entry(case.group.clone()).or_insert_with(BTreeMap::new);
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
                        line.split_whitespace()
                            .map(normalize_cell)
                            .collect::<Vec<_>>()
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
        dataset: dataset
            .ok_or_else(|| anyhow!("missing '# dataset:' header in {}", path.display()))?,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tpch_bench::RunResult;

    fn unique_temp_dir(label: &str) -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("snowdrop-testkit-{label}-{ts}"))
    }

    fn sample_results(include_tpch: bool) -> UnifiedResults {
        UnifiedResults {
            generated_at: "2026-05-05T00:00:00Z".into(),
            runtime: RuntimeConfig {
                snowdrop_host: "orchestrator".into(),
                snowdrop_port: 5432,
                postgres_host: "postgres".into(),
                postgres_port: 5432,
                health_host: "orchestrator".into(),
                health_port: 8080,
                metrics_host: "orchestrator".into(),
                metrics_port: 9090,
            },
            datasets: vec![DatasetManifest {
                name: "tiny_smoke".into(),
                database: "tiny_smoke".into(),
                description: "test fixture".into(),
                purpose: "unit test".into(),
                tables: vec!["items".into()],
                suites: vec!["golden".into()],
                postgres_enabled: true,
            }],
            suite_runs: vec![SuiteRun {
                suite: "golden".into(),
                cases: vec![pass("golden", "tiny_smoke", "group", "case", "ok")],
            }],
            tpch_results: if include_tpch {
                vec![QueryResult {
                    query: "Q6".into(),
                    system: "Snowdrop".into(),
                    scale: 1,
                    phase: "baseline".into(),
                    runs: vec![
                        RunResult {
                            run: 1,
                            latency_ms: 10.0,
                            row_count: 1,
                        },
                        RunResult {
                            run: 2,
                            latency_ms: 1.0,
                            row_count: 1,
                        },
                        RunResult {
                            run: 3,
                            latency_ms: 1.2,
                            row_count: 1,
                        },
                    ],
                }]
            } else {
                Vec::new()
            },
            tpch_options: include_tpch.then_some(TpchRunMeta {
                scale: 1,
                runs: 3,
                seed: 42,
                with_indexes: true,
            }),
        }
    }

    #[test]
    fn write_results_artifacts_uses_reports_directory() {
        let repo_root = unique_temp_dir("write-results");
        fs::create_dir_all(&repo_root).unwrap();

        write_results_artifacts(&repo_root, &sample_results(false)).unwrap();

        assert!(repo_root.join(REPORT_PATH).exists());
        assert!(repo_root.join(RAW_RESULTS_PATH).exists());

        let report = fs::read_to_string(repo_root.join(REPORT_PATH)).unwrap();
        assert!(report.contains("# Snowdrop Test Report"));

        fs::remove_dir_all(repo_root).unwrap();
    }

    #[test]
    fn read_results_supports_legacy_raw_path() {
        let repo_root = unique_temp_dir("legacy-read");
        let legacy_path = repo_root.join(LEGACY_RAW_RESULTS_PATH);
        fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
        fs::write(
            &legacy_path,
            serde_json::to_string_pretty(&sample_results(false)).unwrap(),
        )
        .unwrap();

        let loaded = read_results(&repo_root).unwrap();
        assert_eq!(loaded.generated_at, "2026-05-05T00:00:00Z");
        assert_eq!(loaded.datasets.len(), 1);
        assert_eq!(loaded.suite_runs.len(), 1);

        fs::remove_dir_all(repo_root).unwrap();
    }

    #[test]
    fn render_report_includes_tpch_section_when_present() {
        let report = render_report(&sample_results(true));
        assert!(report.contains("## TPC-H Benchmark"));
        assert!(report.contains("Q6"));
        assert!(report.contains("Runs: `3`. Seed: `42`. Indexed phase: `enabled`."));
    }

    #[test]
    fn stale_runtime_detector_recognizes_old_phase3_boundaries() {
        assert!(looks_like_stale_phase3_runtime(
            "ERROR: WITH / CTE queries are not supported yet"
        ));
        assert!(looks_like_stale_phase3_runtime(
            "ERROR: subqueries in SELECT filters are not supported yet"
        ));
        assert!(looks_like_stale_phase3_runtime(
            "ERROR: compound queries like UNION are not supported yet"
        ));
        assert!(!looks_like_stale_phase3_runtime(
            "ERROR: recursive CTE is not supported yet"
        ));
    }

    #[test]
    fn parse_command_supports_migration_preflight() {
        let args = vec![
            "snowdrop-testkit".to_string(),
            "migration-preflight".to_string(),
            "--database".to_string(),
            "appdb".to_string(),
            "--schema".to_string(),
            "public".to_string(),
        ];
        match parse_command(&args).unwrap() {
            Command::MigrationPreflight { source, schema } => {
                assert_eq!(source, MigrationSourceArg::LiveDatabase("appdb".into()));
                assert_eq!(schema.as_deref(), Some("public"));
            }
            _ => panic!("expected migration preflight command"),
        }
    }

    #[test]
    fn parse_command_supports_migration_run() {
        let args = vec![
            "snowdrop-testkit".to_string(),
            "migration-run".to_string(),
            "--database".to_string(),
            "appdb".to_string(),
            "--schema".to_string(),
            "public".to_string(),
            "--target-database".to_string(),
            "appdb_snowdrop".to_string(),
        ];
        match parse_command(&args).unwrap() {
            Command::MigrationRun {
                source,
                schema,
                target_database,
            } => {
                assert_eq!(source, MigrationSourceArg::LiveDatabase("appdb".into()));
                assert_eq!(schema.as_deref(), Some("public"));
                assert_eq!(target_database.as_deref(), Some("appdb_snowdrop"));
            }
            _ => panic!("expected migration run command"),
        }
    }

    #[test]
    fn parse_command_supports_dump_based_migration_auto() {
        let args = vec![
            "snowdrop-testkit".to_string(),
            "migration-auto".to_string(),
            "--dump-dir".to_string(),
            "tests/large_postgres_bases_for_test/ticketing_orders".to_string(),
            "--source-database".to_string(),
            "ticketing_orders_large".to_string(),
            "--target-database".to_string(),
            "ticketing_orders_large_snowdrop".to_string(),
        ];
        match parse_command(&args).unwrap() {
            Command::MigrationAuto {
                source,
                schema,
                target_database,
            } => {
                assert_eq!(
                    source,
                    MigrationSourceArg::DumpDir {
                        dump_dir: PathBuf::from(
                            "tests/large_postgres_bases_for_test/ticketing_orders"
                        ),
                        source_database: "ticketing_orders_large".into(),
                    }
                );
                assert!(schema.is_none());
                assert_eq!(
                    target_database.as_deref(),
                    Some("ticketing_orders_large_snowdrop")
                );
            }
            _ => panic!("expected migration auto command"),
        }
    }

    #[test]
    fn migration_preflight_slug_uses_database_and_schema() {
        assert_eq!(
            migration_report_slug("App DB", Some("public-api")),
            "app-db-public-api-preflight"
        );
    }

    #[test]
    fn migration_auto_slug_uses_source_schema_and_target() {
        assert_eq!(
            migration_auto_slug("App DB", Some("public-api"), Some("snowdrop-db")),
            "app-db-public-api-snowdrop-db-auto"
        );
    }

    #[test]
    fn normalize_cell_unifies_postgres_boolean_spellings() {
        assert_eq!(normalize_cell("t"), "TRUE");
        assert_eq!(normalize_cell("false"), "FALSE");
    }

    #[test]
    fn numeric_without_scale_is_partial() {
        let column = PgColumn {
            schema_name: "public".into(),
            table_name: "ledger".into(),
            column_name: "amount".into(),
            nullable: false,
            data_type: "numeric".into(),
            udt_name: "numeric".into(),
            char_length: None,
            numeric_precision: Some(12),
            numeric_scale: None,
            datetime_precision: None,
            default: None,
        };
        let (snowdrop_type, status, notes) = map_postgres_type(&column);
        assert_eq!(snowdrop_type.as_deref(), Some("NUMERIC(12,0)"));
        assert_eq!(status, "partial");
        assert!(!notes.is_empty());
    }

    #[test]
    fn detect_table_name_collisions_flags_cross_schema_duplicates() {
        let collisions = detect_table_name_collisions(&[
            PgTable {
                schema_name: "public".into(),
                table_name: "users".into(),
            },
            PgTable {
                schema_name: "tenant".into(),
                table_name: "users".into(),
            },
            PgTable {
                schema_name: "public".into(),
                table_name: "orders".into(),
            },
        ]);
        assert_eq!(
            collisions.get("users"),
            Some(&vec!["public".into(), "tenant".into()])
        );
        assert!(!collisions.contains_key("orders"));
    }

    #[test]
    fn synthesized_not_null_checks_are_ignored_in_preflight() {
        let synthesized = PgConstraint {
            schema_name: "public".into(),
            table_name: "users".into(),
            constraint_name: "users_email_not_null".into(),
            constraint_type: "CHECK".into(),
            columns: vec!["email".into()],
            ref_schema_name: None,
            ref_table_name: None,
            ref_columns: Vec::new(),
            update_rule: None,
            delete_rule: None,
            check_clause: Some("email IS NOT NULL".into()),
        };
        let user_check = PgConstraint {
            constraint_name: "users_email_format".into(),
            check_clause: Some("position('@' in email) > 1".into()),
            ..synthesized.clone()
        };

        assert!(is_synthesized_not_null_check(&synthesized));
        assert!(!is_synthesized_not_null_check(&user_check));
    }

    #[test]
    fn load_order_plan_blocks_foreign_key_cycles() {
        let tables = vec![
            MigrationTableReport {
                schema_name: "public".into(),
                table_name: "accounts".into(),
                status: "supported".into(),
                translation_status: "ready".into(),
                columns: Vec::new(),
                constraints: vec![MigrationConstraintReport {
                    name: "accounts_owner_fk".into(),
                    kind: "FOREIGN KEY".into(),
                    columns: vec!["owner_id".into()],
                    status: "supported".into(),
                    reference: Some("public.users(id)".into()),
                    delete_rule: Some("NO ACTION".into()),
                    update_rule: Some("NO ACTION".into()),
                    check_clause: None,
                    notes: Vec::new(),
                }],
                indexes: Vec::new(),
                notes: Vec::new(),
                translation_notes: Vec::new(),
                translated_ddl: Some("CREATE TABLE \"accounts\" ();".into()),
            },
            MigrationTableReport {
                schema_name: "public".into(),
                table_name: "users".into(),
                status: "supported".into(),
                translation_status: "ready".into(),
                columns: Vec::new(),
                constraints: vec![MigrationConstraintReport {
                    name: "users_account_fk".into(),
                    kind: "FOREIGN KEY".into(),
                    columns: vec!["account_id".into()],
                    status: "supported".into(),
                    reference: Some("public.accounts(id)".into()),
                    delete_rule: Some("NO ACTION".into()),
                    update_rule: Some("NO ACTION".into()),
                    check_clause: None,
                    notes: Vec::new(),
                }],
                indexes: Vec::new(),
                notes: Vec::new(),
                translation_notes: Vec::new(),
                translated_ddl: Some("CREATE TABLE \"users\" ();".into()),
            },
        ];

        let plan = order_tables_for_load(&tables);
        assert!(plan.ordered.is_empty());
        assert_eq!(plan.cyclic, vec![0, 1]);
    }

    #[test]
    fn automatic_migration_allows_index_only_omissions() {
        let report = MigrationPreflightReport {
            generated_at: "2026-05-10T00:00:00Z".into(),
            runtime: RuntimeConfig {
                snowdrop_host: "orchestrator".into(),
                snowdrop_port: 5432,
                postgres_host: "postgres".into(),
                postgres_port: 5432,
                health_host: "orchestrator".into(),
                health_port: 8080,
                metrics_host: "orchestrator".into(),
                metrics_port: 9090,
            },
            source_database: "appdb".into(),
            source_schema_filter: Some("public".into()),
            summary: MigrationSummary {
                table_count: 1,
                column_count: 1,
                constraint_count: 0,
                index_count: 1,
                supported_tables: 0,
                partial_tables: 1,
                unsupported_tables: 0,
                ddl_ready_tables: 0,
                ddl_partial_tables: 1,
                ddl_blocked_tables: 0,
                findings_by_status: BTreeMap::from([("partial".into(), 1usize)]),
            },
            tables: vec![MigrationTableReport {
                schema_name: "public".into(),
                table_name: "users".into(),
                status: "partial".into(),
                translation_status: "partial".into(),
                columns: Vec::new(),
                constraints: Vec::new(),
                indexes: vec![MigrationIndexReport {
                    name: "idx_users_email".into(),
                    definition: "CREATE INDEX idx_users_email ON users(email)".into(),
                    status: "partial".into(),
                    notes: vec!["secondary-index compatibility is narrower".into()],
                }],
                notes: Vec::new(),
                translation_notes: vec![
                    "index 'idx_users_email' is omitted from the preview and should be recreated manually if still needed".into(),
                ],
                translated_ddl: Some("CREATE TABLE \"users\" ();".into()),
            }],
            findings: vec![MigrationFinding {
                status: "partial".into(),
                object_kind: "index".into(),
                object_name: "public.users.idx_users_email".into(),
                category: "index".into(),
                message: "index definition needs review during migration".into(),
                recommendation: "recreate later".into(),
            }],
            unsupported_inventory: BTreeMap::new(),
            partial_inventory: BTreeMap::new(),
        };

        assert!(collect_automatic_migration_blockers(&report).is_empty());
        assert_eq!(collect_automatic_migration_skips(&report).len(), 1);
    }

    #[test]
    fn automatic_migration_blocks_manual_follow_up_notes() {
        let report = MigrationPreflightReport {
            generated_at: "2026-05-10T00:00:00Z".into(),
            runtime: RuntimeConfig {
                snowdrop_host: "orchestrator".into(),
                snowdrop_port: 5432,
                postgres_host: "postgres".into(),
                postgres_port: 5432,
                health_host: "orchestrator".into(),
                health_port: 8080,
                metrics_host: "orchestrator".into(),
                metrics_port: 9090,
            },
            source_database: "appdb".into(),
            source_schema_filter: None,
            summary: MigrationSummary {
                table_count: 1,
                column_count: 1,
                constraint_count: 1,
                index_count: 0,
                supported_tables: 0,
                partial_tables: 1,
                unsupported_tables: 0,
                ddl_ready_tables: 0,
                ddl_partial_tables: 1,
                ddl_blocked_tables: 0,
                findings_by_status: BTreeMap::from([("partial".into(), 1usize)]),
            },
            tables: vec![MigrationTableReport {
                schema_name: "public".into(),
                table_name: "orders".into(),
                status: "partial".into(),
                translation_status: "partial".into(),
                columns: Vec::new(),
                constraints: Vec::new(),
                indexes: Vec::new(),
                notes: Vec::new(),
                translation_notes: vec![
                    "default for column 'created_at' needs manual rewrite and is omitted from preview".into(),
                ],
                translated_ddl: Some("CREATE TABLE \"orders\" ();".into()),
            }],
            findings: Vec::new(),
            unsupported_inventory: BTreeMap::new(),
            partial_inventory: BTreeMap::new(),
        };

        let blockers = collect_automatic_migration_blockers(&report);
        assert_eq!(blockers.len(), 1);
        assert!(blockers[0]
            .message
            .contains("needs manual rewrite and is omitted from preview"));
    }

    #[test]
    fn render_migration_preflight_report_includes_summary() {
        let report = MigrationPreflightReport {
            generated_at: "2026-05-10T00:00:00Z".into(),
            runtime: RuntimeConfig {
                snowdrop_host: "orchestrator".into(),
                snowdrop_port: 5432,
                postgres_host: "postgres".into(),
                postgres_port: 5432,
                health_host: "orchestrator".into(),
                health_port: 8080,
                metrics_host: "orchestrator".into(),
                metrics_port: 9090,
            },
            source_database: "appdb".into(),
            source_schema_filter: Some("public".into()),
            summary: MigrationSummary {
                table_count: 1,
                column_count: 2,
                constraint_count: 1,
                index_count: 1,
                supported_tables: 0,
                partial_tables: 1,
                unsupported_tables: 0,
                ddl_ready_tables: 0,
                ddl_partial_tables: 1,
                ddl_blocked_tables: 0,
                findings_by_status: BTreeMap::from([
                    ("partial".into(), 2usize),
                    ("unsupported".into(), 0usize),
                ]),
            },
            tables: vec![MigrationTableReport {
                schema_name: "public".into(),
                table_name: "users".into(),
                status: "partial".into(),
                translation_status: "partial".into(),
                columns: vec![MigrationColumnReport {
                    column_name: "email".into(),
                    source_type: "varchar(255)".into(),
                    snowdrop_type: Some("VARCHAR(255)".into()),
                    nullable: false,
                    default: None,
                    status: "supported".into(),
                    notes: vec![],
                }],
                constraints: vec![],
                indexes: vec![],
                notes: vec!["table needs manual review".into()],
                translation_notes: vec!["index recreation is omitted from the preview".into()],
                translated_ddl: Some(
                    "-- Source table public.users\nCREATE TABLE \"users\" (\n  \"email\" VARCHAR(255) NOT NULL\n);".into(),
                ),
            }],
            findings: vec![MigrationFinding {
                status: "partial".into(),
                object_kind: "column".into(),
                object_name: "public.users.id".into(),
                category: "default".into(),
                message: "sequence-backed default needs manual handling".into(),
                recommendation: "rewrite serial handling".into(),
            }],
            unsupported_inventory: BTreeMap::new(),
            partial_inventory: BTreeMap::from([(
                "default".into(),
                vec!["column public.users.id -> sequence-backed default needs manual handling".into()],
            )]),
        };

        let rendered = render_migration_preflight_report(&report);
        assert!(rendered.contains("# Snowdrop Migration Preflight"));
        assert!(rendered.contains("Source PostgreSQL database: `appdb`"));
        assert!(rendered.contains("| Tables | 1 |"));
        assert!(rendered.contains("sequence-backed default needs manual handling"));
        assert!(rendered.contains("## Partial Feature Inventory"));
        assert!(rendered.contains("Translation preview status: `partial`"));
        assert!(rendered.contains("```sql"));
        assert!(rendered.contains("CREATE TABLE \"users\""));
    }

    #[test]
    fn render_migration_translation_sql_includes_preview_and_skips() {
        let report = MigrationPreflightReport {
            generated_at: "2026-05-10T00:00:00Z".into(),
            runtime: RuntimeConfig {
                snowdrop_host: "orchestrator".into(),
                snowdrop_port: 5432,
                postgres_host: "postgres".into(),
                postgres_port: 5432,
                health_host: "orchestrator".into(),
                health_port: 8080,
                metrics_host: "orchestrator".into(),
                metrics_port: 9090,
            },
            source_database: "appdb".into(),
            source_schema_filter: None,
            summary: MigrationSummary {
                table_count: 2,
                column_count: 2,
                constraint_count: 0,
                index_count: 0,
                supported_tables: 1,
                partial_tables: 1,
                unsupported_tables: 0,
                ddl_ready_tables: 1,
                ddl_partial_tables: 0,
                ddl_blocked_tables: 1,
                findings_by_status: BTreeMap::new(),
            },
            tables: vec![
                MigrationTableReport {
                    schema_name: "public".into(),
                    table_name: "users".into(),
                    status: "supported".into(),
                    translation_status: "ready".into(),
                    columns: vec![],
                    constraints: vec![],
                    indexes: vec![],
                    notes: vec![],
                    translation_notes: vec![],
                    translated_ddl: Some(
                        "CREATE TABLE \"users\" (\n  \"id\" BIGINT NOT NULL\n);".into(),
                    ),
                },
                MigrationTableReport {
                    schema_name: "public".into(),
                    table_name: "events".into(),
                    status: "unsupported".into(),
                    translation_status: "blocked".into(),
                    columns: vec![],
                    constraints: vec![],
                    indexes: vec![],
                    notes: vec![],
                    translation_notes: vec![],
                    translated_ddl: None,
                },
            ],
            findings: vec![],
            unsupported_inventory: BTreeMap::new(),
            partial_inventory: BTreeMap::new(),
        };
        let rendered = render_migration_translation_sql(&report);
        assert!(rendered.contains("CREATE TABLE \"users\""));
        assert!(rendered.contains("Skipped table public.events"));
    }

    #[test]
    fn render_migration_workflow_report_includes_verification_summary() {
        let report = MigrationWorkflowReport {
            generated_at: "2026-05-10T00:00:00Z".into(),
            runtime: RuntimeConfig {
                snowdrop_host: "orchestrator".into(),
                snowdrop_port: 5432,
                postgres_host: "postgres".into(),
                postgres_port: 5432,
                health_host: "orchestrator".into(),
                health_port: 8080,
                metrics_host: "orchestrator".into(),
                metrics_port: 9090,
            },
            source_database: "appdb".into(),
            target_database: "appdb_snowdrop".into(),
            source_schema_filter: Some("public".into()),
            preflight: MigrationPreflightReport {
                generated_at: "2026-05-10T00:00:00Z".into(),
                runtime: RuntimeConfig {
                    snowdrop_host: "orchestrator".into(),
                    snowdrop_port: 5432,
                    postgres_host: "postgres".into(),
                    postgres_port: 5432,
                    health_host: "orchestrator".into(),
                    health_port: 8080,
                    metrics_host: "orchestrator".into(),
                    metrics_port: 9090,
                },
                source_database: "appdb".into(),
                source_schema_filter: Some("public".into()),
                summary: MigrationSummary {
                    table_count: 1,
                    column_count: 1,
                    constraint_count: 0,
                    index_count: 0,
                    supported_tables: 1,
                    partial_tables: 0,
                    unsupported_tables: 0,
                    ddl_ready_tables: 1,
                    ddl_partial_tables: 0,
                    ddl_blocked_tables: 0,
                    findings_by_status: BTreeMap::new(),
                },
                tables: Vec::new(),
                findings: Vec::new(),
                unsupported_inventory: BTreeMap::new(),
                partial_inventory: BTreeMap::new(),
            },
            summary: MigrationWorkflowSummary {
                planned_tables: 1,
                blocked_tables: 0,
                ddl_applied_tables: 1,
                loaded_tables: 1,
                verified_tables: 1,
                row_count_matches: 1,
                sample_matches: 1,
                aggregate_matches: 1,
                replay_queries_run: 3,
                replay_queries_matched: 3,
            },
            table_runs: vec![MigrationTableRun {
                schema_name: "public".into(),
                table_name: "users".into(),
                translation_status: "ready".into(),
                ddl_applied: true,
                load_status: "verified".into(),
                source_row_count: Some(5),
                target_row_count: Some(5),
                row_count_match: Some(true),
                sample_rows_match: Some(true),
                aggregate_match: Some(true),
                replay_checks: vec![MigrationReplayCheck {
                    name: "row_count".into(),
                    source_sql: "SELECT COUNT(*) FROM \"public\".\"users\";".into(),
                    target_sql: "SELECT COUNT(*) FROM \"users\";".into(),
                    matched: Some(true),
                    note: None,
                }],
                notes: vec!["all verification checks passed".into()],
            }],
        };
        let rendered = render_migration_workflow_report(&report);
        assert!(rendered.contains("# Snowdrop Migration Workflow"));
        assert!(rendered.contains("Target Snowdrop database: `appdb_snowdrop`"));
        assert!(rendered.contains("| Planned tables | 1 |"));
        assert!(rendered.contains("| Replay queries run | 3 |"));
        assert!(rendered.contains("### Replay Checks"));
        assert!(rendered.contains("all verification checks passed"));
    }
}
