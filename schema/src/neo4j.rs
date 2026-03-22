use anyhow::{Context, Result};
use neo4rs::{ConfigBuilder, Graph};
use tracing::info;

pub struct Neo4jConnection {
    graph: Graph,
}

impl Neo4jConnection {
    /// Connects to Neo4j and verifies connectivity with a ping query.
    pub async fn connect(uri: &str, user: &str, password: &str) -> Result<Self> {
        let config = ConfigBuilder::default()
            .uri(uri)
            .user(user)
            .password(password)
            .build()
            .context("failed to build Neo4j config")?;

        let graph = Graph::connect(config)
            .await
            .context("failed to connect to Neo4j")?;

        info!(uri, "Neo4j connection established");

        let conn = Self { graph };
        conn.ping().await?;

        Ok(conn)
    }

    /// Verifies Neo4j is responsive with a simple query.
    pub async fn ping(&self) -> Result<()> {
        let mut result = self
            .graph
            .execute(neo4rs::query("RETURN 1 AS ping"))
            .await
            .context("Neo4j ping query failed")?;

        let row = result
            .next()
            .await
            .context("Neo4j ping returned no result")?
            .context("Neo4j ping returned empty row")?;

        let val: i64 = row.get("ping").context("Neo4j ping missing 'ping' field")?;
        if val != 1 {
            anyhow::bail!("Neo4j ping returned unexpected value: {val}");
        }

        info!("Neo4j ping OK");
        Ok(())
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }
}
