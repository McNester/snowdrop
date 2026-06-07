use crate::ddl::check_constraint_references_column;
use crate::{ColumnDefinition, ColumnInfo, DatabaseInfo, TableConstraintsInfo, TableInfo};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tracing::{info, warn};

/// Runtime catalog backend selector.
///
/// `Neo4j` keeps the existing behavior. `Sqlite` and `Mirror` are the
/// foundation for the embedded catalog migration; the query layer is moved to
/// this abstraction incrementally so the current pgwire surface stays stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogBackend {
    Neo4j,
    Sqlite,
    Mirror,
}

impl CatalogBackend {
    pub fn from_env_value(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "sqlite" | "embedded" => Self::Sqlite,
            "mirror" => Self::Mirror,
            "neo4j" | "" => Self::Neo4j,
            other => {
                warn!(
                    value = other,
                    "unknown SNOWDROP_CATALOG_BACKEND, falling back to neo4j"
                );
                Self::Neo4j
            }
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Neo4j => "neo4j",
            Self::Sqlite => "sqlite",
            Self::Mirror => "mirror",
        }
    }

    pub fn needs_sqlite(self) -> bool {
        matches!(self, Self::Sqlite | Self::Mirror)
    }
}

#[derive(Debug, Clone)]
pub struct CatalogConfig {
    pub backend: CatalogBackend,
    pub sqlite_path: PathBuf,
}

impl CatalogConfig {
    pub fn from_env() -> Self {
        let backend = std::env::var("SNOWDROP_CATALOG_BACKEND")
            .map(|raw| CatalogBackend::from_env_value(&raw))
            .unwrap_or(CatalogBackend::Neo4j);
        let sqlite_path = std::env::var("SQLITE_CATALOG_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/data/catalog/catalog.sqlite"));

        Self {
            backend,
            sqlite_path,
        }
    }
}

#[async_trait]
pub trait CatalogStore: Send + Sync {
    fn backend_name(&self) -> &'static str;
    async fn ping(&self) -> Result<()>;
    async fn init_catalog(&self) -> Result<()>;
    async fn create_database(&self, name: &str) -> Result<DatabaseInfo>;
    async fn put_database(&self, database: &DatabaseInfo) -> Result<()>;
    async fn get_database(&self, name: &str) -> Result<Option<DatabaseInfo>>;
    async fn list_databases(&self) -> Result<Vec<DatabaseInfo>>;
    async fn delete_database(&self, name: &str) -> Result<()>;
    async fn create_table(
        &self,
        db_name: &str,
        table_name: &str,
        columns: Vec<ColumnDefinition>,
        constraints: TableConstraintsInfo,
    ) -> Result<(TableInfo, Vec<ColumnInfo>)>;
    async fn put_table(&self, table: &TableInfo) -> Result<()>;
    async fn get_table(&self, db_id: u32, name: &str) -> Result<Option<TableInfo>>;
    async fn list_tables(&self, db_id: u32) -> Result<Vec<TableInfo>>;
    async fn delete_table(&self, table_id: u32) -> Result<()>;
    async fn replace_columns(&self, table_id: u32, columns: &[ColumnInfo]) -> Result<()>;
    async fn list_columns(&self, table_id: u32) -> Result<Vec<ColumnInfo>>;
    async fn add_column(
        &self,
        db_name: &str,
        table_name: &str,
        column_name: &str,
        data_type: &str,
        nullable: bool,
    ) -> Result<ColumnInfo>;
    async fn rename_column(
        &self,
        db_name: &str,
        table_name: &str,
        old_name: &str,
        new_name: &str,
    ) -> Result<()>;
    async fn drop_column(&self, db_name: &str, table_name: &str, column_name: &str) -> Result<()>;
    async fn set_column_default(
        &self,
        db_name: &str,
        table_name: &str,
        column_name: &str,
        default_expr: Option<String>,
    ) -> Result<()>;
    async fn set_column_nullability(
        &self,
        db_name: &str,
        table_name: &str,
        column_name: &str,
        nullable: bool,
    ) -> Result<()>;
    async fn create_index(
        &self,
        db_name: &str,
        table_name: &str,
        index_name: &str,
        column_name: &str,
    ) -> Result<ColumnInfo>;
    async fn put_constraints(
        &self,
        table_id: u32,
        constraints: &TableConstraintsInfo,
    ) -> Result<()>;
    async fn get_constraints(&self, table_id: u32) -> Result<TableConstraintsInfo>;
}

pub struct SQLiteCatalogStore {
    path: PathBuf,
    conn: Mutex<Connection>,
}

