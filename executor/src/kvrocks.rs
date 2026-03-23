use crate::storage::StorageBackend;
use anyhow::{Context, Result};
use async_trait::async_trait;
use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;
use tracing::{debug, info};

pub struct KvrocksConnection {
    conn: MultiplexedConnection,
}

impl KvrocksConnection {
    pub async fn connect(addr: &str) -> Result<Self> {
        let url = format!("redis://{addr}");
        let client =
            redis::Client::open(url.as_str()).context("failed to create Kvrocks client")?;

        let conn = client
            .get_multiplexed_async_connection()
            .await
            .context("failed to connect to Kvrocks")?;

        info!(addr, "Kvrocks connection established");

        let kv = Self { conn };
        kv.ping().await?;

        Ok(kv)
    }
}

#[async_trait]
impl StorageBackend for KvrocksConnection {
    async fn ping(&self) -> Result<()> {
        let mut conn = self.conn.clone();
        let pong: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .context("Kvrocks PING failed")?;

        if pong != "PONG" {
            anyhow::bail!("Kvrocks PING returned unexpected response: {pong}");
        }

        info!("Kvrocks PING OK");
        Ok(())
    }

    async fn mset(&self, pairs: &[(String, Vec<u8>)]) -> Result<()> {
        if pairs.is_empty() {
            return Ok(());
        }

        let mut conn = self.conn.clone();
        let kv_pairs: Vec<(&str, &[u8])> = pairs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_slice()))
            .collect();
        conn.mset::<_, _, ()>(&kv_pairs)
            .await
            .context("Kvrocks MSET failed")?;

        debug!(count = pairs.len(), "MSET complete");
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let mut conn = self.conn.clone();
        let val: Option<Vec<u8>> = conn.get(key).await.context("Kvrocks GET failed")?;
        Ok(val)
    }

    async fn mget(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }

        let mut conn = self.conn.clone();
        let vals: Vec<Option<Vec<u8>>> = conn.mget(keys).await.context("Kvrocks MGET failed")?;
        Ok(vals)
    }

    async fn del(&self, keys: &[String]) -> Result<u64> {
        if keys.is_empty() {
            return Ok(0);
        }

        let mut conn = self.conn.clone();
        let deleted: u64 = conn.del(keys).await.context("Kvrocks DEL failed")?;

        debug!(count = deleted, "DEL complete");
        Ok(deleted)
    }

    async fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let pattern = format!("{prefix}*");
        let mut conn = self.conn.clone();
        let mut keys = Vec::new();
        let mut cursor: u64 = 0;

        loop {
            let (next_cursor, batch): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(1000)
                .query_async(&mut conn)
                .await
                .context("Kvrocks SCAN failed")?;

            keys.extend(batch);
            cursor = next_cursor;
            if cursor == 0 {
                break;
            }
        }

        // Fetch values for all scanned keys
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let vals: Vec<Option<Vec<u8>>> = conn.mget(&keys).await.context("Kvrocks MGET failed")?;
        let result = keys
            .into_iter()
            .zip(vals)
            .filter_map(|(k, v)| v.map(|val| (k, val)))
            .collect();
        Ok(result)
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        let mut conn = self.conn.clone();
        let exists: bool = conn.exists(key).await.context("Kvrocks EXISTS failed")?;
        Ok(exists)
    }
}
