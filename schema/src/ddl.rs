use crate::neo4j::Neo4jConnection;
use crate::types::ColumnType;
use anyhow::{bail, Context, Result};
use neo4rs::query;
use tracing::info;

#[derive(Debug, Clone)]
pub struct DatabaseInfo {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct TableInfo {
    pub id: u32,
    pub name: String,
    pub db_id: u32,
}

#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub id: u32,
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub is_pk: bool,
    pub table_id: u32,
}

impl Neo4jConnection {
    /// Initializes Neo4j constraints and counter nodes on first startup.
    pub async fn init_schema(&self) -> Result<()> {
        self.graph()
            .run(query(
                "CREATE CONSTRAINT IF NOT EXISTS FOR (db:Database) REQUIRE db.name IS UNIQUE",
            ))
            .await
            .context("failed to create Database uniqueness constraint")?;

        self.graph()
            .run(query(
                "CREATE CONSTRAINT IF NOT EXISTS FOR (t:Table) REQUIRE (t.db_id, t.name) IS UNIQUE",
            ))
            .await
            .context("failed to create Table uniqueness constraint")?;

        // Initialize counter nodes if they don't exist
        self.graph()
            .run(query(
                "MERGE (m:Meta {key: 'db_counter'}) ON CREATE SET m.value = 0",
            ))
            .await
            .context("failed to init db_counter")?;

        self.graph()
            .run(query(
                "MERGE (m:Meta {key: 'table_counter'}) ON CREATE SET m.value = 0",
            ))
            .await
            .context("failed to init table_counter")?;

        self.graph()
            .run(query(
                "MERGE (m:Meta {key: 'col_counter'}) ON CREATE SET m.value = 0",
            ))
            .await
            .context("failed to init col_counter")?;

        info!("Neo4j schema constraints and counters initialized");
        Ok(())
    }

    async fn next_id(&self, counter_key: &str) -> Result<u32> {
        let mut result = self
            .graph()
            .execute(query(
                "MATCH (m:Meta {key: $key}) \
                 SET m.value = m.value + 1 \
                 RETURN m.value AS id",
            ).param("key", counter_key))
            .await
            .context("failed to increment counter")?;

        let row = result
            .next()
            .await?
            .context("counter returned no result")?;

        let id: i64 = row.get("id").context("missing id field")?;
        Ok(id as u32)
    }

    // ── CREATE DATABASE ──

    pub async fn create_database(&self, name: &str) -> Result<DatabaseInfo> {
        let existing = self.get_database(name).await?;
        if existing.is_some() {
            bail!("database '{}' already exists", name);
        }

        let id = self.next_id("db_counter").await?;

        self.graph()
            .run(
                query("CREATE (:Database {id: $id, name: $name})")
                    .param("id", id as i64)
                    .param("name", name),
            )
            .await
            .context("failed to create Database node")?;

        info!(name, id, "database created");
        Ok(DatabaseInfo {
            id,
            name: name.to_string(),
        })
    }

    // ── DROP DATABASE ──

    pub async fn drop_database(&self, name: &str) -> Result<()> {
        let db = self
            .get_database(name)
            .await?
            .context(format!("database '{}' does not exist", name))?;

        // Delete all Table and Column nodes under this database
        self.graph()
            .run(
                query(
                    "MATCH (db:Database {name: $name})-[:HAS_TABLE]->(t:Table)-[:HAS_COLUMN]->(c:Column) \
                     DETACH DELETE c",
                )
                .param("name", name),
            )
            .await?;

        self.graph()
            .run(
                query(
                    "MATCH (db:Database {name: $name})-[:HAS_TABLE]->(t:Table) \
                     DETACH DELETE t",
                )
                .param("name", name),
            )
            .await?;

        self.graph()
            .run(
                query("MATCH (db:Database {name: $name}) DETACH DELETE db")
                    .param("name", name),
            )
            .await?;

        info!(name, id = db.id, "database dropped");
        Ok(())
    }

    // ── GET DATABASE ──

    pub async fn get_database(&self, name: &str) -> Result<Option<DatabaseInfo>> {
        let mut result = self
            .graph()
            .execute(
                query("MATCH (db:Database {name: $name}) RETURN db.id AS id, db.name AS name")
                    .param("name", name),
            )
            .await?;

        match result.next().await? {
            Some(row) => {
                let id: i64 = row.get("id")?;
                let name: String = row.get("name")?;
                Ok(Some(DatabaseInfo {
                    id: id as u32,
                    name,
                }))
            }
            None => Ok(None),
        }
    }

    // ── SHOW DATABASES ──

    pub async fn show_databases(&self) -> Result<Vec<DatabaseInfo>> {
        let mut result = self
            .graph()
            .execute(query(
                "MATCH (db:Database) RETURN db.id AS id, db.name AS name ORDER BY db.name",
            ))
            .await?;

        let mut databases = Vec::new();
        while let Some(row) = result.next().await? {
            let id: i64 = row.get("id")?;
            let name: String = row.get("name")?;
            databases.push(DatabaseInfo {
                id: id as u32,
                name,
            });
        }
        Ok(databases)
    }