impl SQLiteCatalogStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create catalog directory {}", parent.display())
            })?;
        }

        let conn = Connection::open(&path)
            .with_context(|| format!("failed to open SQLite catalog {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .context("failed to enable SQLite WAL mode")?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .context("failed to enable SQLite foreign keys")?;

        Ok(Self {
            path,
            conn: Mutex::new(conn),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("SQLite catalog connection lock poisoned"))?;
        f(&conn)
    }

    fn init_schema_sync(&self) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS catalog_meta (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL,
                    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );

                CREATE TABLE IF NOT EXISTS catalog_databases (
                    database_id TEXT PRIMARY KEY,
                    database_name TEXT NOT NULL UNIQUE,
                    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );

                CREATE TABLE IF NOT EXISTS catalog_tables (
                    table_id TEXT PRIMARY KEY,
                    database_id TEXT NOT NULL REFERENCES catalog_databases(database_id) ON DELETE CASCADE,
                    table_name TEXT NOT NULL,
                    schema_version INTEGER NOT NULL DEFAULT 1,
                    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    UNIQUE(database_id, table_name)
                );

                CREATE TABLE IF NOT EXISTS catalog_columns (
                    column_id TEXT PRIMARY KEY,
                    table_id TEXT NOT NULL REFERENCES catalog_tables(table_id) ON DELETE CASCADE,
                    column_name TEXT NOT NULL,
                    ordinal_position INTEGER NOT NULL,
                    type_name TEXT NOT NULL,
                    nullable INTEGER NOT NULL,
                    default_expr TEXT,
                    is_primary_key INTEGER NOT NULL DEFAULT 0,
                    is_indexed INTEGER NOT NULL DEFAULT 0,
                    index_name TEXT,
                    UNIQUE(table_id, column_name),
                    UNIQUE(table_id, ordinal_position)
                );

                CREATE TABLE IF NOT EXISTS catalog_constraints (
                    constraint_id TEXT PRIMARY KEY,
                    table_id TEXT NOT NULL REFERENCES catalog_tables(table_id) ON DELETE CASCADE,
                    constraint_name TEXT NOT NULL,
                    constraint_type TEXT NOT NULL,
                    column_names_json TEXT NOT NULL,
                    referenced_table_id TEXT,
                    referenced_column_names_json TEXT,
                    expression_sql TEXT,
                    UNIQUE(table_id, constraint_name)
                );

                CREATE TABLE IF NOT EXISTS catalog_indexes (
                    index_id TEXT PRIMARY KEY,
                    table_id TEXT NOT NULL REFERENCES catalog_tables(table_id) ON DELETE CASCADE,
                    index_name TEXT NOT NULL,
                    column_names_json TEXT NOT NULL,
                    is_unique INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    UNIQUE(table_id, index_name)
                );

                CREATE TABLE IF NOT EXISTS catalog_statistics (
                    table_id TEXT NOT NULL REFERENCES catalog_tables(table_id) ON DELETE CASCADE,
                    column_name TEXT NOT NULL,
                    row_count INTEGER,
                    null_count INTEGER,
                    distinct_count INTEGER,
                    min_value TEXT,
                    max_value TEXT,
                    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    PRIMARY KEY(table_id, column_name)
                );

                CREATE TABLE IF NOT EXISTS catalog_migration_artifacts (
                    artifact_id TEXT PRIMARY KEY,
                    source_database TEXT NOT NULL,
                    target_database TEXT NOT NULL,
                    artifact_type TEXT NOT NULL,
                    artifact_path TEXT NOT NULL,
                    status TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );

                CREATE INDEX IF NOT EXISTS idx_catalog_tables_database
                    ON catalog_tables(database_id);
                CREATE INDEX IF NOT EXISTS idx_catalog_columns_table
                    ON catalog_columns(table_id, ordinal_position);
                CREATE INDEX IF NOT EXISTS idx_catalog_constraints_table
                    ON catalog_constraints(table_id);
                CREATE INDEX IF NOT EXISTS idx_catalog_indexes_table
                    ON catalog_indexes(table_id);
                "#,
            )
            .context("failed to initialize SQLite catalog schema")?;

            ensure_column_exists(conn, "catalog_columns", "index_name", "TEXT")?;

            conn.execute(
                "INSERT INTO catalog_meta(key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP",
                params!["schema_version", "1"],
            )
            .context("failed to write SQLite catalog schema version")?;

            Ok(())
        })
    }
}

