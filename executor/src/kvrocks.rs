use anyhow::{Context, Result};
use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;
use tracing::info;

pub struct KvrocksConnection {
    conn: MultiplexedConnection,
}

impl KvrocksConnection {
    /// Connects to Kvrocks and verifies connectivity with PING.
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

    /// Verifies Kvrocks is responsive.
    pub async fn ping(&self) -> Result<()> {
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

    pub fn connection(&self) -> MultiplexedConnection {
        self.conn.clone()
    }
}
