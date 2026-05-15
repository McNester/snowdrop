use crate::neo4j::Neo4jConnection;
use crate::types::ColumnType;
use anyhow::{bail, Context, Result};
use neo4rs::query;
use serde::{Deserialize, Serialize};
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
    pub indexed: bool,
    pub index_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ColumnDefinition {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub is_pk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UniqueConstraintInfo {
    pub name: String,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForeignKeyConstraintInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub ref_db_name: Option<String>,
    pub ref_table_name: String,
    pub ref_columns: Vec<String>,
    pub on_delete: String,
    pub on_update: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckConstraintInfo {
    pub name: String,
    pub expr_sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TableConstraintsInfo {
    pub primary_key_columns: Vec<String>,
    pub defaults: std::collections::HashMap<String, String>,
    pub unique_constraints: Vec<UniqueConstraintInfo>,
    pub foreign_keys: Vec<ForeignKeyConstraintInfo>,
    pub check_constraints: Vec<CheckConstraintInfo>,
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
            .execute(
                query(
                    "MATCH (m:Meta {key: $key}) \
                 SET m.value = m.value + 1 \
                 RETURN m.value AS id",
                )
                .param("key", counter_key),
            )
            .await
            .context("failed to increment counter")?;

        let row = result.next().await?.context("counter returned no result")?;

        let id: i64 = row.get("id").context("missing id field")?;
        Ok(id as u32)
    }

    fn serialize_constraints(
        constraints: &TableConstraintsInfo,
    ) -> Result<(String, String, String, String)> {
        Ok((
            serde_json::to_string(&constraints.primary_key_columns)?,
            serde_json::to_string(&constraints.defaults)?,
            serde_json::to_string(&constraints.unique_constraints)?,
            serde_json::to_string(&constraints.foreign_keys)?,
        ))
    }

    fn serialize_check_constraints(constraints: &TableConstraintsInfo) -> Result<String> {
        Ok(serde_json::to_string(&constraints.check_constraints)?)
    }

    fn deserialize_constraints(
        pk_json: Option<String>,
        defaults_json: Option<String>,
        unique_json: Option<String>,
        foreign_keys_json: Option<String>,
        checks_json: Option<String>,
    ) -> Result<TableConstraintsInfo> {
        Ok(TableConstraintsInfo {
            primary_key_columns: pk_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?
                .unwrap_or_default(),
            defaults: defaults_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?
                .unwrap_or_default(),
            unique_constraints: unique_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?
                .unwrap_or_default(),
            foreign_keys: foreign_keys_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?
                .unwrap_or_default(),
            check_constraints: checks_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?
                .unwrap_or_default(),
        })
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
            .run(query("MATCH (db:Database {name: $name}) DETACH DELETE db").param("name", name))
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
        columns: Vec<ColumnDefinition>,
        constraints: TableConstraintsInfo,
    ) -> Result<TableInfo> {
        let db = self
            .get_database(db_name)
            .await?
            .context(format!("database '{}' does not exist", db_name))?;

        if constraints.primary_key_columns.is_empty() {
            bail!("table must have at least one PRIMARY KEY column");
        }

        // Check table doesn't already exist
        let existing = self.get_table(db_name, table_name).await?;
        if existing.is_some() {
            bail!(
                "table '{}' already exists in database '{}'",
                table_name,
                db_name
            );
        }

        let table_id = self.next_id("table_counter").await?;

        // Create Table node with relationship to Database
        let (pk_json, defaults_json, unique_json, foreign_keys_json) =
            Self::serialize_constraints(&constraints)?;
        let checks_json = Self::serialize_check_constraints(&constraints)?;

        self.graph()
            .run(
                query(
                    "MATCH (db:Database {name: $db_name}) \
                     CREATE (db)-[:HAS_TABLE]->(t:Table { \
                        id: $id, name: $name, db_id: $db_id, \
                        pk_columns_json: $pk_columns_json, \
                        defaults_json: $defaults_json, \
                        unique_constraints_json: $unique_constraints_json, \
                        foreign_keys_json: $foreign_keys_json, \
                        check_constraints_json: $check_constraints_json \
                     })",
                )
                .param("db_name", db_name)
                .param("id", table_id as i64)
                .param("name", table_name)
                .param("db_id", db.id as i64)
                .param("pk_columns_json", pk_json)
                .param("defaults_json", defaults_json)
                .param("unique_constraints_json", unique_json)
                .param("foreign_keys_json", foreign_keys_json)
                .param("check_constraints_json", checks_json),
            )
            .await
            .context("failed to create Table node")?;

        // Create Column nodes
        for col in &columns {
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
                    .param("name", col.name.as_str())
                    .param("data_type", col.data_type.as_str())
                    .param("nullable", col.nullable)
                    .param("is_pk", col.is_pk),
                )
                .await
                .context("failed to create Column node")?;
        }

        info!(
            db_name,
            table_name,
            table_id,
            cols = columns.len(),
            "table created"
        );
        Ok(TableInfo {
            id: table_id,
            name: table_name.to_string(),
            db_id: db.id,
        })
    }

    // ── DROP TABLE ──

    pub async fn drop_table(&self, db_name: &str, table_name: &str) -> Result<()> {
        let _table = self.get_table(db_name, table_name).await?.context(format!(
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

    pub async fn describe_table(&self, db_name: &str, table_name: &str) -> Result<Vec<ColumnInfo>> {
        let _table = self.get_table(db_name, table_name).await?.context(format!(
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
                            c.nullable AS nullable, c.is_pk AS is_pk, c.table_id AS table_id, \
                            COALESCE(c.indexed, false) AS indexed, c.index_name AS index_name \
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
            let indexed: bool = row.get("indexed").unwrap_or(false);
            let index_name: Option<String> = row.get("index_name").ok();
            columns.push(ColumnInfo {
                id: id as u32,
                name,
                data_type,
                nullable,
                is_pk,
                table_id: table_id as u32,
                indexed,
                index_name,
            });
        }
        Ok(columns)
    }

    pub async fn get_table_constraints(
        &self,
        db_name: &str,
        table_name: &str,
    ) -> Result<TableConstraintsInfo> {
        let mut result = self
            .graph()
            .execute(
                query(
                    "MATCH (db:Database {name: $db_name})-[:HAS_TABLE]->(t:Table {name: $table_name}) \
                     RETURN t.pk_columns_json AS pk_columns_json, \
                            t.defaults_json AS defaults_json, \
                            t.unique_constraints_json AS unique_constraints_json, \
                            t.foreign_keys_json AS foreign_keys_json, \
                            t.check_constraints_json AS check_constraints_json",
                )
                .param("db_name", db_name)
                .param("table_name", table_name),
            )
            .await?;

        let row = result.next().await?.context(format!(
            "table '{}' does not exist in database '{}'",
            table_name, db_name
        ))?;

        let pk_json: Option<String> = row.get("pk_columns_json").ok();
        let defaults_json: Option<String> = row.get("defaults_json").ok();
        let unique_json: Option<String> = row.get("unique_constraints_json").ok();
        let foreign_keys_json: Option<String> = row.get("foreign_keys_json").ok();
        let checks_json: Option<String> = row.get("check_constraints_json").ok();
        Self::deserialize_constraints(
            pk_json,
            defaults_json,
            unique_json,
            foreign_keys_json,
            checks_json,
        )
    }

    async fn set_table_constraints(
        &self,
        db_name: &str,
        table_name: &str,
        constraints: &TableConstraintsInfo,
    ) -> Result<()> {
        let (pk_json, defaults_json, unique_json, foreign_keys_json) =
            Self::serialize_constraints(constraints)?;
        let checks_json = Self::serialize_check_constraints(constraints)?;

        self.graph()
            .run(
                query(
                    "MATCH (db:Database {name: $db_name})-[:HAS_TABLE]->(t:Table {name: $table_name}) \
                     SET t.pk_columns_json = $pk_columns_json, \
                         t.defaults_json = $defaults_json, \
                         t.unique_constraints_json = $unique_constraints_json, \
                         t.foreign_keys_json = $foreign_keys_json, \
                         t.check_constraints_json = $check_constraints_json",
                )
                .param("db_name", db_name)
                .param("table_name", table_name)
                .param("pk_columns_json", pk_json)
                .param("defaults_json", defaults_json)
                .param("unique_constraints_json", unique_json)
                .param("foreign_keys_json", foreign_keys_json)
                .param("check_constraints_json", checks_json),
            )
            .await
            .context("failed to update table constraints")?;

        Ok(())
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
        let table = self.get_table(db_name, table_name).await?.context(format!(
            "table '{}' does not exist in database '{}'",
            table_name, db_name
        ))?;

        // Check column doesn't already exist
        let cols = self.describe_table(db_name, table_name).await?;
        if cols.iter().any(|c| c.name == col_name) {
            bail!(
                "column '{}' already exists in table '{}'",
                col_name,
                table_name
            );
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
            indexed: false,
            index_name: None,
        })
    }

    pub async fn rename_column(
        &self,
        db_name: &str,
        table_name: &str,
        old_name: &str,
        new_name: &str,
    ) -> Result<()> {
        let cols = self.describe_table(db_name, table_name).await?;
        if cols.iter().all(|c| c.name != old_name) {
            bail!(
                "column '{}' does not exist in table '{}'",
                old_name,
                table_name
            );
        }
        if cols.iter().any(|c| c.name == new_name) {
            bail!(
                "column '{}' already exists in table '{}'",
                new_name,
                table_name
            );
        }

        let mut constraints = self.get_table_constraints(db_name, table_name).await?;
        for check in &constraints.check_constraints {
            if check.expr_sql.contains(old_name) {
                bail!(
                    "cannot rename column '{}' because check constraint '{}' must be rewritten manually",
                    old_name,
                    check.name
                );
            }
        }

        self.graph()
            .run(
                query(
                    "MATCH (db:Database {name: $db_name})-[:HAS_TABLE]->(t:Table {name: $table_name}) \
                     -[:HAS_COLUMN]->(c:Column {name: $old_name}) \
                     SET c.name = $new_name",
                )
                .param("db_name", db_name)
                .param("table_name", table_name)
                .param("old_name", old_name)
                .param("new_name", new_name),
            )
            .await
            .context("failed to rename column")?;

        if let Some(default_expr) = constraints.defaults.remove(old_name) {
            constraints
                .defaults
                .insert(new_name.to_string(), default_expr);
        }
        for column in &mut constraints.primary_key_columns {
            if column == old_name {
                *column = new_name.to_string();
            }
        }
        for unique in &mut constraints.unique_constraints {
            for column in &mut unique.columns {
                if column == old_name {
                    *column = new_name.to_string();
                }
            }
        }
        for foreign_key in &mut constraints.foreign_keys {
            for column in &mut foreign_key.columns {
                if column == old_name {
                    *column = new_name.to_string();
                }
            }
            if foreign_key.ref_db_name.as_deref() == Some(db_name)
                && foreign_key.ref_table_name == table_name
            {
                for column in &mut foreign_key.ref_columns {
                    if column == old_name {
                        *column = new_name.to_string();
                    }
                }
            }
        }
        self.set_table_constraints(db_name, table_name, &constraints)
            .await?;
        Ok(())
    }

    pub async fn drop_column(&self, db_name: &str, table_name: &str, col_name: &str) -> Result<()> {
        let cols = self.describe_table(db_name, table_name).await?;
        if cols.iter().all(|c| c.name != col_name) {
            bail!(
                "column '{}' does not exist in table '{}'",
                col_name,
                table_name
            );
        }

        let mut constraints = self.get_table_constraints(db_name, table_name).await?;
        if constraints
            .primary_key_columns
            .iter()
            .any(|c| c == col_name)
        {
            bail!(
                "cannot drop column '{}' because it is part of the primary key",
                col_name
            );
        }
        if constraints
            .unique_constraints
            .iter()
            .any(|uq| uq.columns.iter().any(|c| c == col_name))
        {
            bail!(
                "cannot drop column '{}' because it is part of a UNIQUE constraint",
                col_name
            );
        }
        if constraints.foreign_keys.iter().any(|fk| {
            fk.columns.iter().any(|c| c == col_name) || fk.ref_columns.iter().any(|c| c == col_name)
        }) {
            bail!(
                "cannot drop column '{}' because it participates in a FOREIGN KEY constraint",
                col_name
            );
        }
        if constraints
            .check_constraints
            .iter()
            .any(|check| check.expr_sql.contains(col_name))
        {
            bail!(
                "cannot drop column '{}' because it is referenced by a CHECK constraint",
                col_name
            );
        }
        constraints.defaults.remove(col_name);
        self.set_table_constraints(db_name, table_name, &constraints)
            .await?;

        self.graph()
            .run(
                query(
                    "MATCH (db:Database {name: $db_name})-[:HAS_TABLE]->(t:Table {name: $table_name}) \
                     -[:HAS_COLUMN]->(c:Column {name: $col_name}) \
                     DETACH DELETE c",
                )
                .param("db_name", db_name)
                .param("table_name", table_name)
                .param("col_name", col_name),
            )
            .await
            .context("failed to drop column")?;
        Ok(())
    }

    pub async fn set_column_default(
        &self,
        db_name: &str,
        table_name: &str,
        col_name: &str,
        default_expr: Option<String>,
    ) -> Result<()> {
        let cols = self.describe_table(db_name, table_name).await?;
        if cols.iter().all(|c| c.name != col_name) {
            bail!(
                "column '{}' does not exist in table '{}'",
                col_name,
                table_name
            );
        }
        let mut constraints = self.get_table_constraints(db_name, table_name).await?;
        match default_expr {
            Some(expr) => {
                constraints.defaults.insert(col_name.to_string(), expr);
            }
            None => {
                constraints.defaults.remove(col_name);
            }
        }
        self.set_table_constraints(db_name, table_name, &constraints)
            .await
    }

    pub async fn set_column_nullability(
        &self,
        db_name: &str,
        table_name: &str,
        col_name: &str,
        nullable: bool,
    ) -> Result<()> {
        self.graph()
            .run(
                query(
                    "MATCH (db:Database {name: $db_name})-[:HAS_TABLE]->(t:Table {name: $table_name}) \
                     -[:HAS_COLUMN]->(c:Column {name: $col_name}) \
                     SET c.nullable = $nullable",
                )
                .param("db_name", db_name)
                .param("table_name", table_name)
                .param("col_name", col_name)
                .param("nullable", nullable),
            )
            .await
            .context("failed to update column nullability")?;
        Ok(())
    }

    // ── INFORMATION_SCHEMA ──

    pub async fn information_schema_tables(
        &self,
        db_name: &str,
    ) -> Result<Vec<(String, String, String)>> {
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
                    if col.nullable {
                        "YES".to_string()
                    } else {
                        "NO".to_string()
                    },
                ));
            }
        }
        Ok(rows)
    }

    pub async fn information_schema_table_constraints(
        &self,
        db_name: &str,
    ) -> Result<Vec<(String, String, String, String, String)>> {
        let tables = self.show_tables(db_name).await?;
        let mut rows = Vec::new();
        for table in &tables {
            let constraints = self.get_table_constraints(db_name, &table.name).await?;
            if !constraints.primary_key_columns.is_empty() {
                rows.push((
                    db_name.to_string(),
                    "public".to_string(),
                    table.name.clone(),
                    format!("{}_pkey", table.name),
                    "PRIMARY KEY".to_string(),
                ));
            }
            for unique in constraints.unique_constraints {
                rows.push((
                    db_name.to_string(),
                    "public".to_string(),
                    table.name.clone(),
                    unique.name,
                    "UNIQUE".to_string(),
                ));
            }
            for foreign_key in constraints.foreign_keys {
                rows.push((
                    db_name.to_string(),
                    "public".to_string(),
                    table.name.clone(),
                    foreign_key.name,
                    "FOREIGN KEY".to_string(),
                ));
            }
            for check in constraints.check_constraints {
                rows.push((
                    db_name.to_string(),
                    "public".to_string(),
                    table.name.clone(),
                    check.name,
                    "CHECK".to_string(),
                ));
            }
        }
        rows.sort();
        Ok(rows)
    }

    pub async fn information_schema_key_column_usage(
        &self,
        db_name: &str,
    ) -> Result<Vec<(String, String, String, String, i64, String)>> {
        let tables = self.show_tables(db_name).await?;
        let mut rows = Vec::new();
        for table in &tables {
            let constraints = self.get_table_constraints(db_name, &table.name).await?;
            if !constraints.primary_key_columns.is_empty() {
                let constraint_name = format!("{}_pkey", table.name);
                for (idx, column) in constraints.primary_key_columns.iter().enumerate() {
                    rows.push((
                        db_name.to_string(),
                        "public".to_string(),
                        table.name.clone(),
                        column.clone(),
                        (idx + 1) as i64,
                        constraint_name.clone(),
                    ));
                }
            }
            for unique in &constraints.unique_constraints {
                for (idx, column) in unique.columns.iter().enumerate() {
                    rows.push((
                        db_name.to_string(),
                        "public".to_string(),
                        table.name.clone(),
                        column.clone(),
                        (idx + 1) as i64,
                        unique.name.clone(),
                    ));
                }
            }
            for foreign_key in &constraints.foreign_keys {
                for (idx, column) in foreign_key.columns.iter().enumerate() {
                    rows.push((
                        db_name.to_string(),
                        "public".to_string(),
                        table.name.clone(),
                        column.clone(),
                        (idx + 1) as i64,
                        foreign_key.name.clone(),
                    ));
                }
            }
        }
        rows.sort_by(|left, right| {
            left.2
                .cmp(&right.2)
                .then(left.5.cmp(&right.5))
                .then(left.4.cmp(&right.4))
                .then(left.3.cmp(&right.3))
        });
        Ok(rows)
    }

    pub async fn information_schema_referential_constraints(
        &self,
        db_name: &str,
    ) -> Result<
        Vec<(
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        )>,
    > {
        let tables = self.show_tables(db_name).await?;
        let mut rows = Vec::new();
        for table in &tables {
            let constraints = self.get_table_constraints(db_name, &table.name).await?;
            for foreign_key in &constraints.foreign_keys {
                let ref_db_name = foreign_key.ref_db_name.as_deref().unwrap_or(db_name);
                let ref_constraints = self
                    .get_table_constraints(ref_db_name, &foreign_key.ref_table_name)
                    .await?;
                let unique_constraint_name =
                    if ref_constraints.primary_key_columns == foreign_key.ref_columns {
                        format!("{}_pkey", foreign_key.ref_table_name)
                    } else if let Some(unique) = ref_constraints
                        .unique_constraints
                        .iter()
                        .find(|unique| unique.columns == foreign_key.ref_columns)
                    {
                        unique.name.clone()
                    } else {
                        format!("{}_key", foreign_key.ref_table_name)
                    };

                rows.push((
                    db_name.to_string(),
                    "public".to_string(),
                    foreign_key.name.clone(),
                    ref_db_name.to_string(),
                    "public".to_string(),
                    unique_constraint_name,
                    foreign_key.on_update.clone(),
                    foreign_key.on_delete.clone(),
                ));
            }
        }
        rows.sort();
        Ok(rows)
    }

    // ── CREATE INDEX ──

    /// Marks a column as indexed in Neo4j. Does not create index entries --
    /// the caller is responsible for backfilling via executor::index::backfill_index.
    pub async fn create_index(
        &self,
        db_name: &str,
        table_name: &str,
        index_name: &str,
        column_name: &str,
    ) -> Result<ColumnInfo> {
        let cols = self.describe_table(db_name, table_name).await?;
        let col = cols
            .iter()
            .find(|c| c.name == column_name)
            .context(format!(
                "column '{}' not found in table '{}'",
                column_name, table_name
            ))?;

        if col.indexed {
            bail!("column '{}' is already indexed", column_name);
        }

        // Check index name uniqueness across this table
        if cols
            .iter()
            .any(|c| c.index_name.as_deref() == Some(index_name))
        {
            bail!(
                "index '{}' already exists on table '{}'",
                index_name,
                table_name
            );
        }

        self.graph()
            .run(
                query(
                    "MATCH (db:Database {name: $db_name})-[:HAS_TABLE]->(t:Table {name: $table_name}) \
                     -[:HAS_COLUMN]->(c:Column {name: $col_name}) \
                     SET c.indexed = true, c.index_name = $index_name",
                )
                .param("db_name", db_name)
                .param("table_name", table_name)
                .param("col_name", column_name)
                .param("index_name", index_name),
            )
            .await
            .context("failed to mark column as indexed")?;

        info!(
            db_name,
            table_name, column_name, index_name, "index created"
        );
        Ok(col.clone())
    }

    // ── DROP INDEX ──

    /// Removes index metadata from a column. Returns the column name.
    /// The caller is responsible for deleting index entries via executor::index::drop_index_entries.
    pub async fn drop_index(
        &self,
        db_name: &str,
        table_name: &str,
        index_name: &str,
    ) -> Result<ColumnInfo> {
        let cols = self.describe_table(db_name, table_name).await?;
        let col = cols
            .iter()
            .find(|c| c.index_name.as_deref() == Some(index_name))
            .context(format!(
                "index '{}' not found on table '{}'",
                index_name, table_name
            ))?
            .clone();

        self.graph()
            .run(
                query(
                    "MATCH (db:Database {name: $db_name})-[:HAS_TABLE]->(t:Table {name: $table_name}) \
                     -[:HAS_COLUMN]->(c:Column {index_name: $index_name}) \
                     SET c.indexed = false \
                     REMOVE c.index_name",
                )
                .param("db_name", db_name)
                .param("table_name", table_name)
                .param("index_name", index_name),
            )
            .await
            .context("failed to remove index metadata")?;

        info!(
            db_name,
            table_name,
            index_name,
            col_name = col.name,
            "index dropped"
        );
        Ok(col)
    }

    // ── LIST INDEXES ──

    /// Returns (index_name, column_name) for all indexed columns in a table.
    pub async fn list_indexes(
        &self,
        db_name: &str,
        table_name: &str,
    ) -> Result<Vec<(String, String)>> {
        let cols = self.describe_table(db_name, table_name).await?;
        Ok(cols
            .into_iter()
            .filter_map(|c| {
                if c.indexed {
                    Some((c.index_name.unwrap_or_default(), c.name))
                } else {
                    None
                }
            })
            .collect())
    }

    /// Returns all indexed columns for a table.
    pub async fn get_indexed_columns(
        &self,
        db_name: &str,
        table_name: &str,
    ) -> Result<Vec<ColumnInfo>> {
        let cols = self.describe_table(db_name, table_name).await?;
        Ok(cols.into_iter().filter(|c| c.indexed).collect())
    }

    /// Lookup the PK column ID for a table by db_id and table_id.
    /// Used during WAL replay when we only have IDs, not names.
    pub async fn get_pk_col_id_by_table_id(
        &self,
        db_id: u32,
        table_id: u32,
    ) -> Result<Option<u32>> {
        let mut result = self
            .graph()
            .execute(
                query(
                    "MATCH (db:Database {id: $db_id})-[:HAS_TABLE]->(t:Table {id: $table_id}) \
                     -[:HAS_COLUMN]->(c:Column {is_pk: true}) \
                     RETURN c.id AS id",
                )
                .param("db_id", db_id as i64)
                .param("table_id", table_id as i64),
            )
            .await?;

        if let Some(row) = result.next().await? {
            let id: i64 = row.get("id")?;
            return Ok(Some(id as u32));
        }
        Ok(None)
    }
}