#[async_trait]
impl CatalogStore for SQLiteCatalogStore {
    fn backend_name(&self) -> &'static str {
        "sqlite"
    }

    async fn ping(&self) -> Result<()> {
        self.with_conn(|conn| {
            let value: i64 = conn
                .query_row("SELECT 1", [], |row| row.get(0))
                .context("SQLite catalog ping query failed")?;
            anyhow::ensure!(value == 1, "SQLite catalog ping returned {value}");
            Ok(())
        })
    }

    async fn init_catalog(&self) -> Result<()> {
        self.init_schema_sync()?;
        info!(path = %self.path.display(), "SQLite catalog initialized");
        Ok(())
    }

    async fn create_database(&self, name: &str) -> Result<DatabaseInfo> {
        self.with_conn(|conn| {
            if database_by_name_sync(conn, name)?.is_some() {
                bail!("database '{}' already exists", name);
            }

            let id = next_numeric_id(conn, "catalog_databases", "database_id")?;
            let database = DatabaseInfo {
                id,
                name: name.to_string(),
            };
            conn.execute(
                "INSERT INTO catalog_databases(database_id, database_name) VALUES (?1, ?2)",
                params![database.id.to_string(), &database.name],
            )
            .context("failed to create database in SQLite catalog")?;
            Ok(database)
        })
    }

    async fn put_database(&self, database: &DatabaseInfo) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO catalog_databases(database_id, database_name)
                 VALUES (?1, ?2)
                 ON CONFLICT(database_id) DO UPDATE SET database_name = excluded.database_name",
                params![database.id.to_string(), &database.name],
            )
            .context("failed to upsert database into SQLite catalog")?;
            Ok(())
        })
    }

    async fn get_database(&self, name: &str) -> Result<Option<DatabaseInfo>> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT database_id, database_name FROM catalog_databases WHERE database_name = ?1",
                params![name],
                |row| {
                    let id: String = row.get(0)?;
                    Ok(DatabaseInfo {
                        id: id.parse().unwrap_or_default(),
                        name: row.get(1)?,
                    })
                },
            )
            .optional()
            .context("failed to load database from SQLite catalog")
        })
    }

    async fn list_databases(&self) -> Result<Vec<DatabaseInfo>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT database_id, database_name
                     FROM catalog_databases
                     ORDER BY database_name",
                )
                .context("failed to prepare SQLite catalog database list query")?;
            let rows = stmt
                .query_map([], |row| {
                    let id: String = row.get(0)?;
                    Ok(DatabaseInfo {
                        id: id.parse().unwrap_or_default(),
                        name: row.get(1)?,
                    })
                })
                .context("failed to list SQLite catalog databases")?;

            let mut databases = Vec::new();
            for row in rows {
                databases.push(row.context("failed to decode SQLite catalog database")?);
            }
            Ok(databases)
        })
    }

    async fn delete_database(&self, name: &str) -> Result<()> {
        self.with_conn(|conn| {
            let deleted = conn
                .execute(
                    "DELETE FROM catalog_databases WHERE database_name = ?1",
                    params![name],
                )
                .context("failed to delete database from SQLite catalog")?;
            if deleted == 0 {
                bail!("database '{}' does not exist", name);
            }
            Ok(())
        })
    }

    async fn create_table(
        &self,
        db_name: &str,
        table_name: &str,
        columns: Vec<ColumnDefinition>,
        constraints: TableConstraintsInfo,
    ) -> Result<(TableInfo, Vec<ColumnInfo>)> {
        self.with_conn(|conn| {
            if constraints.primary_key_columns.is_empty() {
                bail!("table must have at least one PRIMARY KEY column");
            }
            let db = database_by_name_sync(conn, db_name)?
                .with_context(|| format!("database '{}' does not exist", db_name))?;
            if table_by_name_sync(conn, db.id, table_name)?.is_some() {
                bail!(
                    "table '{}' already exists in database '{}'",
                    table_name,
                    db_name
                );
            }

            let table_id = next_numeric_id(conn, "catalog_tables", "table_id")?;
            let first_col_id = next_numeric_id(conn, "catalog_columns", "column_id")?;
            let table = TableInfo {
                id: table_id,
                name: table_name.to_string(),
                db_id: db.id,
            };

            let tx = conn
                .unchecked_transaction()
                .context("failed to start SQLite catalog create table transaction")?;
            tx.execute(
                "INSERT INTO catalog_tables(table_id, database_id, table_name)
                 VALUES (?1, ?2, ?3)",
                params![table.id.to_string(), table.db_id.to_string(), &table.name],
            )
            .context("failed to create table in SQLite catalog")?;

            let mut column_infos = Vec::with_capacity(columns.len());
            for (idx, column) in columns.iter().enumerate() {
                let info = ColumnInfo {
                    id: first_col_id + idx as u32,
                    name: column.name.clone(),
                    data_type: column.data_type.clone(),
                    nullable: column.nullable,
                    is_pk: column.is_pk,
                    table_id,
                    indexed: false,
                    index_name: None,
                };
                tx.execute(
                    "INSERT INTO catalog_columns(
                        column_id,
                        table_id,
                        column_name,
                        ordinal_position,
                        type_name,
                        nullable,
                        is_primary_key,
                        is_indexed,
                        index_name
                    )
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        info.id.to_string(),
                        table_id.to_string(),
                        &info.name,
                        idx as i64,
                        &info.data_type,
                        info.nullable as i64,
                        info.is_pk as i64,
                        info.indexed as i64,
                        &info.index_name,
                    ],
                )
                .with_context(|| {
                    format!("failed to create column '{}' in SQLite catalog", info.name)
                })?;
                column_infos.push(info);
            }

            insert_constraints_sync(&tx, table_id, &constraints)?;
            tx.commit()
                .context("failed to commit SQLite catalog create table transaction")?;
            Ok((table, column_infos))
        })
    }

    async fn put_table(&self, table: &TableInfo) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO catalog_tables(table_id, database_id, table_name)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(table_id) DO UPDATE SET
                    database_id = excluded.database_id,
                    table_name = excluded.table_name,
                    schema_version = schema_version + 1",
                params![table.id.to_string(), table.db_id.to_string(), &table.name],
            )
            .context("failed to upsert table into SQLite catalog")?;
            Ok(())
        })
    }

    async fn get_table(&self, db_id: u32, name: &str) -> Result<Option<TableInfo>> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT table_id, table_name, database_id
                 FROM catalog_tables
                 WHERE database_id = ?1 AND table_name = ?2",
                params![db_id.to_string(), name],
                |row| {
                    let table_id: String = row.get(0)?;
                    let database_id: String = row.get(2)?;
                    Ok(TableInfo {
                        id: table_id.parse().unwrap_or_default(),
                        name: row.get(1)?,
                        db_id: database_id.parse().unwrap_or_default(),
                    })
                },
            )
            .optional()
            .context("failed to load table from SQLite catalog")
        })
    }

    async fn list_tables(&self, db_id: u32) -> Result<Vec<TableInfo>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT table_id, table_name, database_id
                     FROM catalog_tables
                     WHERE database_id = ?1
                     ORDER BY table_name",
                )
                .context("failed to prepare SQLite catalog table list query")?;
            let rows = stmt
                .query_map(params![db_id.to_string()], |row| {
                    let table_id: String = row.get(0)?;
                    let database_id: String = row.get(2)?;
                    Ok(TableInfo {
                        id: table_id.parse().unwrap_or_default(),
                        name: row.get(1)?,
                        db_id: database_id.parse().unwrap_or_default(),
                    })
                })
                .context("failed to list SQLite catalog tables")?;

            let mut tables = Vec::new();
            for row in rows {
                tables.push(row.context("failed to decode SQLite catalog table")?);
            }
            Ok(tables)
        })
    }

    async fn delete_table(&self, table_id: u32) -> Result<()> {
        self.with_conn(|conn| {
            let deleted = conn
                .execute(
                    "DELETE FROM catalog_tables WHERE table_id = ?1",
                    params![table_id.to_string()],
                )
                .context("failed to delete table from SQLite catalog")?;
            if deleted == 0 {
                bail!("table id '{}' does not exist", table_id);
            }
            Ok(())
        })
    }

    async fn replace_columns(&self, table_id: u32, columns: &[ColumnInfo]) -> Result<()> {
        self.with_conn(|conn| {
            let tx = conn
                .unchecked_transaction()
                .context("failed to start SQLite catalog column transaction")?;
            tx.execute(
                "DELETE FROM catalog_columns WHERE table_id = ?1",
                params![table_id.to_string()],
            )
            .context("failed to clear SQLite catalog columns")?;

            for (idx, column) in columns.iter().enumerate() {
                tx.execute(
                    "INSERT INTO catalog_columns(
                        column_id,
                        table_id,
                        column_name,
                        ordinal_position,
                        type_name,
                        nullable,
                        is_primary_key,
                        is_indexed,
                        index_name
                    )
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        column.id.to_string(),
                        table_id.to_string(),
                        &column.name,
                        idx as i64,
                        &column.data_type,
                        column.nullable as i64,
                        column.is_pk as i64,
                        column.indexed as i64,
                        &column.index_name,
                    ],
                )
                .with_context(|| {
                    format!(
                        "failed to insert column '{}' into SQLite catalog",
                        column.name
                    )
                })?;
            }

            tx.commit()
                .context("failed to commit SQLite catalog column transaction")?;
            Ok(())
        })
    }

    async fn list_columns(&self, table_id: u32) -> Result<Vec<ColumnInfo>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT column_id, column_name, type_name, nullable,
                            is_primary_key, is_indexed, index_name
                     FROM catalog_columns
                     WHERE table_id = ?1
                     ORDER BY ordinal_position",
                )
                .context("failed to prepare SQLite catalog column query")?;
            let rows = stmt
                .query_map(params![table_id.to_string()], |row| {
                    let id: String = row.get(0)?;
                    Ok(ColumnInfo {
                        id: id.parse().unwrap_or_default(),
                        name: row.get(1)?,
                        data_type: row.get(2)?,
                        nullable: row.get::<_, i64>(3)? != 0,
                        is_pk: row.get::<_, i64>(4)? != 0,
                        table_id,
                        indexed: row.get::<_, i64>(5)? != 0,
                        index_name: row.get(6)?,
                    })
                })
                .context("failed to query SQLite catalog columns")?;

            let mut columns = Vec::new();
            for row in rows {
                columns.push(row.context("failed to decode SQLite catalog column")?);
            }
            Ok(columns)
        })
    }

    async fn add_column(
        &self,
        db_name: &str,
        table_name: &str,
        column_name: &str,
        data_type: &str,
        nullable: bool,
    ) -> Result<ColumnInfo> {
        self.with_conn(|conn| {
            let table = table_by_db_and_name_sync(conn, db_name, table_name)?;
            if column_by_name_sync(conn, table.id, column_name)?.is_some() {
                bail!(
                    "column '{}' already exists in table '{}'",
                    column_name,
                    table_name
                );
            }

            let id = next_numeric_id(conn, "catalog_columns", "column_id")?;
            let ordinal_position: i64 = conn
                .query_row(
                    "SELECT COALESCE(MAX(ordinal_position), -1) + 1
                     FROM catalog_columns
                     WHERE table_id = ?1",
                    params![table.id.to_string()],
                    |row| row.get(0),
                )
                .context("failed to allocate SQLite catalog column ordinal")?;
            let column = ColumnInfo {
                id,
                name: column_name.to_string(),
                data_type: data_type.to_string(),
                nullable,
                is_pk: false,
                table_id: table.id,
                indexed: false,
                index_name: None,
            };

            conn.execute(
                "INSERT INTO catalog_columns(
                    column_id,
                    table_id,
                    column_name,
                    ordinal_position,
                    type_name,
                    nullable,
                    is_primary_key,
                    is_indexed,
                    index_name
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    column.id.to_string(),
                    table.id.to_string(),
                    &column.name,
                    ordinal_position,
                    &column.data_type,
                    column.nullable as i64,
                    column.is_pk as i64,
                    column.indexed as i64,
                    &column.index_name,
                ],
            )
            .context("failed to add column to SQLite catalog")?;
            Ok(column)
        })
    }

    async fn rename_column(
        &self,
        db_name: &str,
        table_name: &str,
        old_name: &str,
        new_name: &str,
    ) -> Result<()> {
        self.with_conn(|conn| {
            let table = table_by_db_and_name_sync(conn, db_name, table_name)?;
            if column_by_name_sync(conn, table.id, old_name)?.is_none() {
                bail!(
                    "column '{}' does not exist in table '{}'",
                    old_name,
                    table_name
                );
            }
            if column_by_name_sync(conn, table.id, new_name)?.is_some() {
                bail!(
                    "column '{}' already exists in table '{}'",
                    new_name,
                    table_name
                );
            }

            let mut constraints = constraints_by_table_id_sync(conn, table.id)?;
            for check in &constraints.check_constraints {
                if check_constraint_references_column(&check.expr_sql, old_name)? {
                    bail!(
                        "cannot rename column '{}' because check constraint '{}' must be rewritten manually",
                        old_name,
                        check.name
                    );
                }
            }

            let tx = conn
                .unchecked_transaction()
                .context("failed to start SQLite catalog rename column transaction")?;
            tx.execute(
                "UPDATE catalog_columns
                 SET column_name = ?1
                 WHERE table_id = ?2 AND column_name = ?3",
                params![new_name, table.id.to_string(), old_name],
            )
            .context("failed to rename column in SQLite catalog")?;

            rename_column_in_constraints(&mut constraints, db_name, table_name, old_name, new_name);
            replace_constraints_sync(&tx, table.id, &constraints)?;
            tx.commit()
                .context("failed to commit SQLite catalog rename column transaction")?;
            Ok(())
        })
    }

    async fn drop_column(&self, db_name: &str, table_name: &str, column_name: &str) -> Result<()> {
        self.with_conn(|conn| {
            let table = table_by_db_and_name_sync(conn, db_name, table_name)?;
            if column_by_name_sync(conn, table.id, column_name)?.is_none() {
                bail!(
                    "column '{}' does not exist in table '{}'",
                    column_name,
                    table_name
                );
            }

            let mut constraints = constraints_by_table_id_sync(conn, table.id)?;
            ensure_column_can_be_dropped(&constraints, column_name)?;
            constraints.defaults.remove(column_name);

            let tx = conn
                .unchecked_transaction()
                .context("failed to start SQLite catalog drop column transaction")?;
            replace_constraints_sync(&tx, table.id, &constraints)?;
            tx.execute(
                "DELETE FROM catalog_indexes
                 WHERE table_id = ?1
                   AND EXISTS (
                     SELECT 1
                     FROM catalog_columns c
                     WHERE c.table_id = catalog_indexes.table_id
                       AND c.column_name = ?2
                       AND c.index_name = catalog_indexes.index_name
                   )",
                params![table.id.to_string(), column_name],
            )
            .context("failed to delete column index metadata from SQLite catalog")?;
            let deleted = tx
                .execute(
                    "DELETE FROM catalog_columns WHERE table_id = ?1 AND column_name = ?2",
                    params![table.id.to_string(), column_name],
                )
                .context("failed to drop column from SQLite catalog")?;
            if deleted == 0 {
                bail!(
                    "column '{}' does not exist in table '{}'",
                    column_name,
                    table_name
                );
            }
            tx.commit()
                .context("failed to commit SQLite catalog drop column transaction")?;
            Ok(())
        })
    }

    async fn set_column_default(
        &self,
        db_name: &str,
        table_name: &str,
        column_name: &str,
        default_expr: Option<String>,
    ) -> Result<()> {
        self.with_conn(|conn| {
            let table = table_by_db_and_name_sync(conn, db_name, table_name)?;
            if column_by_name_sync(conn, table.id, column_name)?.is_none() {
                bail!(
                    "column '{}' does not exist in table '{}'",
                    column_name,
                    table_name
                );
            }

            let mut constraints = constraints_by_table_id_sync(conn, table.id)?;
            match default_expr {
                Some(expr) => {
                    constraints.defaults.insert(column_name.to_string(), expr);
                }
                None => {
                    constraints.defaults.remove(column_name);
                }
            }
            let tx = conn
                .unchecked_transaction()
                .context("failed to start SQLite catalog default transaction")?;
            replace_constraints_sync(&tx, table.id, &constraints)?;
            tx.commit()
                .context("failed to commit SQLite catalog default transaction")?;
            Ok(())
        })
    }

    async fn set_column_nullability(
        &self,
        db_name: &str,
        table_name: &str,
        column_name: &str,
        nullable: bool,
    ) -> Result<()> {
        self.with_conn(|conn| {
            let table = table_by_db_and_name_sync(conn, db_name, table_name)?;
            let updated = conn
                .execute(
                    "UPDATE catalog_columns
                     SET nullable = ?1
                     WHERE table_id = ?2 AND column_name = ?3",
                    params![nullable as i64, table.id.to_string(), column_name],
                )
                .context("failed to update SQLite catalog column nullability")?;
            if updated == 0 {
                bail!(
                    "column '{}' does not exist in table '{}'",
                    column_name,
                    table_name
                );
            }
            Ok(())
        })
    }

    async fn create_index(
        &self,
        db_name: &str,
        table_name: &str,
        index_name: &str,
        column_name: &str,
    ) -> Result<ColumnInfo> {
        self.with_conn(|conn| {
            let table = table_by_db_and_name_sync(conn, db_name, table_name)?;
            let mut column =
                column_by_name_sync(conn, table.id, column_name)?.with_context(|| {
                    format!(
                        "column '{}' not found in table '{}'",
                        column_name, table_name
                    )
                })?;
            if column.indexed {
                bail!("column '{}' is already indexed", column_name);
            }
            if index_by_name_sync(conn, table.id, index_name)?.is_some() {
                bail!(
                    "index '{}' already exists on table '{}'",
                    index_name,
                    table_name
                );
            }

            let tx = conn
                .unchecked_transaction()
                .context("failed to start SQLite catalog create index transaction")?;
            tx.execute(
                "UPDATE catalog_columns
                 SET is_indexed = 1, index_name = ?1
                 WHERE table_id = ?2 AND column_name = ?3",
                params![index_name, table.id.to_string(), column_name],
            )
            .context("failed to mark SQLite catalog column as indexed")?;
            tx.execute(
                "INSERT INTO catalog_indexes(
                    index_id,
                    table_id,
                    index_name,
                    column_names_json,
                    is_unique
                )
                VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    format!("{}:{}", table.id, index_name),
                    table.id.to_string(),
                    index_name,
                    serde_json::to_string(&vec![column_name.to_string()])
                        .context("failed to encode index columns")?,
                    0i64,
                ],
            )
            .context("failed to store SQLite catalog index metadata")?;
            tx.commit()
                .context("failed to commit SQLite catalog create index transaction")?;

            column.indexed = true;
            column.index_name = Some(index_name.to_string());
            Ok(column)
        })
    }

    async fn put_constraints(
        &self,
        table_id: u32,
        constraints: &TableConstraintsInfo,
    ) -> Result<()> {
        self.with_conn(|conn| {
            let tx = conn
                .unchecked_transaction()
                .context("failed to start SQLite catalog constraints transaction")?;
            tx.execute(
                "DELETE FROM catalog_constraints WHERE table_id = ?1",
                params![table_id.to_string()],
            )
            .context("failed to clear SQLite catalog constraints")?;
            insert_constraints_sync(&tx, table_id, constraints)?;
            tx.commit()
                .context("failed to commit SQLite catalog constraints transaction")?;
            Ok(())
        })
    }

    async fn get_constraints(&self, table_id: u32) -> Result<TableConstraintsInfo> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT constraint_name, constraint_type, column_names_json,
                            referenced_table_id, referenced_column_names_json, expression_sql
                     FROM catalog_constraints
                     WHERE table_id = ?1
                     ORDER BY constraint_type, constraint_name",
                )
                .context("failed to prepare SQLite catalog constraints query")?;
            let rows = stmt
                .query_map(params![table_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                })
                .context("failed to query SQLite catalog constraints")?;

            let mut constraints = TableConstraintsInfo::default();
            for row in rows {
                let (
                    name,
                    constraint_type,
                    columns_json,
                    referenced_table,
                    referenced_columns_json,
                    expression_sql,
                ) = row.context("failed to decode SQLite catalog constraint")?;
                match constraint_type.as_str() {
                    "PRIMARY KEY" => {
                        constraints.primary_key_columns = serde_json::from_str(&columns_json)
                            .context("failed to decode primary key columns")?;
                    }
                    "DEFAULT" => {
                        let columns: Vec<String> = serde_json::from_str(&columns_json)
                            .context("failed to decode default column")?;
                        if let (Some(column), Some(expr)) = (columns.first(), expression_sql) {
                            constraints.defaults.insert(column.clone(), expr);
                        }
                    }
                    "UNIQUE" => constraints
                        .unique_constraints
                        .push(crate::UniqueConstraintInfo {
                            name,
                            columns: serde_json::from_str(&columns_json)
                                .context("failed to decode unique columns")?,
                        }),
                    "FOREIGN KEY" => {
                        if let Some(expr) = expression_sql {
                            constraints.foreign_keys.push(
                                serde_json::from_str(&expr)
                                    .context("failed to decode foreign key payload")?,
                            );
                        } else {
                            constraints
                                .foreign_keys
                                .push(crate::ForeignKeyConstraintInfo {
                                    name,
                                    columns: serde_json::from_str(&columns_json)
                                        .context("failed to decode foreign key columns")?,
                                    ref_db_name: None,
                                    ref_table_name: referenced_table.unwrap_or_default(),
                                    ref_columns: referenced_columns_json
                                        .as_deref()
                                        .map(serde_json::from_str)
                                        .transpose()
                                        .context("failed to decode foreign key referenced columns")?
                                        .unwrap_or_default(),
                                    on_delete: "NO ACTION".to_string(),
                                    on_update: "NO ACTION".to_string(),
                                });
                        }
                    }
                    "CHECK" => constraints
                        .check_constraints
                        .push(crate::CheckConstraintInfo {
                            name,
                            expr_sql: expression_sql.unwrap_or_default(),
                        }),
                    _ => {}
                }
            }
            Ok(constraints)
        })
    }
}