    // ── CREATE TABLE ──

    pub async fn create_table(
        &self,
        db_name: &str,
        table_name: &str,
        columns: Vec<(String, ColumnType, bool, bool)>, // (name, type, nullable, is_pk)
    ) -> Result<TableInfo> {
        let db = self
            .get_database(db_name)
            .await?
            .context(format!("database '{}' does not exist", db_name))?;

        // Validate exactly one PK
        let pk_count = columns.iter().filter(|(_, _, _, is_pk)| *is_pk).count();
        if pk_count == 0 {
            bail!("table must have exactly one PRIMARY KEY column");
        }
        if pk_count > 1 {
            bail!("table must have exactly one PRIMARY KEY column, found {pk_count}");
        }

        // Check table doesn't already exist
        let existing = self.get_table(db_name, table_name).await?;
        if existing.is_some() {
            bail!("table '{}' already exists in database '{}'", table_name, db_name);
        }

        let table_id = self.next_id("table_counter").await?;

        // Create Table node with relationship to Database
        self.graph()
            .run(
                query(
                    "MATCH (db:Database {name: $db_name}) \
                     CREATE (db)-[:HAS_TABLE]->(t:Table {id: $id, name: $name, db_id: $db_id})",
                )
                .param("db_name", db_name)
                .param("id", table_id as i64)
                .param("name", table_name)
                .param("db_id", db.id as i64),
            )
            .await
            .context("failed to create Table node")?;

        // Create Column nodes
        for (col_name, col_type, nullable, is_pk) in &columns {
            let col_id = self.next_id("col_counter").await?;

            self.graph()
                .run(
                    query(
                        "MATCH (t:Table {id: $table_id}) \
                         CREATE (t)-[:HAS_COLUMN]->(c:Column { \
                             id: $id, name: $name, data_type: $data_type, \
                             nullable: $nullable, is_pk: $is_pk, table_id: $table_id \
                         })",
                    )
                    .param("table_id", table_id as i64)
                    .param("id", col_id as i64)
                    .param("name", col_name.as_str())
                    .param("data_type", col_type.as_str())
                    .param("nullable", *nullable)
                    .param("is_pk", *is_pk),
                )
                .await
                .context("failed to create Column node")?;
        }

        info!(db_name, table_name, table_id, cols = columns.len(), "table created");
        Ok(TableInfo {
            id: table_id,
            name: table_name.to_string(),
            db_id: db.id,
        })
    }

    // ── DROP TABLE ──

    pub async fn drop_table(&self, db_name: &str, table_name: &str) -> Result<()> {
        let _table = self
            .get_table(db_name, table_name)
            .await?
            .context(format!(
                "table '{}' does not exist in database '{}'",
                table_name, db_name
            ))?;

        // Delete IndexEntry nodes first
        self.graph()
            .run(
                query(
                    "MATCH (db:Database {name: $db_name})-[:HAS_TABLE]->(t:Table {name: $table_name}) \
                     -[:HAS_COLUMN]->(c:Column)-[:HAS_INDEX_ENTRY]->(e:IndexEntry) \
                     DETACH DELETE e",
                )
                .param("db_name", db_name)
                .param("table_name", table_name),
            )
            .await?;

        // Delete Column nodes
        self.graph()
            .run(
                query(
                    "MATCH (db:Database {name: $db_name})-[:HAS_TABLE]->(t:Table {name: $table_name}) \
                     -[:HAS_COLUMN]->(c:Column) \
                     DETACH DELETE c",
                )
                .param("db_name", db_name)
                .param("table_name", table_name),
            )
            .await?;

        // Delete Table node
        self.graph()
            .run(
                query(
                    "MATCH (db:Database {name: $db_name})-[:HAS_TABLE]->(t:Table {name: $table_name}) \
                     DETACH DELETE t",
                )
                .param("db_name", db_name)
                .param("table_name", table_name),
            )
            .await?;

        info!(db_name, table_name, "table dropped");
        Ok(())
    }

    // ── GET TABLE ──

    pub async fn get_table(&self, db_name: &str, table_name: &str) -> Result<Option<TableInfo>> {
        let mut result = self
            .graph()
            .execute(
                query(
                    "MATCH (db:Database {name: $db_name})-[:HAS_TABLE]->(t:Table {name: $table_name}) \
                     RETURN t.id AS id, t.name AS name, t.db_id AS db_id",
                )
                .param("db_name", db_name)
                .param("table_name", table_name),
            )
            .await?;

        match result.next().await? {
            Some(row) => {
                let id: i64 = row.get("id")?;
                let name: String = row.get("name")?;
                let db_id: i64 = row.get("db_id")?;
                Ok(Some(TableInfo {
                    id: id as u32,
                    name,
                    db_id: db_id as u32,
                }))
            }
            None => Ok(None),
        }
    }

