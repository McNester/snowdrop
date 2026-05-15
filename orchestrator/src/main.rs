mod health;
mod pgwire_handler;
mod query_router;

use anyhow::{Context, Result};
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

    // Step 3: Verify Neo4j connectivity
    let neo4j_uri =
        std::env::var("NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".to_string());
    let neo4j_user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".to_string());
    let neo4j_pass = std::env::var("NEO4J_PASS").unwrap_or_else(|_| "snowdroppassword".to_string());

    let neo4j = schema::Neo4jConnection::connect(&neo4j_uri, &neo4j_user, &neo4j_pass)
        .await
        .context("Neo4j connection failed")?;

    neo4j
        .init_schema()
        .await
        .context("schema initialization failed")?;
    info!("Neo4j connectivity verified");

    // Step 4: Open storage backend (embedded RocksDB or remote Kvrocks)
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

    // Step 5: Re-execute writes for Neo4jCommitted entries
    if !needs_replay.is_empty() {
        info!(
            count = needs_replay.len(),
            "replaying WAL entries to storage"
        );
        executor::replay_wal_entries(storage.as_ref(), &neo4j, &needs_replay, &wal)
            .await
            .context("WAL replay failed")?;
    }

    // Step 6: Mark ready, begin accepting pgwire connections
    {
        let mut s = state.write().await;
        s.status = ServerStatus::Ready;
    }
    info!(
        port = pgwire_port,
        "Snowdrop ready - accepting pgwire connections"
    );

    let pgwire_addr = format!("0.0.0.0:{pgwire_port}");
    pgwire_handler::serve_pgwire(pgwire_addr, neo4j, storage, wal).await
}