fn ensure_column_exists(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
    column_type: &str,
) -> Result<()> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table_name})"))
        .with_context(|| format!("failed to inspect SQLite table {table_name}"))?;
    let mut rows = stmt
        .query([])
        .with_context(|| format!("failed to inspect SQLite table {table_name}"))?;
    while let Some(row) = rows.next()? {
        let existing: String = row.get(1)?;
        if existing == column_name {
            return Ok(());
        }
    }

    conn.execute(
        &format!("ALTER TABLE {table_name} ADD COLUMN {column_name} {column_type}"),
        [],
    )
    .with_context(|| format!("failed to add {table_name}.{column_name}"))?;
    Ok(())
}

fn next_numeric_id(conn: &Connection, table_name: &str, id_column: &str) -> Result<u32> {
    let sql =
        format!("SELECT COALESCE(MAX(CAST({id_column} AS INTEGER)), 0) + 1 FROM {table_name}");
    let id: i64 = conn
        .query_row(&sql, [], |row| row.get(0))
        .with_context(|| format!("failed to allocate next id for {table_name}.{id_column}"))?;
    Ok(id as u32)
}

fn database_by_name_sync(conn: &Connection, name: &str) -> Result<Option<DatabaseInfo>> {
    conn.query_row(
        "SELECT database_id, database_name FROM catalog_databases WHERE database_name = ?1",
        params![name],
        |row| {
            let id: String = row.get(0)?;
            Ok(DatabaseInfo {
                id: id.parse().unwrap_or_default(),
                name: row.get(1)?,
            })
        },
    )
    .optional()
    .context("failed to load database from SQLite catalog")
}