    // ── SHOW TABLES ──

    pub async fn show_tables(&self, db_name: &str) -> Result<Vec<TableInfo>> {
        let mut result = self
            .graph()
            .execute(
                query(
                    "MATCH (db:Database {name: $db_name})-[:HAS_TABLE]->(t:Table) \
                     RETURN t.id AS id, t.name AS name, t.db_id AS db_id \
                     ORDER BY t.name",
                )
                .param("db_name", db_name),
            )
            .await?;

        let mut tables = Vec::new();
        while let Some(row) = result.next().await? {
            let id: i64 = row.get("id")?;
            let name: String = row.get("name")?;
            let db_id: i64 = row.get("db_id")?;
            tables.push(TableInfo {
                id: id as u32,
                name,
                db_id: db_id as u32,
            });
        }
        Ok(tables)
    }

    // ── DESCRIBE TABLE ──

    pub async fn describe_table(
        &self,
        db_name: &str,
        table_name: &str,
    ) -> Result<Vec<ColumnInfo>> {
        let _table = self
            .get_table(db_name, table_name)
            .await?
            .context(format!(
                "table '{}' does not exist in database '{}'",
                table_name, db_name
            ))?;

        let mut result = self
            .graph()
            .execute(
                query(
                    "MATCH (db:Database {name: $db_name})-[:HAS_TABLE]->(t:Table {name: $table_name}) \
                     -[:HAS_COLUMN]->(c:Column) \
                     RETURN c.id AS id, c.name AS name, c.data_type AS data_type, \
                            c.nullable AS nullable, c.is_pk AS is_pk, c.table_id AS table_id \
                     ORDER BY c.id",
                )
                .param("db_name", db_name)
                .param("table_name", table_name),
            )
            .await?;

        let mut columns = Vec::new();
        while let Some(row) = result.next().await? {
            let id: i64 = row.get("id")?;
            let name: String = row.get("name")?;
            let data_type: String = row.get("data_type")?;
            let nullable: bool = row.get("nullable")?;
            let is_pk: bool = row.get("is_pk")?;
            let table_id: i64 = row.get("table_id")?;
            columns.push(ColumnInfo {
                id: id as u32,
                name,
                data_type,
                nullable,
                is_pk,
                table_id: table_id as u32,
            });
        }
        Ok(columns)
    }

    // ── ALTER TABLE ADD COLUMN ──

    pub async fn add_column(
        &self,
        db_name: &str,
        table_name: &str,
        col_name: &str,
        col_type: &ColumnType,
        nullable: bool,
    ) -> Result<ColumnInfo> {
        let table = self
            .get_table(db_name, table_name)
            .await?
            .context(format!(
                "table '{}' does not exist in database '{}'",
                table_name, db_name
            ))?;

        // Check column doesn't already exist
        let cols = self.describe_table(db_name, table_name).await?;
        if cols.iter().any(|c| c.name == col_name) {
            bail!("column '{}' already exists in table '{}'", col_name, table_name);
        }

        let col_id = self.next_id("col_counter").await?;

        self.graph()
            .run(
                query(
                    "MATCH (t:Table {id: $table_id}) \
                     CREATE (t)-[:HAS_COLUMN]->(c:Column { \
                         id: $id, name: $name, data_type: $data_type, \
                         nullable: $nullable, is_pk: false, table_id: $table_id \
                     })",
                )
                .param("table_id", table.id as i64)
                .param("id", col_id as i64)
                .param("name", col_name)
                .param("data_type", col_type.as_str())
                .param("nullable", nullable),
            )
            .await
            .context("failed to add column")?;

        info!(db_name, table_name, col_name, col_id, "column added");
        Ok(ColumnInfo {
            id: col_id,
            name: col_name.to_string(),
            data_type: col_type.as_str(),
            nullable,
            is_pk: false,
            table_id: table.id,
        })
    }

    // ── INFORMATION_SCHEMA ──

    pub async fn information_schema_tables(&self, db_name: &str) -> Result<Vec<(String, String, String)>> {
        // Returns (table_catalog, table_schema, table_name)
        let tables = self.show_tables(db_name).await?;
        Ok(tables
            .into_iter()
            .map(|t| (db_name.to_string(), "public".to_string(), t.name))
            .collect())
    }

    pub async fn information_schema_columns(
        &self,
        db_name: &str,
    ) -> Result<Vec<(String, String, String, String, String, String)>> {
        // Returns (table_catalog, table_schema, table_name, column_name, data_type, is_nullable)
        let tables = self.show_tables(db_name).await?;
        let mut rows = Vec::new();
        for table in &tables {
            let cols = self.describe_table(db_name, &table.name).await?;
            for col in cols {
                rows.push((
                    db_name.to_string(),
                    "public".to_string(),
                    table.name.clone(),
                    col.name,
                    col.data_type,
                    if col.nullable { "YES".to_string() } else { "NO".to_string() },
                ));
            }
        }
        Ok(rows)
    }
}
