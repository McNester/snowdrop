mod health;
mod pgwire_handler;
mod query_router;

use anyhow::{Context, Result};
use schema::{CatalogBackend, CatalogStore};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerStatus {
    Starting,
    Ready,
}

pub struct AppState {
    pub status: ServerStatus,
}

fn env_port(name: &str, default: u16) -> u16 {
    match std::env::var(name) {
        Ok(raw) => match raw.parse::<u16>() {
            Ok(port) => port,
            Err(_) => {
                warn!(
                    env = name,
                    value = raw,
                    default,
                    "invalid port value, using default"
                );
                default
            }
        },
        Err(_) => default,
    }
}

fn env_choice(name: &str, default: &str, allowed: &[&str]) -> String {
    let raw = std::env::var(name).unwrap_or_else(|_| default.to_string());
    let normalized = raw.trim().to_ascii_lowercase();
    if allowed.iter().any(|candidate| *candidate == normalized) {
        normalized
    } else {
        warn!(
            env = name,
            value = raw,
            default,
            "invalid runtime mode, using default"
        );
        default.to_string()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("Snowdrop database starting");

    let metrics_port = env_port("METRICS_PORT", 9090);
    let health_port = env_port("HEALTH_PORT", 8080);
    let pgwire_port = env_port("PGWIRE_PORT", 5432);
    let execution_engine = env_choice(
        "SNOWDROP_EXECUTION_ENGINE",
        "current",
        &["current", "datafusion"],
    );
    let columnar_store = env_choice("SNOWDROP_COLUMNAR_STORE", "cache", &["cache", "segments"]);
    let columnar_segment_path =
        std::env::var("COLUMNAR_SEGMENT_PATH").unwrap_or_else(|_| "/data/columnar".to_string());
    info!(
        execution_engine,
        columnar_store, columnar_segment_path, "runtime acceleration modes configured"
    );
    if columnar_store == "segments" {
        let segments = executor::FileColumnarSegmentStore::open(&columnar_segment_path)
            .context("columnar segment store initialization failed")?;
        info!(
            path = %segments.root().display(),
            "columnar segment store initialized"
        );
    }
    if execution_engine == "datafusion" {
        warn!(
            "SNOWDROP_EXECUTION_ENGINE=datafusion selected, but DataFusion execution is not wired yet; falling back to current executor"
        );
    }

    // Initialize Prometheus metrics exporter on METRICS_PORT
    metrics_exporter_prometheus::PrometheusBuilder::new()
        .with_http_listener(([0, 0, 0, 0], metrics_port))
        .install()
        .expect("failed to install Prometheus metrics exporter");
    info!(port = metrics_port, "Prometheus metrics exporter running");

    let state = Arc::new(RwLock::new(AppState {
        status: ServerStatus::Starting,
    }));

    // Step 1: Start health endpoint (returns 503 until ready)
    let health_state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = health::serve_health(health_state, health_port).await {
            error!("health server failed: {e}");
        }
    });

    // Step 2: Open WAL, scan and replay
    let wal_path = std::env::var("WAL_PATH").unwrap_or_else(|_| "/data/wal".to_string());
    let wal_file_path = format!("{wal_path}/wal.log");

    let mut wal = wal::WalFile::open(&wal_file_path).context("failed to open WAL")?;

    let needs_replay = wal.replay().context("WAL replay failed")?;

    if !needs_replay.is_empty() {
        info!(count = needs_replay.len(), "WAL entries need replay");
    }

    // Step 3: Initialize catalog backend foundation. The legacy Neo4j catalog
    // remains available while sqlite/mirror modes start serving runtime cache
    // lookups through CatalogStore.
    let catalog_config = schema::CatalogConfig::from_env();
    info!(
        backend = catalog_config.backend.as_str(),
        sqlite_path = %catalog_config.sqlite_path.display(),
        "catalog backend configured"
    );
    let catalog_store: Option<Arc<dyn schema::CatalogStore>> =
        if catalog_config.backend.needs_sqlite() {
            let sqlite_catalog = Arc::new(
                schema::SQLiteCatalogStore::open(&catalog_config.sqlite_path)
                    .context("SQLite catalog open failed")?,
            );
            sqlite_catalog
                .init_catalog()
                .await
                .context("SQLite catalog initialization failed")?;
            sqlite_catalog
                .ping()
                .await
                .context("SQLite catalog ping failed")?;
            Some(sqlite_catalog)
        } else {
            None
        };
    let embedded_catalog_only = catalog_config.backend == CatalogBackend::Sqlite;
    if embedded_catalog_only {
        warn!(
            "SNOWDROP_CATALOG_BACKEND=sqlite enables embedded catalog reads and basic \
             database/table DDL; Neo4j is still required for unsupported legacy DDL paths"
        );
    }

    // Step 4: Verify Neo4j connectivity when the selected mode still needs the
    // legacy catalog. SQLite mode can start without Neo4j and replays storage
    // WAL entries through the embedded catalog metadata path.
    let neo4j_uri =
        std::env::var("NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".to_string());
    let neo4j_user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".to_string());
    let neo4j_pass = std::env::var("NEO4J_PASS").unwrap_or_else(|_| "snowdroppassword".to_string());

    let neo4j = if embedded_catalog_only {
        info!("Neo4j connection skipped for embedded SQLite catalog mode");
        None
    } else {
        let neo4j = schema::Neo4jConnection::connect(&neo4j_uri, &neo4j_user, &neo4j_pass)
            .await
            .context("Neo4j connection failed")?;

        neo4j
            .init_schema()
            .await
            .context("schema initialization failed")?;
        info!("Neo4j connectivity verified");
        Some(neo4j)
    };

    // Step 5: Open storage backend (embedded RocksDB or remote Kvrocks)
    let storage_backend =
        std::env::var("STORAGE_BACKEND").unwrap_or_else(|_| "rocksdb".to_string());
    let storage: Arc<dyn executor::StorageBackend> = match storage_backend.as_str() {
        "kvrocks" => {
            let kvrocks_addr =
                std::env::var("KVROCKS_ADDR").unwrap_or_else(|_| "localhost:6666".to_string());
            let kv = executor::KvrocksConnection::connect(&kvrocks_addr)
                .await
                .context("Kvrocks connection failed")?;
            info!("Storage: Kvrocks (remote)");
            Arc::new(kv)
        }
        _ => {
            let rocks_path =
                std::env::var("ROCKS_PATH").unwrap_or_else(|_| "/data/rocks".to_string());
            let rocks = executor::RocksStorage::open(&rocks_path).context("RocksDB open failed")?;
            info!("Storage: RocksDB (embedded)");
            Arc::new(rocks)
        }
    };

    storage.ping().await?;
    info!("Storage connectivity verified");

    // Step 6: Re-execute writes for Neo4jCommitted entries
    if !needs_replay.is_empty() {
        info!(
            count = needs_replay.len(),
            "replaying WAL entries to storage"
        );
        if embedded_catalog_only {
            let Some(catalog) = catalog_store.as_ref() else {
                anyhow::bail!("SQLite catalog backend selected but embedded catalog is missing");
            };
            executor::replay_wal_entries_with_catalog(
                storage.as_ref(),
                catalog.as_ref(),
                &needs_replay,
                &wal,
            )
            .await
            .context("embedded catalog WAL replay failed")?;
        } else {
            let Some(neo4j_ref) = neo4j.as_ref() else {
                anyhow::bail!(
                    "WAL replay has {} pending entries, but Neo4j is unavailable",
                    needs_replay.len()
                );
            };
            executor::replay_wal_entries(storage.as_ref(), neo4j_ref, &needs_replay, &wal)
                .await
                .context("WAL replay failed")?;
        }
    }

    // Step 7: Mark ready, begin accepting pgwire connections
    {
        let mut s = state.write().await;
        s.status = ServerStatus::Ready;
    }
    info!(
        port = pgwire_port,
        "Snowdrop ready - accepting pgwire connections"
    );

    let pgwire_addr = format!("0.0.0.0:{pgwire_port}");
    pgwire_handler::serve_pgwire(
        pgwire_addr,
        neo4j,
        storage,
        wal,
        catalog_config.backend,
        catalog_store,
    )
    .await
}