fn table_by_name_sync(conn: &Connection, db_id: u32, name: &str) -> Result<Option<TableInfo>> {
    conn.query_row(
        "SELECT table_id, table_name, database_id
         FROM catalog_tables
         WHERE database_id = ?1 AND table_name = ?2",
        params![db_id.to_string(), name],
        |row| {
            let table_id: String = row.get(0)?;
            let database_id: String = row.get(2)?;
            Ok(TableInfo {
                id: table_id.parse().unwrap_or_default(),
                name: row.get(1)?,
                db_id: database_id.parse().unwrap_or_default(),
            })
        },
    )
    .optional()
    .context("failed to load table from SQLite catalog")
}

fn table_by_db_and_name_sync(
    conn: &Connection,
    db_name: &str,
    table_name: &str,
) -> Result<TableInfo> {
    let db = database_by_name_sync(conn, db_name)?
        .with_context(|| format!("database '{}' does not exist", db_name))?;
    table_by_name_sync(conn, db.id, table_name)?.with_context(|| {
        format!(
            "table '{}' does not exist in database '{}'",
            table_name, db_name
        )
    })
}

fn column_by_name_sync(
    conn: &Connection,
    table_id: u32,
    column_name: &str,
) -> Result<Option<ColumnInfo>> {
    conn.query_row(
        "SELECT column_id, column_name, type_name, nullable,
                is_primary_key, is_indexed, index_name
         FROM catalog_columns
         WHERE table_id = ?1 AND column_name = ?2",
        params![table_id.to_string(), column_name],
        |row| {
            let id: String = row.get(0)?;
            Ok(ColumnInfo {
                id: id.parse().unwrap_or_default(),
                name: row.get(1)?,
                data_type: row.get(2)?,
                nullable: row.get::<_, i64>(3)? != 0,
                is_pk: row.get::<_, i64>(4)? != 0,
                table_id,
                indexed: row.get::<_, i64>(5)? != 0,
                index_name: row.get(6)?,
            })
        },
    )
    .optional()
    .context("failed to load column from SQLite catalog")
}

fn index_by_name_sync(
    conn: &Connection,
    table_id: u32,
    index_name: &str,
) -> Result<Option<String>> {
    conn.query_row(
        "SELECT index_id
         FROM catalog_indexes
         WHERE table_id = ?1 AND index_name = ?2",
        params![table_id.to_string(), index_name],
        |row| row.get(0),
    )
    .optional()
    .context("failed to load index from SQLite catalog")
}

fn constraints_by_table_id_sync(conn: &Connection, table_id: u32) -> Result<TableConstraintsInfo> {
    let mut stmt = conn
        .prepare(
            "SELECT constraint_name, constraint_type, column_names_json,
                    referenced_table_id, referenced_column_names_json, expression_sql
             FROM catalog_constraints
             WHERE table_id = ?1
             ORDER BY constraint_type, constraint_name",
        )
        .context("failed to prepare SQLite catalog constraints query")?;
    let rows = stmt
        .query_map(params![table_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .context("failed to query SQLite catalog constraints")?;

    let mut constraints = TableConstraintsInfo::default();
    for row in rows {
        let (
            name,
            constraint_type,
            columns_json,
            referenced_table,
            referenced_columns_json,
            expression_sql,
        ) = row.context("failed to decode SQLite catalog constraint")?;
        match constraint_type.as_str() {
            "PRIMARY KEY" => {
                constraints.primary_key_columns = serde_json::from_str(&columns_json)
                    .context("failed to decode primary key columns")?;
            }
            "DEFAULT" => {
                let columns: Vec<String> = serde_json::from_str(&columns_json)
                    .context("failed to decode default column")?;
                if let (Some(column), Some(expr)) = (columns.first(), expression_sql) {
                    constraints.defaults.insert(column.clone(), expr);
                }
            }
            "UNIQUE" => constraints
                .unique_constraints
                .push(crate::UniqueConstraintInfo {
                    name,
                    columns: serde_json::from_str(&columns_json)
                        .context("failed to decode unique columns")?,
                }),
            "FOREIGN KEY" => {
                if let Some(expr) = expression_sql {
                    constraints.foreign_keys.push(
                        serde_json::from_str(&expr)
                            .context("failed to decode foreign key payload")?,
                    );
                } else {
                    constraints
                        .foreign_keys
                        .push(crate::ForeignKeyConstraintInfo {
                            name,
                            columns: serde_json::from_str(&columns_json)
                                .context("failed to decode foreign key columns")?,
                            ref_db_name: None,
                            ref_table_name: referenced_table.unwrap_or_default(),
                            ref_columns: referenced_columns_json
                                .as_deref()
                                .map(serde_json::from_str)
                                .transpose()
                                .context("failed to decode foreign key referenced columns")?
                                .unwrap_or_default(),
                            on_delete: "NO ACTION".to_string(),
                            on_update: "NO ACTION".to_string(),
                        });
                }
            }
            "CHECK" => constraints
                .check_constraints
                .push(crate::CheckConstraintInfo {
                    name,
                    expr_sql: expression_sql.unwrap_or_default(),
                }),
            _ => {}
        }
    }
    Ok(constraints)
}

fn replace_constraints_sync(
    tx: &Transaction<'_>,
    table_id: u32,
    constraints: &TableConstraintsInfo,
) -> Result<()> {
    tx.execute(
        "DELETE FROM catalog_constraints WHERE table_id = ?1",
        params![table_id.to_string()],
    )
    .context("failed to clear SQLite catalog constraints")?;
    insert_constraints_sync(tx, table_id, constraints)
}

fn rename_column_in_constraints(
    constraints: &mut TableConstraintsInfo,
    db_name: &str,
    table_name: &str,
    old_name: &str,
    new_name: &str,
) {
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
}

fn ensure_column_can_be_dropped(
    constraints: &TableConstraintsInfo,
    column_name: &str,
) -> Result<()> {
    if constraints
        .primary_key_columns
        .iter()
        .any(|column| column == column_name)
    {
        bail!(
            "cannot drop column '{}' because it is part of the primary key",
            column_name
        );
    }
    if constraints
        .unique_constraints
        .iter()
        .any(|unique| unique.columns.iter().any(|column| column == column_name))
    {
        bail!(
            "cannot drop column '{}' because it is part of a UNIQUE constraint",
            column_name
        );
    }
    if constraints.foreign_keys.iter().any(|foreign_key| {
        foreign_key
            .columns
            .iter()
            .any(|column| column == column_name)
            || foreign_key
                .ref_columns
                .iter()
                .any(|column| column == column_name)
    }) {
        bail!(
            "cannot drop column '{}' because it participates in a FOREIGN KEY constraint",
            column_name
        );
    }
    for check in &constraints.check_constraints {
        if check_constraint_references_column(&check.expr_sql, column_name)? {
            bail!(
                "cannot drop column '{}' because it is referenced by a CHECK constraint",
                column_name
            );
        }
    }
    Ok(())
}

fn insert_constraints_sync(
    tx: &Transaction<'_>,
    table_id: u32,
    constraints: &TableConstraintsInfo,
) -> Result<()> {
    let pk_json = serde_json::to_string(&constraints.primary_key_columns)
        .context("failed to encode primary key constraints")?;
    tx.execute(
        "INSERT INTO catalog_constraints(
            constraint_id,
            table_id,
            constraint_name,
            constraint_type,
            column_names_json
        )
        VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            format!("{table_id}:primary_key"),
            table_id.to_string(),
            "primary_key",
            "PRIMARY KEY",
            pk_json,
        ],
    )
    .context("failed to store primary key constraint")?;

    for (column_name, default_expr) in &constraints.defaults {
        tx.execute(
            "INSERT INTO catalog_constraints(
                constraint_id,
                table_id,
                constraint_name,
                constraint_type,
                column_names_json,
                expression_sql
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                format!("{table_id}:default:{column_name}"),
                table_id.to_string(),
                format!("{column_name}_default"),
                "DEFAULT",
                serde_json::to_string(&vec![column_name])
                    .context("failed to encode default column")?,
                default_expr,
            ],
        )
        .with_context(|| format!("failed to store default for column {column_name}"))?;
    }

    for unique in &constraints.unique_constraints {
        tx.execute(
            "INSERT INTO catalog_constraints(
                constraint_id,
                table_id,
                constraint_name,
                constraint_type,
                column_names_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                format!("{table_id}:unique:{}", unique.name),
                table_id.to_string(),
                &unique.name,
                "UNIQUE",
                serde_json::to_string(&unique.columns)
                    .context("failed to encode unique columns")?,
            ],
        )
        .with_context(|| format!("failed to store unique constraint {}", unique.name))?;
    }

    for fk in &constraints.foreign_keys {
        tx.execute(
            "INSERT INTO catalog_constraints(
                constraint_id,
                table_id,
                constraint_name,
                constraint_type,
                column_names_json,
                referenced_table_id,
                referenced_column_names_json,
                expression_sql
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                format!("{table_id}:foreign_key:{}", fk.name),
                table_id.to_string(),
                &fk.name,
                "FOREIGN KEY",
                serde_json::to_string(&fk.columns)
                    .context("failed to encode foreign key columns")?,
                &fk.ref_table_name,
                serde_json::to_string(&fk.ref_columns)
                    .context("failed to encode foreign key referenced columns")?,
                serde_json::to_string(fk).context("failed to encode foreign key")?,
            ],
        )
        .with_context(|| format!("failed to store foreign key {}", fk.name))?;
    }

    for check in &constraints.check_constraints {
        tx.execute(
            "INSERT INTO catalog_constraints(
                constraint_id,
                table_id,
                constraint_name,
                constraint_type,
                column_names_json,
                expression_sql
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                format!("{table_id}:check:{}", check.name),
                table_id.to_string(),
                &check.name,
                "CHECK",
                "[]",
                &check.expr_sql,
            ],
        )
        .with_context(|| format!("failed to store check constraint {}", check.name))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_catalog_backend_env_values() {
        assert_eq!(
            CatalogBackend::from_env_value("neo4j"),
            CatalogBackend::Neo4j
        );
        assert_eq!(
            CatalogBackend::from_env_value("sqlite"),
            CatalogBackend::Sqlite
        );
        assert_eq!(
            CatalogBackend::from_env_value("embedded"),
            CatalogBackend::Sqlite
        );
        assert_eq!(
            CatalogBackend::from_env_value("mirror"),
            CatalogBackend::Mirror
        );
        assert_eq!(
            CatalogBackend::from_env_value("unknown"),
            CatalogBackend::Neo4j
        );
    }

    #[tokio::test]
    async fn initializes_sqlite_catalog_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let catalog = SQLiteCatalogStore::open(&path).unwrap();

        catalog.init_catalog().await.unwrap();
        catalog.ping().await.unwrap();

        catalog
            .with_conn(|conn| {
                let count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'catalog_tables'",
                    [],
                    |row| row.get(0),
                )?;
                assert_eq!(count, 1);
                Ok(())
            })
            .unwrap();
    }

    #[tokio::test]
    async fn roundtrips_database_table_and_columns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let catalog = SQLiteCatalogStore::open(&path).unwrap();
        catalog.init_catalog().await.unwrap();

        let database = DatabaseInfo {
            id: 7,
            name: "app".to_string(),
        };
        let table = TableInfo {
            id: 42,
            name: "events".to_string(),
            db_id: database.id,
        };
        let columns = vec![
            ColumnInfo {
                id: 1,
                name: "id".to_string(),
                data_type: "BIGINT".to_string(),
                nullable: false,
                is_pk: true,
                table_id: table.id,
                indexed: true,
                index_name: Some("events_pkey".to_string()),
            },
            ColumnInfo {
                id: 2,
                name: "status".to_string(),
                data_type: "VARCHAR".to_string(),
                nullable: true,
                is_pk: false,
                table_id: table.id,
                indexed: false,
                index_name: None,
            },
        ];

        catalog.put_database(&database).await.unwrap();
        catalog.put_table(&table).await.unwrap();
        catalog.replace_columns(table.id, &columns).await.unwrap();

        assert_eq!(
            catalog.get_database("app").await.unwrap().unwrap().id,
            database.id
        );
        assert_eq!(
            catalog
                .get_table(database.id, "events")
                .await
                .unwrap()
                .unwrap()
                .id,
            table.id
        );
        assert_eq!(catalog.list_databases().await.unwrap().len(), 1);
        assert_eq!(catalog.list_tables(database.id).await.unwrap().len(), 1);
        assert_eq!(catalog.list_columns(table.id).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn creates_and_deletes_database_table_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let catalog = SQLiteCatalogStore::open(&path).unwrap();
        catalog.init_catalog().await.unwrap();

        let database = catalog.create_database("app").await.unwrap();
        assert_eq!(database.name, "app");

        let constraints = TableConstraintsInfo {
            primary_key_columns: vec!["id".to_string()],
            defaults: [("status".to_string(), "'open'".to_string())].into(),
            unique_constraints: vec![crate::UniqueConstraintInfo {
                name: "events_code_key".to_string(),
                columns: vec!["code".to_string()],
            }],
            foreign_keys: Vec::new(),
            check_constraints: Vec::new(),
        };
        let (table, columns) = catalog
            .create_table(
                "app",
                "events",
                vec![
                    ColumnDefinition {
                        name: "id".to_string(),
                        data_type: "BIGINT".to_string(),
                        nullable: false,
                        is_pk: true,
                    },
                    ColumnDefinition {
                        name: "status".to_string(),
                        data_type: "VARCHAR".to_string(),
                        nullable: true,
                        is_pk: false,
                    },
                    ColumnDefinition {
                        name: "code".to_string(),
                        data_type: "VARCHAR".to_string(),
                        nullable: false,
                        is_pk: false,
                    },
                ],
                constraints.clone(),
            )
            .await
            .unwrap();

        assert_eq!(table.name, "events");
        assert_eq!(columns.len(), 3);
        assert_eq!(catalog.list_tables(database.id).await.unwrap().len(), 1);
        assert_eq!(catalog.list_columns(table.id).await.unwrap().len(), 3);
        assert_eq!(
            catalog.get_constraints(table.id).await.unwrap(),
            constraints
        );

        catalog.delete_table(table.id).await.unwrap();
        assert!(catalog
            .get_table(database.id, "events")
            .await
            .unwrap()
            .is_none());
        catalog.delete_database("app").await.unwrap();
        assert!(catalog.get_database("app").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn mutates_table_columns_defaults_and_indexes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let catalog = SQLiteCatalogStore::open(&path).unwrap();
        catalog.init_catalog().await.unwrap();

        let database = catalog.create_database("app").await.unwrap();
        let (table, _) = catalog
            .create_table(
                "app",
                "events",
                vec![ColumnDefinition {
                    name: "id".to_string(),
                    data_type: "BIGINT".to_string(),
                    nullable: false,
                    is_pk: true,
                }],
                TableConstraintsInfo {
                    primary_key_columns: vec!["id".to_string()],
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        let status = catalog
            .add_column("app", "events", "status", "VARCHAR", true)
            .await
            .unwrap();
        assert_eq!(status.name, "status");

        catalog
            .set_column_default("app", "events", "status", Some("'open'".to_string()))
            .await
            .unwrap();
        catalog
            .set_column_nullability("app", "events", "status", false)
            .await
            .unwrap();
        catalog
            .rename_column("app", "events", "status", "state")
            .await
            .unwrap();

        let constraints = catalog.get_constraints(table.id).await.unwrap();
        assert_eq!(
            constraints.defaults.get("state"),
            Some(&"'open'".to_string())
        );
        assert!(
            !catalog
                .list_columns(table.id)
                .await
                .unwrap()
                .iter()
                .find(|column| column.name == "state")
                .unwrap()
                .nullable
        );

        let indexed = catalog
            .create_index("app", "events", "events_state_idx", "state")
            .await
            .unwrap();
        assert!(indexed.indexed);
        assert_eq!(indexed.index_name.as_deref(), Some("events_state_idx"));

        catalog.drop_column("app", "events", "state").await.unwrap();
        let columns = catalog.list_columns(table.id).await.unwrap();
        assert_eq!(columns.len(), 1);
        assert_eq!(columns[0].name, "id");
        assert_eq!(catalog.list_tables(database.id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn roundtrips_constraints() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let catalog = SQLiteCatalogStore::open(&path).unwrap();
        catalog.init_catalog().await.unwrap();
        let database = DatabaseInfo {
            id: 7,
            name: "app".to_string(),
        };
        let table = TableInfo {
            id: 42,
            name: "events".to_string(),
            db_id: database.id,
        };
        catalog.put_database(&database).await.unwrap();
        catalog.put_table(&table).await.unwrap();

        let constraints = TableConstraintsInfo {
            primary_key_columns: vec!["id".to_string()],
            defaults: [("status".to_string(), "'open'".to_string())].into(),
            unique_constraints: vec![crate::UniqueConstraintInfo {
                name: "events_code_key".to_string(),
                columns: vec!["code".to_string()],
            }],
            foreign_keys: vec![crate::ForeignKeyConstraintInfo {
                name: "events_account_fkey".to_string(),
                columns: vec!["account_id".to_string()],
                ref_db_name: None,
                ref_table_name: "accounts".to_string(),
                ref_columns: vec!["id".to_string()],
                on_delete: "NO ACTION".to_string(),
                on_update: "NO ACTION".to_string(),
            }],
            check_constraints: vec![crate::CheckConstraintInfo {
                name: "events_status_check".to_string(),
                expr_sql: "status IS NOT NULL".to_string(),
            }],
        };

        catalog.put_constraints(42, &constraints).await.unwrap();
        assert_eq!(catalog.get_constraints(42).await.unwrap(), constraints);
    }
}
