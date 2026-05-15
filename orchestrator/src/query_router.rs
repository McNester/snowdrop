use executor::column_store::{self, Bitmap, CmpOp, TableColumns};
use executor::index::{self, encode_index_value};
use executor::typed_value::EMPTY_TEXT_SENTINEL;
use executor::{
    bytes_to_row, bytes_to_row_ref, deserialize_columns, evaluate_predicate,
    evaluate_predicate_view, resolve_value, Row, StorageBackend, TypedValue, WriteEngine,
};
use futures::{future::BoxFuture, stream, FutureExt};
use pgwire::api::results::{DataRowEncoder, FieldFormat, FieldInfo, QueryResponse, Response, Tag};
use pgwire::error::{ErrorInfo, PgWireError, PgWireResult};
use pgwire::messages::data::DataRow;
use schema::{ColumnInfo, ColumnType, DatabaseInfo, Neo4jConnection, TableInfo};
use sqlparser::ast::{
    AlterTableOperation, ColumnOption, Expr, Function, FunctionArg, FunctionArgExpr,
    FunctionArguments, JoinConstraint, JoinOperator, ObjectName, OrderByExpr, SelectItem, SetExpr,
    Statement, Value, Values,
};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::warn;
use wal::WalFile;

/// In-memory schema cache. Eliminates Neo4j round-trips on the read path.
/// Loaded on startup, invalidated by DDL operations.
struct SchemaCache {
    databases: HashMap<String, DatabaseInfo>,
    tables: HashMap<(String, String), TableInfo>,
    columns: HashMap<(String, String), Vec<ColumnInfo>>,
}

impl SchemaCache {
    fn new() -> Self {
        Self {
            databases: HashMap::new(),
            tables: HashMap::new(),
            columns: HashMap::new(),
        }
    }

    fn invalidate_database(&mut self, db_name: &str) {
        self.databases.remove(db_name);
        self.tables.retain(|(d, _), _| d != db_name);
        self.columns.retain(|(d, _), _| d != db_name);
    }

    fn invalidate_table(&mut self, db_name: &str, table_name: &str) {
        self.tables
            .remove(&(db_name.to_string(), table_name.to_string()));
        self.columns
            .remove(&(db_name.to_string(), table_name.to_string()));
    }
}

#[derive(Debug, Clone)]
struct RelationColumn {
    lookup_name: String,
    output_name: String,
}

#[derive(Debug, Clone)]
struct RelationResult {
    rows: Vec<Row>,
    columns: Vec<RelationColumn>,
}

impl RelationResult {
    fn from_named_rows(rows: Vec<Row>, field_names: Vec<String>) -> Self {
        let columns = field_names
            .into_iter()
            .map(|name| {
                let lookup_name = name.to_lowercase();
                RelationColumn {
                    lookup_name,
                    output_name: name,
                }
            })
            .collect();
        Self { rows, columns }
    }

    fn field_names(&self) -> Vec<String> {
        self.columns
            .iter()
            .map(|col| col.output_name.clone())
            .collect()
    }

    fn schema_cols(&self) -> Vec<ColumnInfo> {
        self.columns
            .iter()
            .enumerate()
            .map(|(idx, col)| ColumnInfo {
                id: idx as u32 + 1,
                name: col.lookup_name.clone(),
                data_type: "TEXT".to_string(),
                nullable: true,
                is_pk: false,
                table_id: 0,
                indexed: false,
                index_name: None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, Default)]
struct RelationContext {
    ctes: HashMap<String, RelationResult>,
    outer_rows: Vec<Row>,
}

impl RelationContext {
    fn with_outer_row(&self, row: Row) -> Self {
        let mut next = self.clone();
        next.outer_rows.push(row);
        next
    }

    fn with_ctes(&self, ctes: HashMap<String, RelationResult>) -> Self {
        let mut next = self.clone();
        next.ctes = ctes;
        next
    }
}

pub struct QueryRouter {
    neo4j: Arc<RwLock<Neo4jConnection>>,
    storage: Arc<dyn StorageBackend>,
    wal: Arc<RwLock<WalFile>>,
    current_db: Arc<RwLock<String>>,
    schema_cache: Arc<RwLock<SchemaCache>>,
    /// In-memory columnar cache for SIMD-filtered full scans.
    /// Key: (db_id, table_id). Invalidated on mutations.
    column_cache: Arc<RwLock<HashMap<(u32, u32), TableColumns>>>,
}

impl QueryRouter {
    pub fn new(neo4j: Neo4jConnection, storage: Arc<dyn StorageBackend>, wal: WalFile) -> Self {
        Self {
            neo4j: Arc::new(RwLock::new(neo4j)),
            storage,
            wal: Arc::new(RwLock::new(wal)),
            current_db: Arc::new(RwLock::new("default".to_string())),
            schema_cache: Arc::new(RwLock::new(SchemaCache::new())),
            column_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set the current database context (called from pgwire startup params).
    pub async fn set_current_db(&self, db_name: &str) {
        let mut current = self.current_db.write().await;
        *current = db_name.to_string();
    }

    /// Load all existing schemas into cache on startup.
    pub async fn warm_schema_cache(&self) -> PgWireResult<()> {
        let neo4j = self.neo4j.read().await;
        let dbs = neo4j.show_databases().await.map_err(to_err)?;
        let mut cache = self.schema_cache.write().await;
        for db in &dbs {
            cache.databases.insert(db.name.clone(), db.clone());
            let tables = neo4j.show_tables(&db.name).await.map_err(to_err)?;
            for tbl in &tables {
                cache
                    .tables
                    .insert((db.name.clone(), tbl.name.clone()), tbl.clone());
                let cols = neo4j
                    .describe_table(&db.name, &tbl.name)
                    .await
                    .map_err(to_err)?;
                cache
                    .columns
                    .insert((db.name.clone(), tbl.name.clone()), cols);
            }
        }
        tracing::info!(
            databases = cache.databases.len(),
            tables = cache.tables.len(),
            "schema cache warmed"
        );
        Ok(())
    }

    /// Get database info from cache, falling back to Neo4j.
    async fn cached_get_database(&self, db_name: &str) -> PgWireResult<Option<DatabaseInfo>> {
        {
            let cache = self.schema_cache.read().await;
            if let Some(db) = cache.databases.get(db_name) {
                return Ok(Some(db.clone()));
            }
        }
        let neo4j = self.neo4j.read().await;
        let db = neo4j.get_database(db_name).await.map_err(to_err)?;
        if let Some(ref db_info) = db {
            let mut cache = self.schema_cache.write().await;
            cache.databases.insert(db_name.to_string(), db_info.clone());
        }
        Ok(db)
    }

    /// Get table info from cache, falling back to Neo4j.
    async fn cached_get_table(
        &self,
        db_name: &str,
        table_name: &str,
    ) -> PgWireResult<Option<TableInfo>> {
        let key = (db_name.to_string(), table_name.to_string());
        {
            let cache = self.schema_cache.read().await;
            if let Some(tbl) = cache.tables.get(&key) {
                return Ok(Some(tbl.clone()));
            }
        }
        let neo4j = self.neo4j.read().await;
        let tbl = neo4j.get_table(db_name, table_name).await.map_err(to_err)?;
        if let Some(ref tbl_info) = tbl {
            let mut cache = self.schema_cache.write().await;
            cache.tables.insert(key, tbl_info.clone());
        }
        Ok(tbl)
    }

    /// Get column schema from cache, falling back to Neo4j.
    async fn cached_describe_table(
        &self,
        db_name: &str,
        table_name: &str,
    ) -> PgWireResult<Vec<ColumnInfo>> {
        let key = (db_name.to_string(), table_name.to_string());
        {
            let cache = self.schema_cache.read().await;
            if let Some(cols) = cache.columns.get(&key) {
                return Ok(cols.clone());
            }
        }
        let neo4j = self.neo4j.read().await;
        let cols = neo4j
            .describe_table(db_name, table_name)
            .await
            .map_err(to_err)?;
        let mut cache = self.schema_cache.write().await;
        cache.columns.insert(key, cols.clone());
        Ok(cols)
    }

    pub async fn route<'a>(&self, query_str: &'a str) -> PgWireResult<Vec<Response<'a>>> {
        let trimmed = query_str.trim();

        if trimmed.is_empty() {
            return Ok(vec![Response::EmptyQuery]);
        }

        let upper = trimmed.to_uppercase();
        if upper.starts_with("SHOW DATABASES") {
            return self.handle_show_databases().await;
        }
        if upper.starts_with("SHOW TABLES") {
            return self.handle_show_tables().await;
        }
        if upper.starts_with("DESCRIBE ") || upper.starts_with("\\D ") {
            let table_name = trimmed
                .split_whitespace()
                .nth(1)
                .unwrap_or("")
                .trim_end_matches(';');
            return self.handle_describe(table_name).await;
        }
        if trimmed == "\\d" {
            return self.handle_show_tables().await;
        }
        if upper.starts_with("USE ") {
            let db_name = trimmed
                .split_whitespace()
                .nth(1)
                .unwrap_or("")
                .trim_end_matches(';');
            let mut current = self.current_db.write().await;
            *current = db_name.to_string();
            return Ok(vec![Response::Execution(Tag::new("USE"))]);
        }

        let stmts = Parser::parse_sql(&PostgreSqlDialect {}, trimmed).map_err(|e| {
            PgWireError::UserError(Box::new(ErrorInfo::new(
                "ERROR".to_string(),
                "42601".to_string(),
                format!("syntax error: {e}"),
            )))
        })?;

        if stmts.is_empty() {
            return Ok(vec![Response::EmptyQuery]);
        }

        let mut responses = Vec::new();
        for stmt in stmts {
            let resp = self.dispatch_statement(stmt).await?;
            responses.push(resp);
        }
        Ok(responses)
    }

    async fn dispatch_statement<'a>(&self, stmt: Statement) -> PgWireResult<Response<'a>> {
        let t_start = std::time::Instant::now();
        let stmt_type = classify_statement(&stmt);

        let result = self.dispatch_statement_inner(stmt).await;

        let elapsed = t_start.elapsed().as_secs_f64();
        metrics::counter!("snowdrop_queries_total", "type" => stmt_type).increment(1);
        metrics::histogram!("snowdrop_query_duration_seconds", "type" => stmt_type).record(elapsed);

        result
    }

    async fn dispatch_statement_inner<'a>(&self, stmt: Statement) -> PgWireResult<Response<'a>> {
        match stmt {
            Statement::CreateDatabase { db_name, .. } => {
                self.handle_create_database(&db_name.to_string()).await
            }
            Statement::Drop {
                object_type, names, ..
            } => match object_type {
                sqlparser::ast::ObjectType::Database => {
                    let name = names.first().map(|n| n.to_string()).unwrap_or_default();
                    self.handle_drop_database(&name).await
                }
                sqlparser::ast::ObjectType::Table => {
                    let name = names.first().map(|n| n.to_string()).unwrap_or_default();
                    self.handle_drop_table(&name).await
                }
                sqlparser::ast::ObjectType::Index => {
                    let name = names.first().map(|n| n.to_string()).unwrap_or_default();
                    self.handle_drop_index(&name).await
                }
                _ => Err(user_error(format!(
                    "DROP {:?} is not supported",
                    object_type
                ))),
            },
            Statement::CreateTable(ct) => self.handle_create_table(&ct.name, &ct.columns).await,
            Statement::AlterTable {
                name, operations, ..
            } => {
                for op in operations {
                    self.handle_alter_table(&name, op).await?;
                }
                Ok(Response::Execution(Tag::new("ALTER TABLE")))
            }
            Statement::Insert(insert) => self.handle_insert(insert).await,
            Statement::Update {
                table,
                assignments,
                selection,
                ..
            } => {
                self.handle_update(&table.relation, &assignments, selection.as_ref())
                    .await
            }
            Statement::Delete(delete) => {
                self.handle_delete(&delete.from, delete.selection.as_ref())
                    .await
            }
            Statement::Query(q) => {
                let q_str = q.to_string().to_uppercase();
                if q_str.contains("INFORMATION_SCHEMA.TABLES") {
                    return self.handle_info_schema_tables().await;
                }
                if q_str.contains("INFORMATION_SCHEMA.COLUMNS") {
                    return self.handle_info_schema_columns().await;
                }
                self.handle_select(*q).await
            }
            Statement::CreateIndex(ci) => self.handle_create_index(ci).await,
            _ => {
                warn!(stmt = ?stmt, "unhandled statement type");
                Ok(Response::EmptyQuery)
            }
        }
    }

    fn resolve_table_name(&self, name: &ObjectName, current_db: &str) -> (String, String) {
        let parts: Vec<String> = name.0.iter().map(|i| i.value.clone()).collect();
        match parts.len() {
            1 => (current_db.to_string(), parts[0].clone()),
            2 => (parts[0].clone(), parts[1].clone()),
            _ => (current_db.to_string(), name.to_string()),
        }
    }

    // ── DDL handlers (unchanged from M2) ──

    async fn handle_create_database<'a>(&self, name: &str) -> PgWireResult<Response<'a>> {
        let neo4j = self.neo4j.read().await;
        neo4j.create_database(name).await.map_err(to_err)?;
        // Warm cache for the new database
        if let Ok(Some(db)) = neo4j.get_database(name).await {
            let mut cache = self.schema_cache.write().await;
            cache.databases.insert(name.to_string(), db);
        }
        Ok(Response::Execution(Tag::new("CREATE DATABASE")))
    }

    async fn handle_drop_database<'a>(&self, name: &str) -> PgWireResult<Response<'a>> {
        let neo4j = self.neo4j.read().await;
        neo4j.drop_database(name).await.map_err(to_err)?;
        let mut cache = self.schema_cache.write().await;
        cache.invalidate_database(name);
        Ok(Response::Execution(Tag::new("DROP DATABASE")))
    }

    async fn handle_create_table<'a>(
        &self,
        name: &ObjectName,
        columns: &[sqlparser::ast::ColumnDef],
    ) -> PgWireResult<Response<'a>> {
        let current_db = self.current_db.read().await.clone();
        let (db_name, table_name) = self.resolve_table_name(name, &current_db);

        let mut col_defs = Vec::new();
        for col in columns {
            let col_type =
                ColumnType::from_sql_type(&col.data_type).map_err(|e| user_error(e.to_string()))?;

            let mut nullable = true;
            let mut is_pk = false;

            for opt in &col.options {
                match &opt.option {
                    ColumnOption::Unique { is_primary, .. } if *is_primary => {
                        is_pk = true;
                        nullable = false;
                    }
                    ColumnOption::NotNull => nullable = false,
                    ColumnOption::Null => nullable = true,
                    _ => {}
                }
            }

            col_defs.push((col.name.value.clone(), col_type, nullable, is_pk));
        }

        let neo4j = self.neo4j.read().await;
        neo4j
            .create_table(&db_name, &table_name, col_defs)
            .await
            .map_err(to_err)?;

        // Warm cache for the new table
        if let Ok(Some(tbl)) = neo4j.get_table(&db_name, &table_name).await {
            if let Ok(cols) = neo4j.describe_table(&db_name, &table_name).await {
                let mut cache = self.schema_cache.write().await;
                cache
                    .tables
                    .insert((db_name.clone(), table_name.clone()), tbl);
                cache.columns.insert((db_name, table_name), cols);
            }
        }

        Ok(Response::Execution(Tag::new("CREATE TABLE")))
    }

    async fn handle_drop_table<'a>(&self, name: &str) -> PgWireResult<Response<'a>> {
        let current_db = self.current_db.read().await.clone();
        let parts: Vec<&str> = name.split('.').collect();
        let (db_name, table_name) = match parts.len() {
            1 => (current_db.as_str(), parts[0]),
            2 => (parts[0], parts[1]),
            _ => (current_db.as_str(), name),
        };

        let neo4j = self.neo4j.read().await;
        neo4j
            .drop_table(db_name, table_name)
            .await
            .map_err(to_err)?;
        let mut cache = self.schema_cache.write().await;
        cache.invalidate_table(db_name, table_name);
        Ok(Response::Execution(Tag::new("DROP TABLE")))
    }

    async fn handle_alter_table<'a>(
        &self,
        name: &ObjectName,
        op: AlterTableOperation,
    ) -> PgWireResult<Response<'a>> {
        let current_db = self.current_db.read().await.clone();
        let (db_name, table_name) = self.resolve_table_name(name, &current_db);

        match op {
            AlterTableOperation::AddColumn { column_def, .. } => {
                let col_type = ColumnType::from_sql_type(&column_def.data_type)
                    .map_err(|e| user_error(e.to_string()))?;

                let nullable = !column_def
                    .options
                    .iter()
                    .any(|o| matches!(o.option, ColumnOption::NotNull));

                let neo4j = self.neo4j.read().await;
                neo4j
                    .add_column(
                        &db_name,
                        &table_name,
                        &column_def.name.value,
                        &col_type,
                        nullable,
                    )
                    .await
                    .map_err(to_err)?;

                let mut cache = self.schema_cache.write().await;
                cache.invalidate_table(&db_name, &table_name);

                Ok(Response::Execution(Tag::new("ALTER TABLE")))
            }
            other => Err(user_error(format!(
                "ALTER TABLE {:?} is not supported",
                other
            ))),
        }
    }

    // ── Index DDL handlers (M7) ──

    async fn handle_create_index<'a>(
        &self,
        ci: sqlparser::ast::CreateIndex,
    ) -> PgWireResult<Response<'a>> {
        let current_db = self.current_db.read().await.clone();

        let table_name_obj = ci.table_name;
        let (db_name, table_name) = self.resolve_table_name(&table_name_obj, &current_db);

        let index_name = ci
            .name
            .map(|n| n.to_string())
            .unwrap_or_else(|| "unnamed_index".to_string());

        if ci.columns.is_empty() {
            return Err(user_error(
                "CREATE INDEX requires at least one column".to_string(),
            ));
        }
        let col_name = ci.columns[0].expr.to_string();

        // Mark column as indexed in Neo4j
        let neo4j = self.neo4j.read().await;
        let col = neo4j
            .create_index(&db_name, &table_name, &index_name, &col_name)
            .await
            .map_err(to_err)?;

        // Get table info for backfill
        let table = self
            .cached_get_table(&db_name, &table_name)
            .await?
            .ok_or_else(|| user_error(format!("table '{}' does not exist", table_name)))?;
        let db = self
            .cached_get_database(&db_name)
            .await?
            .ok_or_else(|| user_error(format!("database '{}' does not exist", db_name)))?;
        let schema_cols = self.cached_describe_table(&db_name, &table_name).await?;

        // Backfill index entries for existing rows
        let count = index::backfill_index(
            self.storage.as_ref(),
            db.id,
            table.id,
            col.id,
            &col.data_type,
            &schema_cols,
        )
        .await
        .map_err(to_err)?;

        tracing::info!(index_name, col_name, count, "index created and backfilled");

        // Invalidate caches
        let mut cache = self.schema_cache.write().await;
        cache.invalidate_table(&db_name, &table_name);
        drop(cache);
        self.invalidate_column_cache(db.id, table.id).await;

        Ok(Response::Execution(Tag::new("CREATE INDEX")))
    }

    async fn handle_drop_index<'a>(&self, index_name: &str) -> PgWireResult<Response<'a>> {
        let current_db = self.current_db.read().await.clone();

        // Need to find which table this index belongs to by searching all tables
        let neo4j = self.neo4j.read().await;
        let tables = neo4j.show_tables(&current_db).await.map_err(to_err)?;

        for table in &tables {
            let indexes = neo4j
                .list_indexes(&current_db, &table.name)
                .await
                .map_err(to_err)?;
            if let Some((_, col_name)) = indexes.iter().find(|(name, _)| name == index_name) {
                let col = neo4j
                    .drop_index(&current_db, &table.name, index_name)
                    .await
                    .map_err(to_err)?;

                let db = self
                    .cached_get_database(&current_db)
                    .await?
                    .ok_or_else(|| user_error("database not found".to_string()))?;

                index::drop_index_entries(self.storage.as_ref(), db.id, table.id, col.id)
                    .await
                    .map_err(to_err)?;

                tracing::info!(index_name, col_name, "index dropped");

                let mut cache = self.schema_cache.write().await;
                cache.invalidate_table(&current_db, &table.name);
                drop(cache);
                self.invalidate_column_cache(db.id, table.id).await;

                return Ok(Response::Execution(Tag::new("DROP INDEX")));
            }
        }

        Err(user_error(format!("index '{}' does not exist", index_name)))
    }

    // ── DML handlers (M3) ──

    async fn handle_insert<'a>(
        &self,
        insert: sqlparser::ast::Insert,
    ) -> PgWireResult<Response<'a>> {
        let current_db = self.current_db.read().await.clone();
        let table_name_obj = insert.table_name.clone();
        let (db_name, table_name) = self.resolve_table_name(&table_name_obj, &current_db);

        let mut total_inserted = 0u64;
        let neo4j = self.neo4j.read().await;
        let target_schema = self.cached_describe_table(&db_name, &table_name).await?;
        let column_names: Vec<String> = if insert.columns.is_empty() {
            target_schema.iter().map(|col| col.name.clone()).collect()
        } else {
            insert.columns.iter().map(|c| c.value.clone()).collect()
        };

        // Extract values from the INSERT source
        match insert.source.as_ref() {
            Some(query) => match query.body.as_ref() {
                SetExpr::Values(Values { rows, .. }) => {
                    for row in rows {
                        let values: Vec<Vec<u8>> = row
                            .iter()
                            .map(|expr| expr_to_bytes(expr))
                            .collect::<Result<_, _>>()?;

                        let mut wal = self.wal.write().await;
                        let mut engine = WriteEngine::new(self.storage.as_ref(), &neo4j, &mut wal);
                        let n = engine
                            .insert(&db_name, &table_name, &column_names, &values)
                            .await
                            .map_err(to_err)?;
                        total_inserted += n;
                    }
                }
                SetExpr::Select(_) | SetExpr::SetOperation { .. } | SetExpr::Query(_) => {
                    let relation = self.execute_query_to_relation(query.as_ref()).await?;
                    let source_columns: Vec<String> = relation
                        .field_names()
                        .into_iter()
                        .map(|name| name.to_lowercase())
                        .collect();
                    if source_columns.len() != column_names.len() {
                        return Err(user_error(format!(
                            "INSERT ... SELECT source/target column count mismatch (target={}, source={})",
                            column_names.len(),
                            source_columns.len()
                        )));
                    }
                    let target_columns =
                        resolve_insert_target_columns(&target_schema, &column_names)?;
                    let rows = relation.rows;
                    if rows.is_empty() {
                        return Ok(Response::Execution(Tag::new("INSERT").with_rows(0)));
                    }

                    let materialized_rows: Vec<Vec<TypedValue>> = rows
                        .iter()
                        .map(|row| {
                            source_columns
                                .iter()
                                .map(|col| lookup_row_value(row, col))
                                .collect()
                        })
                        .collect();

                    for values in &materialized_rows {
                        validate_insert_select_row_compatibility(values, &target_columns)?;
                    }

                    for values in &materialized_rows {
                        let bytes: Vec<Vec<u8>> = values.iter().map(typed_value_to_bytes).collect();

                        let mut wal = self.wal.write().await;
                        let mut engine = WriteEngine::new(self.storage.as_ref(), &neo4j, &mut wal);
                        let n = engine
                            .insert(&db_name, &table_name, &column_names, &bytes)
                            .await
                            .map_err(to_err)?;
                        total_inserted += n;
                    }
                }
                _ => {
                    return Err(user_error(
                        "INSERT supports VALUES or SELECT in the current Phase 3 subset"
                            .to_string(),
                    ))
                }
            },
            None => return Err(user_error("INSERT must have values".to_string())),
        }

        // Invalidate column cache for this table
        if let (Some(db_info), Some(tbl_info)) = (
            self.cached_get_database(&db_name).await.ok().flatten(),
            self.cached_get_table(&db_name, &table_name)
                .await
                .ok()
                .flatten(),
        ) {
            self.invalidate_column_cache(db_info.id, tbl_info.id).await;
        }

        Ok(Response::Execution(
            Tag::new("INSERT").with_rows(total_inserted as usize),
        ))
    }

    async fn handle_update<'a>(
        &self,
        table: &sqlparser::ast::TableFactor,
        assignments: &[sqlparser::ast::Assignment],
        selection: Option<&Expr>,
    ) -> PgWireResult<Response<'a>> {
        let current_db = self.current_db.read().await.clone();

        let table_name_obj = match table {
            sqlparser::ast::TableFactor::Table { name, .. } => name.clone(),
            _ => return Err(user_error("UPDATE requires a table name".to_string())),
        };
        let (db_name, table_name) = self.resolve_table_name(&table_name_obj, &current_db);
        // Parse SET pairs
        let mut set_pairs: Vec<(String, Vec<u8>)> = Vec::new();
        for assign in assignments {
            let col_name = match &assign.target {
                sqlparser::ast::AssignmentTarget::ColumnName(name) => {
                    name.0.last().map(|i| i.value.clone()).unwrap_or_default()
                }
                sqlparser::ast::AssignmentTarget::Tuple(_) => {
                    return Err(user_error("tuple assignment not supported".to_string()));
                }
            };
            let val = expr_to_bytes(&assign.value)?;
            set_pairs.push((col_name, val));
        }

        // Parse WHERE clause (simple col = val for M3)
        let (where_col, where_val) = parse_simple_where(selection)?;

        let neo4j = self.neo4j.read().await;
        let mut wal = self.wal.write().await;
        let mut engine = WriteEngine::new(self.storage.as_ref(), &neo4j, &mut wal);
        let n = engine
            .update(&db_name, &table_name, &set_pairs, &where_col, &where_val)
            .await
            .map_err(to_err)?;

        if let (Some(db_info), Some(tbl_info)) = (
            self.cached_get_database(&db_name).await.ok().flatten(),
            self.cached_get_table(&db_name, &table_name)
                .await
                .ok()
                .flatten(),
        ) {
            self.invalidate_column_cache(db_info.id, tbl_info.id).await;
        }

        Ok(Response::Execution(
            Tag::new("UPDATE").with_rows(n as usize),
        ))
    }

    async fn handle_delete<'a>(
        &self,
        from: &sqlparser::ast::FromTable,
        selection: Option<&Expr>,
    ) -> PgWireResult<Response<'a>> {
        let current_db = self.current_db.read().await.clone();

        let table_with_joins = match from {
            sqlparser::ast::FromTable::WithFromKeyword(tables) => tables
                .first()
                .ok_or_else(|| user_error("DELETE requires FROM".to_string()))?,
            sqlparser::ast::FromTable::WithoutKeyword(tables) => tables
                .first()
                .ok_or_else(|| user_error("DELETE requires FROM".to_string()))?,
        };

        let table_name_obj = match &table_with_joins.relation {
            sqlparser::ast::TableFactor::Table { name, .. } => name.clone(),
            _ => return Err(user_error("DELETE requires a table name".to_string())),
        };
        let (db_name, table_name) = self.resolve_table_name(&table_name_obj, &current_db);

        let (where_col, where_val) = parse_simple_where(selection)?;

        let neo4j = self.neo4j.read().await;
        let mut wal = self.wal.write().await;
        let mut engine = WriteEngine::new(self.storage.as_ref(), &neo4j, &mut wal);
        let n = engine
            .delete(&db_name, &table_name, &where_col, &where_val)
            .await
            .map_err(to_err)?;

        if let (Some(db_info), Some(tbl_info)) = (
            self.cached_get_database(&db_name).await.ok().flatten(),
            self.cached_get_table(&db_name, &table_name)
                .await
                .ok()
                .flatten(),
        ) {
            self.invalidate_column_cache(db_info.id, tbl_info.id).await;
        }

        Ok(Response::Execution(
            Tag::new("DELETE").with_rows(n as usize),
        ))
    }

    // ── SELECT handler (M4) ──

    async fn handle_select<'a>(&self, query: sqlparser::ast::Query) -> PgWireResult<Response<'a>> {
        if !query_can_use_optimized_relation_fast_path(&query) {
            let relation = self.execute_query_to_relation(&query).await?;
            return self.relation_to_response(&relation);
        }

        let t_total = std::time::Instant::now();
        let select = match query.body.as_ref() {
            SetExpr::Select(sel) => sel.clone(),
            _ => {
                return Err(user_error(
                    "compound queries like UNION are not supported yet".to_string(),
                ))
            }
        };

        validate_select_expressions(&select, &query)?;
        validate_query_surface_boundaries(&select, &query)?;

        if select.from.is_empty() {
            return Err(user_error("SELECT requires FROM clause".to_string()));
        }

        if !select.from[0].joins.is_empty() {
            return self.handle_join_select(*select, query).await;
        }

        let table_factor = &select.from[0].relation;
        let table_name_obj = match table_factor {
            sqlparser::ast::TableFactor::Table { name, .. } => name.clone(),
            _ => return Err(user_error("SELECT requires a table name".to_string())),
        };

        let current_db = self.current_db.read().await.clone();
        let (db_name, table_name) = self.resolve_table_name(&table_name_obj, &current_db);

        let t_schema = std::time::Instant::now();
        let schema_cols = self.cached_describe_table(&db_name, &table_name).await?;
        let db = self
            .cached_get_database(&db_name)
            .await?
            .ok_or_else(|| user_error(format!("database '{}' does not exist", db_name)))?;
        let table = self
            .cached_get_table(&db_name, &table_name)
            .await?
            .ok_or_else(|| user_error(format!("table '{}' does not exist", table_name)))?;
        let schema_ms = t_schema.elapsed().as_secs_f64() * 1000.0;

        let agg_exprs = extract_aggregates(&select.projection);
        let is_aggregate = !agg_exprs.is_empty();
        let has_expr_proj = has_expression_projection(&select.projection);

        let projected_cols = if is_aggregate || has_expr_proj {
            schema_cols.clone()
        } else {
            resolve_projection(&select.projection, &schema_cols)?
        };

        if is_aggregate && select.selection.is_none() && is_count_star_only(&select.projection) {
            let count = self.count_rows_fast(db.id, table.id).await?;
            return self.make_count_star_response(count);
        }

        let needed_col_ids = compute_needed_col_ids(
            select.selection.as_ref(),
            &select.projection,
            query.order_by.as_ref(),
            &select.group_by,
            &schema_cols,
        );

        let (query_offset, query_limit) = query_offset_limit(&query);
        let agg_offset = query_offset;
        let agg_limit = query_limit;

        let t_fetch = std::time::Instant::now();
        if let Some(pred) = select.selection.as_ref() {
            if let Some(rows) = self
                .try_index_scan(db.id, table.id, &schema_cols, &needed_col_ids, pred)
                .await?
            {
                let fetch_ms = t_fetch.elapsed().as_secs_f64() * 1000.0;
                metrics::counter!("snowdrop_query_plans_total", "kind" => "scan_type", "value" => "index").increment(1);

                tracing::info!(
                    schema_ms = format!("{:.1}", schema_ms),
                    fetch_ms = format!("{:.1}", fetch_ms),
                    scan_type = "index",
                    needed_cols = needed_col_ids.len(),
                    filtered_rows = rows.len(),
                    total_ms = format!("{:.1}", t_total.elapsed().as_secs_f64() * 1000.0),
                    "SELECT perf breakdown"
                );

                if is_aggregate {
                    return self.handle_aggregate_result(
                        &select.projection,
                        &rows,
                        &schema_cols,
                        &select.group_by,
                        select.having.as_ref(),
                        query.order_by.as_ref(),
                        agg_limit,
                        agg_offset,
                    );
                }

                let mut result_rows = rows;
                if let Some(ref order_by) = query.order_by {
                    sort_rows(&mut result_rows, &order_by.exprs, &schema_cols)?;
                }

                let paged = paginate_rows(result_rows, agg_offset, query_limit);

                return if has_expr_proj {
                    let proj_exprs = resolve_projection_exprs(&select.projection, &schema_cols)?;
                    self.expr_rows_to_response(&paged, &proj_exprs, &schema_cols)
                } else {
                    self.rows_to_response(&paged, &projected_cols)
                };
            }
        }

        if is_aggregate {
            if let Some(resp) = self
                .try_fast_columnar_aggregate_response(
                    db.id,
                    table.id,
                    &select.projection,
                    select.selection.as_ref(),
                    &schema_cols,
                    &select.group_by,
                    query.order_by.as_ref(),
                    agg_limit,
                    agg_offset,
                )
                .await?
            {
                let fetch_ms = t_fetch.elapsed().as_secs_f64() * 1000.0;
                metrics::counter!("snowdrop_query_plans_total", "kind" => "scan_type", "value" => "columnar_fast_agg").increment(1);

                tracing::info!(
                    schema_ms = format!("{:.1}", schema_ms),
                    fetch_ms = format!("{:.1}", fetch_ms),
                    scan_type = "columnar_fast_agg",
                    needed_cols = needed_col_ids.len(),
                    total_ms = format!("{:.1}", t_total.elapsed().as_secs_f64() * 1000.0),
                    "SELECT perf breakdown"
                );

                return Ok(resp);
            }
        }

        let filtered = if let Some(pred) = select.selection.as_ref() {
            self.fetch_columnar(db.id, table.id, &schema_cols, &needed_col_ids, Some(pred))
                .await?
        } else {
            self.fetch_columnar(db.id, table.id, &schema_cols, &needed_col_ids, None)
                .await?
        };
        let fetch_ms = t_fetch.elapsed().as_secs_f64() * 1000.0;
        let filtered_count = filtered.len();
        metrics::counter!("snowdrop_query_plans_total", "kind" => "scan_type", "value" => "columnar").increment(1);

        tracing::info!(
            schema_ms = format!("{:.1}", schema_ms),
            fetch_ms = format!("{:.1}", fetch_ms),
            scan_type = "columnar",
            needed_cols = needed_col_ids.len(),
            filtered_rows = filtered_count,
            total_ms = format!("{:.1}", t_total.elapsed().as_secs_f64() * 1000.0),
            "SELECT perf breakdown"
        );

        if is_aggregate {
            return self.handle_aggregate_result(
                &select.projection,
                &filtered,
                &schema_cols,
                &select.group_by,
                select.having.as_ref(),
                query.order_by.as_ref(),
                agg_limit,
                agg_offset,
            );
        }

        let mut result_rows = filtered;
        if let Some(ref order_by) = query.order_by {
            sort_rows(&mut result_rows, &order_by.exprs, &schema_cols)?;
        }

        let paged = paginate_rows(result_rows, query_offset, query_limit);

        if has_expr_proj {
            let proj_exprs = resolve_projection_exprs(&select.projection, &schema_cols)?;
            self.expr_rows_to_response(&paged, &proj_exprs, &schema_cols)
        } else {
            self.rows_to_response(&paged, &projected_cols)
        }
    }

    // ── JOIN handler (M5) ──

    async fn handle_join_select<'a>(
        &self,
        select: sqlparser::ast::Select,
        query: sqlparser::ast::Query,
    ) -> PgWireResult<Response<'a>> {
        let current_db = self.current_db.read().await.clone();
        let include_all_columns = projection_needs_all_columns(&select.projection);
        let query_refs = collect_join_query_refs(&select, &query);
        let mut remaining_conjuncts = Vec::new();
        if let Some(selection) = select.selection.as_ref() {
            collect_conjuncts(selection, &mut remaining_conjuncts);
        }
        metrics::counter!("snowdrop_query_plans_total", "kind" => "join_strategy", "value" => "hash").increment(1);

        let (left_name, left_alias) = extract_table_info(&select.from[0].relation)?;
        let (left_db, left_tbl) = self.resolve_table_name(&left_name, &current_db);
        let left_schema = self.cached_describe_table(&left_db, &left_tbl).await?;
        let left_db_info = self
            .cached_get_database(&left_db)
            .await?
            .ok_or_else(|| user_error(format!("database '{}' does not exist", left_db)))?;
        let left_tbl_info = self
            .cached_get_table(&left_db, &left_tbl)
            .await?
            .ok_or_else(|| user_error(format!("table '{}' does not exist", left_tbl)))?;

        let left_prefix = left_alias.as_deref().unwrap_or(&left_tbl).to_lowercase();
        let left_pushdown =
            take_pushdown_predicate(&mut remaining_conjuncts, &left_prefix, &left_schema);
        let left_needed_col_ids = compute_join_needed_col_ids(
            &left_schema,
            &left_prefix,
            &query_refs,
            include_all_columns,
        );
        let left_rows = self
            .fetch_filtered_rows(
                left_db_info.id,
                left_tbl_info.id,
                &left_schema,
                &left_needed_col_ids,
                left_pushdown.as_ref(),
            )
            .await?;
        let mut joined_rows: Vec<Row> = left_rows
            .into_iter()
            .map(|r| prefix_row(r, &left_prefix))
            .collect();
        let mut combined_schema = left_schema
            .iter()
            .map(|c| ColumnInfo {
                name: format!("{}.{}", left_prefix, c.name.to_lowercase()),
                ..c.clone()
            })
            .collect::<Vec<_>>();

        for join in &select.from[0].joins {
            let (right_name, right_alias) = extract_table_info(&join.relation)?;
            let (right_db, right_tbl) = self.resolve_table_name(&right_name, &current_db);
            let right_schema = self.cached_describe_table(&right_db, &right_tbl).await?;
            let right_db_info = self
                .cached_get_database(&right_db)
                .await?
                .ok_or_else(|| user_error(format!("database '{}' does not exist", right_db)))?;
            let right_tbl_info = self
                .cached_get_table(&right_db, &right_tbl)
                .await?
                .ok_or_else(|| user_error(format!("table '{}' does not exist", right_tbl)))?;

            let (is_left_join, on_expr) = match &join.join_operator {
                JoinOperator::Inner(JoinConstraint::On(expr)) => (false, expr.clone()),
                JoinOperator::LeftOuter(JoinConstraint::On(expr)) => (true, expr.clone()),
                _ => {
                    return Err(user_error(
                        "only INNER JOIN ... ON and LEFT JOIN ... ON are supported".to_string(),
                    ))
                }
            };

            let join_conds = extract_join_conditions(&on_expr)?;
            let right_prefix = right_alias.as_deref().unwrap_or(&right_tbl).to_lowercase();
            let right_pushdown =
                take_pushdown_predicate(&mut remaining_conjuncts, &right_prefix, &right_schema);
            let right_needed_col_ids = compute_join_needed_col_ids(
                &right_schema,
                &right_prefix,
                &query_refs,
                include_all_columns,
            );
            let right_rows = self
                .fetch_filtered_rows(
                    right_db_info.id,
                    right_tbl_info.id,
                    &right_schema,
                    &right_needed_col_ids,
                    right_pushdown.as_ref(),
                )
                .await?;
            let right_prefixed: Vec<Row> = right_rows
                .into_iter()
                .map(|r| prefix_row(r, &right_prefix))
                .collect();

            record_cardinality("join_left_rows", joined_rows.len());
            record_cardinality("join_right_rows", right_prefixed.len());

            let oriented_conds: Vec<(String, String)> = join_conds
                .iter()
                .map(|(a, b)| {
                    let a_is_right = a.starts_with(&format!("{}.", right_prefix));
                    if a_is_right {
                        (b.clone(), a.clone())
                    } else {
                        (a.clone(), b.clone())
                    }
                })
                .collect();
            let right_join_keys: Vec<String> = oriented_conds
                .iter()
                .map(|(_, rc)| {
                    if rc.contains('.') {
                        rc.clone()
                    } else {
                        format!("{}.{}", right_prefix, rc)
                    }
                })
                .collect();
            let left_join_keys: Vec<String> = oriented_conds
                .iter()
                .map(|(lc, _)| {
                    if lc.contains('.') {
                        lc.clone()
                    } else {
                        combined_schema
                            .iter()
                            .find(|c| c.name.ends_with(&format!(".{}", lc)))
                            .map(|c| c.name.clone())
                            .unwrap_or_else(|| lc.clone())
                    }
                })
                .collect();

            let t_build = std::time::Instant::now();
            let mut right_hash: HashMap<Vec<String>, Vec<&Row>> = HashMap::new();
            for row in &right_prefixed {
                let key: Vec<String> = right_join_keys
                    .iter()
                    .map(|k| row.get(k).map(|v| v.to_string()).unwrap_or_default())
                    .collect();
                right_hash.entry(key).or_default().push(row);
            }
            record_stage_duration("join_build", t_build.elapsed().as_secs_f64());

            let t_probe = std::time::Instant::now();
            let mut new_rows: Vec<Row> = Vec::new();
            for left_row in &joined_rows {
                let left_key: Vec<String> = left_join_keys
                    .iter()
                    .map(|k| left_row.get(k).map(|v| v.to_string()).unwrap_or_default())
                    .collect();

                if let Some(matching_rights) = right_hash.get(&left_key) {
                    for right_row in matching_rights {
                        let mut combined = left_row.clone();
                        combined.extend((*right_row).clone());
                        new_rows.push(combined);
                    }
                } else if is_left_join {
                    let mut combined = left_row.clone();
                    for col in &right_schema {
                        let key = format!("{}.{}", right_prefix, col.name.to_lowercase());
                        combined.insert(key, TypedValue::Null);
                    }
                    new_rows.push(combined);
                }
            }
            record_stage_duration("join_probe", t_probe.elapsed().as_secs_f64());
            record_cardinality("join_output_rows", new_rows.len());

            joined_rows = new_rows;
            combined_schema.extend(right_schema.iter().map(|c| ColumnInfo {
                name: format!("{}.{}", right_prefix, c.name.to_lowercase()),
                ..c.clone()
            }));
        }

        if let Some(remaining_selection) = combine_conjuncts(remaining_conjuncts) {
            joined_rows
                .retain(|row| evaluate_predicate(&remaining_selection, row, &combined_schema));
        }

        let agg_exprs = extract_aggregates(&select.projection);
        if !agg_exprs.is_empty() {
            let (agg_offset, agg_limit) = query_offset_limit(&query);
            return self.handle_aggregate_result(
                &select.projection,
                &joined_rows,
                &combined_schema,
                &select.group_by,
                select.having.as_ref(),
                query.order_by.as_ref(),
                agg_limit,
                agg_offset,
            );
        }

        let projected = resolve_join_projection(&select.projection, &combined_schema)?;
        if let Some(ref order_by) = query.order_by {
            sort_rows(&mut joined_rows, &order_by.exprs, &combined_schema)?;
        }

        let (offset, limit) = query_offset_limit(&query);
        let paged = paginate_rows(joined_rows, offset, limit);

        let fields = Arc::new(
            projected
                .iter()
                .map(|(name, _)| text_field(name))
                .collect::<Vec<_>>(),
        );
        let t_encode = std::time::Instant::now();
        let mut data_rows: Vec<DataRow> = Vec::new();
        for row in &paged {
            let mut encoder = DataRowEncoder::new(fields.clone());
            for (_name, key) in &projected {
                let val = row.get(key);
                match val {
                    Some(TypedValue::Null) | None => encoder.encode_field(&None::<&str>)?,
                    Some(tv) => encoder.encode_field(&tv.to_string())?,
                }
            }
            data_rows.push(encoder.finish()?);
        }
        record_stage_duration("pgwire_encode", t_encode.elapsed().as_secs_f64());

        Ok(Response::Query(query_response(fields, data_rows)))
    }

    /// Fast row count: SCAN keys and count unique row_ids without MGET.
    async fn count_rows_fast(&self, db_id: u32, table_id: u32) -> PgWireResult<i64> {
        let prefix =
            keys::table_scan_prefix(db_id, table_id).map_err(|e| user_error(e.to_string()))?;
        let all_kv = self.storage.scan_prefix(&prefix).await.map_err(to_err)?;
        Ok(all_kv.len() as i64)
    }

    async fn ensure_column_cache(
        &self,
        db_id: u32,
        table_id: u32,
        schema_cols: &[ColumnInfo],
    ) -> PgWireResult<(bool, f64)> {
        let t_cache = std::time::Instant::now();
        {
            let cache = self.column_cache.read().await;
            if cache.contains_key(&(db_id, table_id)) {
                return Ok((true, t_cache.elapsed().as_secs_f64() * 1000.0));
            }
        }

        let prefix =
            keys::table_scan_prefix(db_id, table_id).map_err(|e| user_error(e.to_string()))?;
        let all_kv = self.storage.scan_prefix(&prefix).await.map_err(to_err)?;
        let table_cols = TableColumns::build(&all_kv, schema_cols);
        let mut cache = self.column_cache.write().await;
        cache.insert((db_id, table_id), table_cols);

        let cache_ms = t_cache.elapsed().as_secs_f64() * 1000.0;
        record_stage_duration("columnar_cache_build", cache_ms / 1000.0);
        Ok((false, cache_ms))
    }

    /// Build a simple COUNT(*) = N response.
    fn make_count_star_response<'a>(&self, count: i64) -> PgWireResult<Response<'a>> {
        let fields = Arc::new(vec![text_field("COUNT(*)")]);
        let mut encoder = DataRowEncoder::new(fields.clone());
        encoder.encode_field(&count.to_string())?;
        let data_rows = vec![encoder.finish()?];
        Ok(Response::Query(query_response(fields, data_rows)))
    }

    /// Fused scan + decode + filter. Only decodes needed columns, applies predicate inline.
    async fn fetch_filtered_rows(
        &self,
        db_id: u32,
        table_id: u32,
        schema_cols: &[ColumnInfo],
        needed_col_ids: &HashSet<u32>,
        predicate: Option<&Expr>,
    ) -> PgWireResult<Vec<Row>> {
        let t_scan = std::time::Instant::now();
        let prefix =
            keys::table_scan_prefix(db_id, table_id).map_err(|e| user_error(e.to_string()))?;
        let all_kv = self.storage.scan_prefix(&prefix).await.map_err(to_err)?;
        let scan_ms = t_scan.elapsed().as_secs_f64() * 1000.0;

        if all_kv.is_empty() {
            return Ok(Vec::new());
        }

        let needed_schema: Vec<&ColumnInfo> = schema_cols
            .iter()
            .filter(|c| needed_col_ids.contains(&c.id))
            .collect();

        let t_assemble = std::time::Instant::now();
        let mut rows = Vec::with_capacity(all_kv.len());
        let mut scanned = 0usize;
        for (_key, row_data) in &all_kv {
            scanned += 1;
            if let Some(view) = executor::RowView::new(row_data) {
                // Evaluate predicate directly on RowView (no HashMap, no upfront parsing)
                if let Some(pred) = predicate {
                    if !evaluate_predicate_view(pred, &view, schema_cols) {
                        continue;
                    }
                }
                // Only build Row for rows that pass the predicate
                let col_values: Vec<(&ColumnInfo, Option<&[u8]>)> = needed_schema
                    .iter()
                    .map(|sc| (*sc, view.find_col(sc.id)))
                    .collect();
                rows.push(bytes_to_row_ref(&col_values));
            } else {
                // Legacy bincode fallback
                let columns = deserialize_columns(row_data);
                let col_values: Vec<(&ColumnInfo, Option<Vec<u8>>)> = needed_schema
                    .iter()
                    .map(|sc| {
                        let val = columns
                            .iter()
                            .find(|(id, _)| *id == sc.id)
                            .map(|(_, v)| v.clone());
                        (*sc, val)
                    })
                    .collect();
                let row = bytes_to_row(&col_values);

                if let Some(pred) = predicate {
                    if !evaluate_predicate(pred, &row, schema_cols) {
                        continue;
                    }
                }
                rows.push(row);
            }
        }
        let assemble_ms = t_assemble.elapsed().as_secs_f64() * 1000.0;

        tracing::info!(
            scanned,
            matched = rows.len(),
            needed_cols = needed_schema.len(),
            total_cols = schema_cols.len(),
            scan_ms = format!("{:.1}", scan_ms),
            assemble_ms = format!("{:.1}", assemble_ms),
            "fetch_filtered_rows breakdown"
        );

        Ok(rows)
    }

    /// Columnar scan path: build/use in-memory typed arrays, apply SIMD filters.
    async fn fetch_columnar(
        &self,
        db_id: u32,
        table_id: u32,
        schema_cols: &[ColumnInfo],
        needed_col_ids: &HashSet<u32>,
        predicate: Option<&Expr>,
    ) -> PgWireResult<Vec<Row>> {
        let t_total = std::time::Instant::now();

        let (cache_hit, cache_ms) = self
            .ensure_column_cache(db_id, table_id, schema_cols)
            .await?;

        let cache = self.column_cache.read().await;
        let table_cols = cache.get(&(db_id, table_id)).unwrap();

        if table_cols.row_count == 0 {
            return Ok(Vec::new());
        }

        // Apply predicate using columnar filters
        let t_filter = std::time::Instant::now();
        let bitmap = if let Some(pred) = predicate {
            apply_columnar_predicate(pred, table_cols, schema_cols)
        } else {
            Bitmap::all_true(table_cols.row_count)
        };
        let filter_ms = t_filter.elapsed().as_secs_f64() * 1000.0;
        record_stage_duration("columnar_filter", filter_ms / 1000.0);

        // Materialize matching rows
        let t_materialize = std::time::Instant::now();
        let needed_schema: Vec<&ColumnInfo> = schema_cols
            .iter()
            .filter(|c| needed_col_ids.contains(&c.id))
            .collect();

        let match_count = bitmap.count_ones();
        let mut rows = Vec::with_capacity(match_count);
        for idx in bitmap.iter_set_bits() {
            let mut row = Row::new();
            for col in &needed_schema {
                let val = table_cols.get_typed_value(idx, col);
                row.insert(col.name.to_lowercase(), val);
            }
            rows.push(row);
        }
        let materialize_ms = t_materialize.elapsed().as_secs_f64() * 1000.0;
        record_stage_duration("row_materialization", materialize_ms / 1000.0);
        record_cardinality("rows_before_filter", table_cols.row_count);
        record_cardinality("rows_after_filter", match_count);

        tracing::info!(
            row_count = table_cols.row_count,
            matched = match_count,
            cache_hit,
            cache_ms = format!("{:.2}", cache_ms),
            filter_ms = format!("{:.2}", filter_ms),
            materialize_ms = format!("{:.2}", materialize_ms),
            total_ms = format!("{:.2}", t_total.elapsed().as_secs_f64() * 1000.0),
            "columnar scan"
        );

        metrics::counter!("snowdrop_full_scans_total").increment(1);
        metrics::counter!("snowdrop_rows_scanned_total").increment(table_cols.row_count as u64);
        metrics::counter!("snowdrop_rows_returned_total").increment(match_count as u64);
        if cache_hit {
            metrics::counter!("snowdrop_column_cache_hits").increment(1);
        } else {
            metrics::counter!("snowdrop_column_cache_misses").increment(1);
        }

        Ok(rows)
    }

    async fn try_fast_columnar_aggregate_response<'a>(
        &self,
        db_id: u32,
        table_id: u32,
        projection: &[SelectItem],
        predicate: Option<&Expr>,
        schema_cols: &[ColumnInfo],
        group_by: &sqlparser::ast::GroupByExpr,
        order_by: Option<&sqlparser::ast::OrderBy>,
        limit: Option<usize>,
        offset: usize,
    ) -> PgWireResult<Option<Response<'a>>> {
        let plan = match build_columnar_aggregate_plan(projection, group_by) {
            Some(plan) => plan,
            None => return Ok(None),
        };

        let (cache_hit, cache_ms) = self
            .ensure_column_cache(db_id, table_id, schema_cols)
            .await?;
        let cache = self.column_cache.read().await;
        let table_cols = cache.get(&(db_id, table_id)).unwrap();

        let t_filter = std::time::Instant::now();
        let bitmap = if let Some(pred) = predicate {
            apply_columnar_predicate(pred, table_cols, schema_cols)
        } else {
            Bitmap::all_true(table_cols.row_count)
        };
        let filter_ms = t_filter.elapsed().as_secs_f64() * 1000.0;
        record_stage_duration("columnar_filter", filter_ms / 1000.0);
        record_cardinality("rows_before_filter", table_cols.row_count);
        record_cardinality("rows_after_filter", bitmap.count_ones());

        let t_agg = std::time::Instant::now();
        let mut agg_rows =
            compute_columnar_aggregate_rows(&plan, table_cols, &bitmap, schema_cols)?;
        let aggregate_ms = t_agg.elapsed().as_secs_f64() * 1000.0;
        record_stage_duration("aggregate", aggregate_ms / 1000.0);

        if let Some(ob) = order_by {
            sort_rows(&mut agg_rows, &ob.exprs, schema_cols)?;
        }

        let paged = paginate_rows(agg_rows, offset, limit);
        record_cardinality("aggregate_output_rows", paged.len());

        tracing::info!(
            row_count = table_cols.row_count,
            matched = bitmap.count_ones(),
            groups = paged.len(),
            cache_hit,
            cache_ms = format!("{:.2}", cache_ms),
            filter_ms = format!("{:.2}", filter_ms),
            aggregate_ms = format!("{:.2}", aggregate_ms),
            "columnar aggregate fast path"
        );

        metrics::counter!("snowdrop_full_scans_total").increment(1);
        metrics::counter!("snowdrop_rows_scanned_total").increment(table_cols.row_count as u64);
        metrics::counter!("snowdrop_rows_returned_total").increment(paged.len() as u64);
        if cache_hit {
            metrics::counter!("snowdrop_column_cache_hits").increment(1);
        } else {
            metrics::counter!("snowdrop_column_cache_misses").increment(1);
        }

        self.named_rows_to_response(&paged, &plan.field_names)
            .map(Some)
    }

    /// Try to satisfy the query using a secondary index scan.
    /// Returns Some(rows) if an index was used, None to fall back to columnar scan.
    async fn try_index_scan(
        &self,
        db_id: u32,
        table_id: u32,
        schema_cols: &[ColumnInfo],
        needed_col_ids: &HashSet<u32>,
        predicate: &Expr,
    ) -> PgWireResult<Option<Vec<Row>>> {
        // Extract simple predicates that can use an index
        let indexed_cols: Vec<&ColumnInfo> = schema_cols.iter().filter(|c| c.indexed).collect();
        if indexed_cols.is_empty() {
            return Ok(None);
        }

        // Try to extract an index-usable predicate from the WHERE clause
        let candidate = extract_index_candidate(predicate, &indexed_cols);
        if candidate.is_none() {
            return Ok(None);
        }
        let (col, op, encoded_value, encoded_high) = candidate.unwrap();

        let t_index = std::time::Instant::now();

        // Execute index scan
        let row_ids = match op.as_str() {
            "eq" => index::index_point_lookup(
                self.storage.as_ref(),
                db_id,
                table_id,
                col.id,
                &encoded_value,
            )
            .await
            .map_err(to_err)?,
            "range" => {
                let high = encoded_high.as_deref();
                index::index_range_scan(
                    self.storage.as_ref(),
                    db_id,
                    table_id,
                    col.id,
                    Some(&encoded_value),
                    high,
                    true,
                    false,
                )
                .await
                .map_err(to_err)?
            }
            "gt" => index::index_range_scan(
                self.storage.as_ref(),
                db_id,
                table_id,
                col.id,
                Some(&encoded_value),
                None,
                false,
                false,
            )
            .await
            .map_err(to_err)?,
            "gte" => index::index_range_scan(
                self.storage.as_ref(),
                db_id,
                table_id,
                col.id,
                Some(&encoded_value),
                None,
                true,
                false,
            )
            .await
            .map_err(to_err)?,
            "lt" => index::index_range_scan(
                self.storage.as_ref(),
                db_id,
                table_id,
                col.id,
                None,
                Some(&encoded_value),
                false,
                false,
            )
            .await
            .map_err(to_err)?,
            "lte" => index::index_range_scan(
                self.storage.as_ref(),
                db_id,
                table_id,
                col.id,
                None,
                Some(&encoded_value),
                false,
                true,
            )
            .await
            .map_err(to_err)?,
            _ => return Ok(None),
        };

        if row_ids.is_empty() {
            tracing::info!(col = col.name, op, "index scan: 0 rows");
            return Ok(Some(Vec::new()));
        }

        let total_rows_hint = {
            let cache = self.column_cache.read().await;
            cache.get(&(db_id, table_id)).map(|table| table.row_count)
        };
        record_cardinality("index_candidate_rows", row_ids.len());
        if !should_use_index_scan(&op, row_ids.len(), total_rows_hint) {
            tracing::info!(
                col = col.name,
                op,
                candidate_rows = row_ids.len(),
                total_rows = total_rows_hint.unwrap_or(0),
                "index scan skipped by selectivity guard"
            );
            return Ok(None);
        }

        // Fetch rows by row_ids via MGET
        let row_keys: Vec<String> = row_ids
            .iter()
            .filter_map(|rid| keys::build_row_key(db_id, table_id, *rid).ok())
            .collect();
        let values = self.storage.mget(&row_keys).await.map_err(to_err)?;

        // Deserialize and apply remaining predicates
        let mut rows = Vec::new();
        for val in values.into_iter().flatten() {
            let cols = deserialize_columns(&val);
            let mut row = Row::new();
            for sc in schema_cols {
                if needed_col_ids.contains(&sc.id) {
                    let raw = cols
                        .iter()
                        .find(|(id, _)| *id == sc.id)
                        .map(|(_, v)| v.as_slice());
                    let typed = match raw {
                        Some(b) if !b.is_empty() => TypedValue::from_bytes(b, &sc.data_type),
                        _ => TypedValue::Null,
                    };
                    row.insert(sc.name.to_lowercase(), typed);
                }
            }
            // Apply the full predicate (handles AND with non-indexed columns)
            if evaluate_predicate(predicate, &row, schema_cols) {
                rows.push(row);
            }
        }

        let index_ms = t_index.elapsed().as_secs_f64() * 1000.0;
        tracing::info!(
            col = col.name,
            op,
            index_hits = row_ids.len(),
            post_filter = rows.len(),
            index_ms = format!("{:.2}", index_ms),
            "index scan"
        );
        record_stage_duration("index_lookup", index_ms / 1000.0);

        metrics::counter!("snowdrop_index_scans_total").increment(1);
        metrics::counter!("snowdrop_rows_returned_total").increment(rows.len() as u64);

        Ok(Some(rows))
    }

    /// Invalidate the column cache for a table (called after INSERT/UPDATE/DELETE).
    async fn invalidate_column_cache(&self, db_id: u32, table_id: u32) {
        let mut cache = self.column_cache.write().await;
        cache.remove(&(db_id, table_id));
    }

    /// Convert typed rows to a pgwire query response.
    fn rows_to_response<'a>(
        &self,
        rows: &[Row],
        projected_cols: &[ColumnInfo],
    ) -> PgWireResult<Response<'a>> {
        let t_encode = std::time::Instant::now();
        let fields = Arc::new(
            projected_cols
                .iter()
                .map(|c| text_field(&c.name))
                .collect::<Vec<_>>(),
        );

        let mut data_rows: Vec<DataRow> = Vec::new();
        for row in rows {
            let mut encoder = DataRowEncoder::new(fields.clone());
            for col in projected_cols {
                let val = row.get(&col.name.to_lowercase());
                match val {
                    Some(TypedValue::Null) | None => encoder.encode_field(&None::<&str>)?,
                    Some(tv) => encoder.encode_field(&tv.to_string())?,
                }
            }
            data_rows.push(encoder.finish()?);
        }

        record_stage_duration("pgwire_encode", t_encode.elapsed().as_secs_f64());
        Ok(Response::Query(query_response(fields, data_rows)))
    }

    /// Build pgwire response by evaluating expression projections per row.
    fn expr_rows_to_response<'a>(
        &self,
        rows: &[Row],
        projected_exprs: &[(String, Expr)],
        schema_cols: &[ColumnInfo],
    ) -> PgWireResult<Response<'a>> {
        let t_encode = std::time::Instant::now();
        let fields = Arc::new(
            projected_exprs
                .iter()
                .map(|(name, _)| text_field(name))
                .collect::<Vec<_>>(),
        );

        let mut data_rows: Vec<DataRow> = Vec::new();
        for row in rows {
            let mut encoder = DataRowEncoder::new(fields.clone());
            for (_, expr) in projected_exprs {
                let val = resolve_value(expr, row, schema_cols);
                match val {
                    TypedValue::Null => encoder.encode_field(&None::<&str>)?,
                    tv => encoder.encode_field(&tv.to_string())?,
                }
            }
            data_rows.push(encoder.finish()?);
        }

        record_stage_duration("pgwire_encode", t_encode.elapsed().as_secs_f64());
        Ok(Response::Query(query_response(fields, data_rows)))
    }

    fn named_rows_to_response<'a>(
        &self,
        rows: &[Row],
        field_names: &[String],
    ) -> PgWireResult<Response<'a>> {
        let t_encode = std::time::Instant::now();
        let fields = Arc::new(
            field_names
                .iter()
                .map(|name| text_field(name))
                .collect::<Vec<_>>(),
        );

        let mut data_rows: Vec<DataRow> = Vec::new();
        for row in rows {
            let mut encoder = DataRowEncoder::new(fields.clone());
            for name in field_names {
                let val = lookup_row_value(row, &name.to_lowercase());
                match val {
                    TypedValue::Null => encoder.encode_field(&None::<&str>)?,
                    tv => encoder.encode_field(&tv.to_string())?,
                }
            }
            data_rows.push(encoder.finish()?);
        }

        record_stage_duration("pgwire_encode", t_encode.elapsed().as_secs_f64());
        Ok(Response::Query(query_response(fields, data_rows)))
    }

    fn relation_to_response<'a>(&self, relation: &RelationResult) -> PgWireResult<Response<'a>> {
        let t_encode = std::time::Instant::now();
        let fields = Arc::new(
            relation
                .columns
                .iter()
                .map(|col| text_field(&col.output_name))
                .collect::<Vec<_>>(),
        );

        let mut data_rows: Vec<DataRow> = Vec::new();
        for row in &relation.rows {
            let mut encoder = DataRowEncoder::new(fields.clone());
            for col in &relation.columns {
                let val = lookup_row_value(row, &col.lookup_name);
                match val {
                    TypedValue::Null => encoder.encode_field(&None::<&str>)?,
                    tv => encoder.encode_field(&tv.to_string())?,
                }
            }
            data_rows.push(encoder.finish()?);
        }

        record_stage_duration("pgwire_encode", t_encode.elapsed().as_secs_f64());
        Ok(Response::Query(query_response(fields, data_rows)))
    }

    async fn execute_query_to_relation(
        &self,
        query: &sqlparser::ast::Query,
    ) -> PgWireResult<RelationResult> {
        self.execute_query_to_relation_boxed(query, &RelationContext::default())
            .await
    }

    fn execute_query_to_relation_boxed<'a>(
        &'a self,
        query: &'a sqlparser::ast::Query,
        ctx: &'a RelationContext,
    ) -> BoxFuture<'a, PgWireResult<RelationResult>> {
        async move {
            let mut local_ctx = ctx.clone();
            if let Some(with) = &query.with {
                if with.recursive {
                    return Err(user_error("recursive CTE is not supported yet".to_string()));
                }

                let mut ctes = ctx.ctes.clone();
                for cte in &with.cte_tables {
                    let cte_ctx = ctx.with_ctes(ctes.clone());
                    let mut relation = self
                        .execute_query_to_relation_boxed(cte.query.as_ref(), &cte_ctx)
                        .await?;
                    if !cte.alias.columns.is_empty() {
                        relation = rename_relation_columns(
                            relation,
                            &cte.alias
                                .columns
                                .iter()
                                .map(|ident| ident.name.value.clone())
                                .collect::<Vec<_>>(),
                        )?;
                    }
                    ctes.insert(cte.alias.name.value.to_lowercase(), relation);
                }
                local_ctx.ctes = ctes;
            }

            match query.body.as_ref() {
                SetExpr::Select(select) => {
                    validate_select_expressions(select.as_ref(), query)?;
                    if ctx.ctes.is_empty()
                        && ctx.outer_rows.is_empty()
                        && query_can_use_optimized_relation_fast_path(query)
                    {
                        let (rows, field_names) =
                            self.materialize_legacy_query_output(query).await?;
                        return Ok(RelationResult::from_named_rows(rows, field_names));
                    }
                    self.execute_select_to_relation(select.as_ref(), query, &local_ctx)
                        .await
                }
                SetExpr::Query(inner) => {
                    let relation = self
                        .execute_query_to_relation_boxed(inner.as_ref(), &local_ctx)
                        .await?;
                    self.apply_query_post_processing(relation, query, &local_ctx)
                        .await
                }
                SetExpr::SetOperation {
                    op,
                    set_quantifier,
                    left,
                    right,
                } => {
                    let relation = self
                        .execute_set_operation_to_relation(
                            op,
                            set_quantifier,
                            left.as_ref(),
                            right.as_ref(),
                            &local_ctx,
                        )
                        .await?;
                    self.apply_query_post_processing(relation, query, &local_ctx)
                        .await
                }
                SetExpr::Values(_) => Err(user_error(
                    "VALUES query sources are not supported as standalone relation results"
                        .to_string(),
                )),
                _ => Err(user_error("unsupported query form in Phase 3".to_string())),
            }
        }
        .boxed()
    }

    async fn materialize_legacy_query_output(
        &self,
        query: &sqlparser::ast::Query,
    ) -> PgWireResult<(Vec<Row>, Vec<String>)> {
        let select =
            match query.body.as_ref() {
                SetExpr::Select(sel) => sel.clone(),
                _ => return Err(user_error(
                    "only simple SELECT query sources are supported in the current Phase 3 subset"
                        .to_string(),
                )),
            };

        validate_select_expressions(&select, query)?;
        validate_query_surface_boundaries(&select, query)?;

        if select.from.is_empty() {
            return Err(user_error("SELECT requires FROM clause".to_string()));
        }

        if !select.from[0].joins.is_empty() {
            return self.materialize_join_select_output(*select, query).await;
        }

        let table_factor = &select.from[0].relation;
        let table_name_obj = match table_factor {
            sqlparser::ast::TableFactor::Table { name, .. } => name.clone(),
            _ => {
                return Err(user_error(
                    "derived tables and CTE-backed SELECT sources are not yet supported"
                        .to_string(),
                ))
            }
        };

        let current_db = self.current_db.read().await.clone();
        let (db_name, table_name) = self.resolve_table_name(&table_name_obj, &current_db);

        let schema_cols = self.cached_describe_table(&db_name, &table_name).await?;
        let db = self
            .cached_get_database(&db_name)
            .await?
            .ok_or_else(|| user_error(format!("database '{}' does not exist", db_name)))?;
        let table = self
            .cached_get_table(&db_name, &table_name)
            .await?
            .ok_or_else(|| user_error(format!("table '{}' does not exist", table_name)))?;

        let agg_exprs = extract_aggregates(&select.projection);
        let is_aggregate = !agg_exprs.is_empty();
        let has_expr_proj = has_expression_projection(&select.projection);

        let projected_cols = if is_aggregate || has_expr_proj {
            schema_cols.clone()
        } else {
            resolve_projection(&select.projection, &schema_cols)?
        };

        let needed_col_ids = compute_needed_col_ids(
            select.selection.as_ref(),
            &select.projection,
            query.order_by.as_ref(),
            &select.group_by,
            &schema_cols,
        );

        let (query_offset, query_limit) = query_offset_limit(query);
        let agg_offset = query_offset;
        let agg_limit = query_limit;

        let filtered = if let Some(pred) = select.selection.as_ref() {
            self.fetch_columnar(db.id, table.id, &schema_cols, &needed_col_ids, Some(pred))
                .await?
        } else {
            self.fetch_columnar(db.id, table.id, &schema_cols, &needed_col_ids, None)
                .await?
        };

        if is_aggregate {
            return self.materialize_aggregate_result(
                &select.projection,
                &filtered,
                &schema_cols,
                &select.group_by,
                select.having.as_ref(),
                query.order_by.as_ref(),
                agg_limit,
                agg_offset,
            );
        }

        let mut result_rows = filtered;
        if let Some(ref order_by) = query.order_by {
            sort_rows(&mut result_rows, &order_by.exprs, &schema_cols)?;
        }

        let paged = paginate_rows(result_rows, query_offset, query_limit);

        if has_expr_proj {
            let proj_exprs = resolve_projection_exprs(&select.projection, &schema_cols)?;
            Ok(materialize_expression_projection(
                &paged,
                &proj_exprs,
                &schema_cols,
            ))
        } else {
            Ok(materialize_named_projection(&paged, &projected_cols))
        }
    }

    async fn materialize_join_select_output(
        &self,
        select: sqlparser::ast::Select,
        query: &sqlparser::ast::Query,
    ) -> PgWireResult<(Vec<Row>, Vec<String>)> {
        let current_db = self.current_db.read().await.clone();
        let include_all_columns = projection_needs_all_columns(&select.projection);
        let query_refs = collect_join_query_refs(&select, query);
        let mut remaining_conjuncts = Vec::new();
        if let Some(selection) = select.selection.as_ref() {
            validate_no_subqueries(selection, "subqueries in JOIN filters")?;
            collect_conjuncts(selection, &mut remaining_conjuncts);
        }
        if let Some(having) = select.having.as_ref() {
            validate_no_subqueries(having, "subqueries in HAVING")?;
        }
        if let Some(ref order_by) = query.order_by {
            for expr in &order_by.exprs {
                validate_no_subqueries(&expr.expr, "subqueries in ORDER BY")?;
            }
        }

        let (left_name, left_alias) = extract_table_info(&select.from[0].relation)?;
        let (left_db, left_tbl) = self.resolve_table_name(&left_name, &current_db);
        let left_schema = self.cached_describe_table(&left_db, &left_tbl).await?;
        let left_db_info = self
            .cached_get_database(&left_db)
            .await?
            .ok_or_else(|| user_error(format!("database '{}' does not exist", left_db)))?;
        let left_tbl_info = self
            .cached_get_table(&left_db, &left_tbl)
            .await?
            .ok_or_else(|| user_error(format!("table '{}' does not exist", left_tbl)))?;

        let left_prefix = left_alias.as_deref().unwrap_or(&left_tbl).to_lowercase();
        let left_pushdown =
            take_pushdown_predicate(&mut remaining_conjuncts, &left_prefix, &left_schema);
        let left_needed_col_ids = compute_join_needed_col_ids(
            &left_schema,
            &left_prefix,
            &query_refs,
            include_all_columns,
        );
        let left_rows = self
            .fetch_filtered_rows(
                left_db_info.id,
                left_tbl_info.id,
                &left_schema,
                &left_needed_col_ids,
                left_pushdown.as_ref(),
            )
            .await?;
        let mut joined_rows: Vec<Row> = left_rows
            .into_iter()
            .map(|r| prefix_row(r, &left_prefix))
            .collect();
        let mut combined_schema = left_schema
            .iter()
            .map(|c| ColumnInfo {
                name: format!("{}.{}", left_prefix, c.name.to_lowercase()),
                ..c.clone()
            })
            .collect::<Vec<_>>();

        for join in &select.from[0].joins {
            let (right_name, right_alias) = extract_table_info(&join.relation)?;
            let (right_db, right_tbl) = self.resolve_table_name(&right_name, &current_db);
            let right_schema = self.cached_describe_table(&right_db, &right_tbl).await?;
            let right_db_info = self
                .cached_get_database(&right_db)
                .await?
                .ok_or_else(|| user_error(format!("database '{}' does not exist", right_db)))?;
            let right_tbl_info = self
                .cached_get_table(&right_db, &right_tbl)
                .await?
                .ok_or_else(|| user_error(format!("table '{}' does not exist", right_tbl)))?;

            let (is_left_join, on_expr) = match &join.join_operator {
                JoinOperator::Inner(JoinConstraint::On(expr)) => (false, expr.clone()),
                JoinOperator::LeftOuter(JoinConstraint::On(expr)) => (true, expr.clone()),
                _ => {
                    return Err(user_error(
                        "only INNER JOIN ... ON and LEFT JOIN ... ON are supported".to_string(),
                    ))
                }
            };

            let join_conds = extract_join_conditions(&on_expr)?;
            let right_prefix = right_alias.as_deref().unwrap_or(&right_tbl).to_lowercase();
            let right_pushdown =
                take_pushdown_predicate(&mut remaining_conjuncts, &right_prefix, &right_schema);
            let right_needed_col_ids = compute_join_needed_col_ids(
                &right_schema,
                &right_prefix,
                &query_refs,
                include_all_columns,
            );
            let right_rows = self
                .fetch_filtered_rows(
                    right_db_info.id,
                    right_tbl_info.id,
                    &right_schema,
                    &right_needed_col_ids,
                    right_pushdown.as_ref(),
                )
                .await?;
            let right_prefixed: Vec<Row> = right_rows
                .into_iter()
                .map(|r| prefix_row(r, &right_prefix))
                .collect();

            let oriented_conds: Vec<(String, String)> = join_conds
                .iter()
                .map(|(a, b)| {
                    let a_is_right = a.starts_with(&format!("{}.", right_prefix));
                    if a_is_right {
                        (b.clone(), a.clone())
                    } else {
                        (a.clone(), b.clone())
                    }
                })
                .collect();
            let right_join_keys: Vec<String> = oriented_conds
                .iter()
                .map(|(_, rc)| {
                    if rc.contains('.') {
                        rc.clone()
                    } else {
                        format!("{}.{}", right_prefix, rc)
                    }
                })
                .collect();
            let left_join_keys: Vec<String> = oriented_conds
                .iter()
                .map(|(lc, _)| {
                    if lc.contains('.') {
                        lc.clone()
                    } else {
                        combined_schema
                            .iter()
                            .find(|c| c.name.ends_with(&format!(".{}", lc)))
                            .map(|c| c.name.clone())
                            .unwrap_or_else(|| lc.clone())
                    }
                })
                .collect();

            let mut right_hash: HashMap<Vec<String>, Vec<&Row>> = HashMap::new();
            for row in &right_prefixed {
                let key: Vec<String> = right_join_keys
                    .iter()
                    .map(|k| row.get(k).map(|v| v.to_string()).unwrap_or_default())
                    .collect();
                right_hash.entry(key).or_default().push(row);
            }

            let mut new_rows: Vec<Row> = Vec::new();
            for left_row in &joined_rows {
                let left_key: Vec<String> = left_join_keys
                    .iter()
                    .map(|k| left_row.get(k).map(|v| v.to_string()).unwrap_or_default())
                    .collect();

                if let Some(matching_rights) = right_hash.get(&left_key) {
                    for right_row in matching_rights {
                        let mut combined = left_row.clone();
                        combined.extend((*right_row).clone());
                        new_rows.push(combined);
                    }
                } else if is_left_join {
                    let mut combined = left_row.clone();
                    for col in &right_schema {
                        let key = format!("{}.{}", right_prefix, col.name.to_lowercase());
                        combined.insert(key, TypedValue::Null);
                    }
                    new_rows.push(combined);
                }
            }

            joined_rows = new_rows;
            combined_schema.extend(right_schema.iter().map(|c| ColumnInfo {
                name: format!("{}.{}", right_prefix, c.name.to_lowercase()),
                ..c.clone()
            }));
        }

        if let Some(remaining_selection) = combine_conjuncts(remaining_conjuncts) {
            joined_rows
                .retain(|row| evaluate_predicate(&remaining_selection, row, &combined_schema));
        }

        let agg_exprs = extract_aggregates(&select.projection);
        if !agg_exprs.is_empty() {
            let (agg_offset, agg_limit) = query_offset_limit(query);
            return self.materialize_aggregate_result(
                &select.projection,
                &joined_rows,
                &combined_schema,
                &select.group_by,
                select.having.as_ref(),
                query.order_by.as_ref(),
                agg_limit,
                agg_offset,
            );
        }

        let projected = resolve_join_projection(&select.projection, &combined_schema)?;
        if let Some(ref order_by) = query.order_by {
            sort_rows(&mut joined_rows, &order_by.exprs, &combined_schema)?;
        }

        let (offset, limit) = query_offset_limit(query);
        let paged = paginate_rows(joined_rows, offset, limit);

        let mut output_rows = Vec::with_capacity(paged.len());
        let field_names: Vec<String> = projected.iter().map(|(name, _)| name.clone()).collect();
        for row in &paged {
            let mut output_row = Row::new();
            for (name, key) in &projected {
                output_row.insert(name.to_lowercase(), lookup_row_value(row, key));
            }
            output_rows.push(output_row);
        }

        Ok((output_rows, field_names))
    }

    fn materialize_aggregate_result(
        &self,
        projection: &[SelectItem],
        rows: &[Row],
        schema_cols: &[ColumnInfo],
        group_by: &sqlparser::ast::GroupByExpr,
        having: Option<&Expr>,
        order_by: Option<&sqlparser::ast::OrderBy>,
        limit: Option<usize>,
        offset: usize,
    ) -> PgWireResult<(Vec<Row>, Vec<String>)> {
        let group_by_cols = extract_group_by_cols(group_by);
        let field_names: Vec<String> = projection
            .iter()
            .map(|item| match item {
                SelectItem::UnnamedExpr(Expr::Identifier(ident)) => ident.value.clone(),
                SelectItem::UnnamedExpr(Expr::CompoundIdentifier(parts)) => {
                    parts.last().map(|p| p.value.clone()).unwrap_or_default()
                }
                SelectItem::UnnamedExpr(Expr::Function(f)) => f.to_string(),
                SelectItem::ExprWithAlias { alias, .. } => alias.value.clone(),
                _ => "?column?".to_string(),
            })
            .collect();

        if group_by_cols.is_empty() {
            if let Some(having_expr) = having {
                if !evaluate_grouped_predicate(having_expr, rows, schema_cols)? {
                    return Ok((Vec::new(), field_names));
                }
            }

            let agg_row = compute_aggregates(projection, rows, schema_cols)?;
            let mut row = Row::new();
            for (name, val) in agg_row {
                row.insert(name.to_lowercase(), val);
            }
            return Ok((vec![row], field_names));
        }

        let mut buckets: HashMap<Vec<String>, Vec<&Row>> = HashMap::new();
        for row in rows {
            let key: Vec<String> = group_by_cols
                .iter()
                .map(|col| {
                    let val = lookup_row_value(row, col);
                    if val.is_null() {
                        "NULL".to_string()
                    } else {
                        val.to_string()
                    }
                })
                .collect();
            buckets.entry(key).or_default().push(row);
        }

        let mut agg_rows: Vec<Row> = Vec::new();
        for bucket_rows in buckets.values() {
            let owned_rows: Vec<Row> = bucket_rows.iter().map(|r| (*r).clone()).collect();
            if let Some(having_expr) = having {
                if !evaluate_grouped_predicate(having_expr, &owned_rows, schema_cols)? {
                    continue;
                }
            }

            let agg_values = compute_aggregates(projection, &owned_rows, schema_cols)?;
            let mut row = Row::new();
            for (name, val) in &agg_values {
                row.insert(name.to_lowercase(), val.clone());
            }
            agg_rows.push(row);
        }

        if let Some(ob) = order_by {
            sort_rows(&mut agg_rows, &ob.exprs, schema_cols)?;
        }

        let paged = paginate_rows(agg_rows, offset, limit);

        Ok((paged, field_names))
    }

    async fn execute_select_to_relation(
        &self,
        select: &sqlparser::ast::Select,
        query: &sqlparser::ast::Query,
        ctx: &RelationContext,
    ) -> PgWireResult<RelationResult> {
        self.execute_select_body_to_relation(
            select,
            query.order_by.as_ref(),
            query.limit.as_ref(),
            query.offset.as_ref(),
            ctx,
        )
        .await
    }

    async fn execute_select_body_to_relation(
        &self,
        select: &sqlparser::ast::Select,
        order_by: Option<&sqlparser::ast::OrderBy>,
        limit: Option<&Expr>,
        offset: Option<&sqlparser::ast::Offset>,
        ctx: &RelationContext,
    ) -> PgWireResult<RelationResult> {
        validate_select_body_expressions(select)?;

        if select.from.is_empty() {
            return Err(user_error("SELECT requires FROM clause".to_string()));
        }

        let current_db = self.current_db.read().await.clone();
        let mut relation = self
            .build_source_relation(&select.from[0].relation, &current_db, ctx)
            .await?;

        if !select.from[0].joins.is_empty() {
            qualify_relation_for_join_wildcard(&mut relation);
        }

        for join in &select.from[0].joins {
            let (is_left_join, on_expr) = match &join.join_operator {
                JoinOperator::Inner(JoinConstraint::On(expr)) => (false, expr),
                JoinOperator::LeftOuter(JoinConstraint::On(expr)) => (true, expr),
                _ => {
                    return Err(user_error(
                        "only INNER JOIN ... ON and LEFT JOIN ... ON are supported".to_string(),
                    ))
                }
            };

            let mut right_relation = self
                .build_source_relation(&join.relation, &current_db, ctx)
                .await?;
            qualify_relation_for_join_wildcard(&mut right_relation);
            relation = self
                .join_relations(relation, right_relation, on_expr, is_left_join)
                .await?;
        }

        relation = self
            .apply_where_to_relation(relation, select.selection.as_ref(), ctx)
            .await?;

        let agg_exprs = extract_aggregates(&select.projection);
        let mut relation = if !agg_exprs.is_empty() {
            self.execute_aggregate_relation(
                relation,
                &select.projection,
                &select.group_by,
                select.having.as_ref(),
                ctx,
            )
            .await?
        } else {
            self.project_relation(relation, &select.projection, ctx)
                .await?
        };

        if let Some(order_by) = order_by {
            relation = self.relation_order(relation, &order_by.exprs, ctx).await?;
        }

        self.apply_relation_limit_offset(relation, limit, offset)
    }

    fn execute_setexpr_to_relation_boxed<'a>(
        &'a self,
        setexpr: &'a SetExpr,
        ctx: &'a RelationContext,
    ) -> BoxFuture<'a, PgWireResult<RelationResult>> {
        async move {
            match setexpr {
                SetExpr::Select(select) => {
                    self.execute_select_body_to_relation(select.as_ref(), None, None, None, ctx)
                        .await
                }
                SetExpr::Query(query) => {
                    self.execute_query_to_relation_boxed(query.as_ref(), ctx)
                        .await
                }
                SetExpr::SetOperation {
                    op,
                    set_quantifier,
                    left,
                    right,
                } => {
                    self.execute_set_operation_to_relation(
                        op,
                        set_quantifier,
                        left.as_ref(),
                        right.as_ref(),
                        ctx,
                    )
                    .await
                }
                SetExpr::Values(_) => Err(user_error(
                    "VALUES relations are not supported in the current Phase 3 subset".to_string(),
                )),
                _ => Err(user_error("unsupported query form in Phase 3".to_string())),
            }
        }
        .boxed()
    }

    async fn execute_set_operation_to_relation(
        &self,
        op: &sqlparser::ast::SetOperator,
        set_quantifier: &sqlparser::ast::SetQuantifier,
        left: &SetExpr,
        right: &SetExpr,
        ctx: &RelationContext,
    ) -> PgWireResult<RelationResult> {
        if matches!(
            op,
            sqlparser::ast::SetOperator::Intersect | sqlparser::ast::SetOperator::Except
        ) {
            return Err(user_error(format!("{op} is not supported yet")));
        }

        let left_relation = self.execute_setexpr_to_relation_boxed(left, ctx).await?;
        let right_relation = self.execute_setexpr_to_relation_boxed(right, ctx).await?;

        if left_relation.columns.len() != right_relation.columns.len() {
            return Err(user_error(format!(
                "{} requires matching column counts (left={}, right={})",
                op,
                left_relation.columns.len(),
                right_relation.columns.len()
            )));
        }

        validate_set_operation_column_compatibility(op, &left_relation, &right_relation)?;

        let left_normalized = normalize_relation_for_set_op(left_relation);
        let right_normalized = normalize_relation_for_set_op(right_relation);

        let mut rows = left_normalized.rows.clone();
        rows.extend(right_normalized.rows);

        let relation = RelationResult {
            rows,
            columns: left_normalized.columns,
        };

        if is_union_all_quantifier(set_quantifier) {
            Ok(relation)
        } else {
            Ok(deduplicate_relation_rows(relation))
        }
    }

    async fn apply_query_post_processing(
        &self,
        mut relation: RelationResult,
        query: &sqlparser::ast::Query,
        ctx: &RelationContext,
    ) -> PgWireResult<RelationResult> {
        if let Some(order_by) = query.order_by.as_ref() {
            relation = self.relation_order(relation, &order_by.exprs, ctx).await?;
        }
        self.apply_relation_limit_offset(relation, query.limit.as_ref(), query.offset.as_ref())
    }

    async fn build_source_relation(
        &self,
        factor: &sqlparser::ast::TableFactor,
        current_db: &str,
        ctx: &RelationContext,
    ) -> PgWireResult<RelationResult> {
        match factor {
            sqlparser::ast::TableFactor::Table { name, alias, .. } => {
                let raw_name = object_name_to_string(name);
                let cte_key = raw_name.to_lowercase();
                let base_relation = if let Some(cte_relation) = ctx.ctes.get(&cte_key) {
                    cte_relation.clone()
                } else {
                    self.build_physical_table_relation(name, current_db).await?
                };
                let source_label = alias
                    .as_ref()
                    .map(|a| a.name.value.clone())
                    .unwrap_or_else(|| object_name_last(name));
                Ok(prefix_relation_as_source(base_relation, &source_label))
            }
            sqlparser::ast::TableFactor::Derived {
                lateral,
                subquery,
                alias,
            } => {
                if *lateral {
                    return Err(user_error("LATERAL is not supported yet".to_string()));
                }
                let alias = alias
                    .as_ref()
                    .ok_or_else(|| user_error("derived tables require an alias".to_string()))?;
                let mut relation = self
                    .execute_query_to_relation_boxed(subquery.as_ref(), ctx)
                    .await?;
                if !alias.columns.is_empty() {
                    relation = rename_relation_columns(
                        relation,
                        &alias
                            .columns
                            .iter()
                            .map(|ident| ident.name.value.clone())
                            .collect::<Vec<_>>(),
                    )?;
                }
                Ok(prefix_relation_as_source(relation, &alias.name.value))
            }
            _ => Err(user_error(
                "unsupported FROM item in the current Phase 3 subset".to_string(),
            )),
        }
    }

    async fn build_physical_table_relation(
        &self,
        table_name_obj: &ObjectName,
        current_db: &str,
    ) -> PgWireResult<RelationResult> {
        let (db_name, table_name) = self.resolve_table_name(table_name_obj, current_db);
        let schema_cols = self.cached_describe_table(&db_name, &table_name).await?;
        let db = self
            .cached_get_database(&db_name)
            .await?
            .ok_or_else(|| user_error(format!("database '{}' does not exist", db_name)))?;
        let table = self
            .cached_get_table(&db_name, &table_name)
            .await?
            .ok_or_else(|| user_error(format!("table '{}' does not exist", table_name)))?;
        let needed_col_ids: HashSet<u32> = schema_cols.iter().map(|col| col.id).collect();
        let rows = self
            .fetch_columnar(db.id, table.id, &schema_cols, &needed_col_ids, None)
            .await?;
        let columns = schema_cols
            .iter()
            .map(|col| RelationColumn {
                lookup_name: col.name.to_lowercase(),
                output_name: col.name.clone(),
            })
            .collect();
        Ok(RelationResult { rows, columns })
    }

    async fn join_relations(
        &self,
        left: RelationResult,
        right: RelationResult,
        on_expr: &Expr,
        is_left_join: bool,
    ) -> PgWireResult<RelationResult> {
        let join_conds = extract_join_conditions(on_expr)?;
        let right_lookup_names: HashSet<String> = right
            .columns
            .iter()
            .map(|col| col.lookup_name.clone())
            .collect();

        let oriented_conds: Vec<(String, String)> = join_conds
            .iter()
            .map(|(a, b)| {
                let a_is_right = right_lookup_names.contains(&normalize_join_key(a));
                if a_is_right {
                    (normalize_join_key(b), normalize_join_key(a))
                } else {
                    (normalize_join_key(a), normalize_join_key(b))
                }
            })
            .collect();

        let right_join_keys: Vec<String> = oriented_conds
            .iter()
            .map(|(_, right)| right.clone())
            .collect();
        let left_join_keys: Vec<String> = oriented_conds
            .iter()
            .map(|(left, _)| left.clone())
            .collect();

        let mut right_hash: HashMap<Vec<String>, Vec<&Row>> = HashMap::new();
        for row in &right.rows {
            let key = right_join_keys
                .iter()
                .map(|lookup| lookup_row_value(row, lookup).to_string())
                .collect::<Vec<_>>();
            right_hash.entry(key).or_default().push(row);
        }

        let mut rows = Vec::new();
        for left_row in &left.rows {
            let left_key = left_join_keys
                .iter()
                .map(|lookup| lookup_row_value(left_row, lookup).to_string())
                .collect::<Vec<_>>();

            if let Some(matches) = right_hash.get(&left_key) {
                for right_row in matches {
                    let mut combined = left_row.clone();
                    combined.extend((*right_row).clone());
                    rows.push(combined);
                }
            } else if is_left_join {
                let mut combined = left_row.clone();
                for col in &right.columns {
                    combined.insert(col.lookup_name.clone(), TypedValue::Null);
                }
                rows.push(combined);
            }
        }

        let mut columns = left.columns;
        columns.extend(right.columns);
        for col in &mut columns {
            col.output_name = col.lookup_name.clone();
        }

        Ok(RelationResult { rows, columns })
    }

    async fn apply_where_to_relation(
        &self,
        mut relation: RelationResult,
        selection: Option<&Expr>,
        ctx: &RelationContext,
    ) -> PgWireResult<RelationResult> {
        let Some(selection) = selection else {
            return Ok(relation);
        };

        let schema_cols = relation.schema_cols();
        let mut filtered = Vec::with_capacity(relation.rows.len());
        for row in relation.rows {
            if self
                .evaluate_predicate_for_relation(selection, &row, &schema_cols, ctx)
                .await?
            {
                filtered.push(row);
            }
        }
        relation.rows = filtered;
        Ok(relation)
    }

    async fn execute_aggregate_relation(
        &self,
        relation: RelationResult,
        projection: &[SelectItem],
        group_by: &sqlparser::ast::GroupByExpr,
        having: Option<&Expr>,
        ctx: &RelationContext,
    ) -> PgWireResult<RelationResult> {
        let schema_cols = relation.schema_cols();
        let group_by_cols = extract_group_by_cols(group_by);
        let field_names: Vec<String> = projection
            .iter()
            .map(|item| match item {
                SelectItem::UnnamedExpr(Expr::Identifier(ident)) => ident.value.clone(),
                SelectItem::UnnamedExpr(Expr::CompoundIdentifier(parts)) => {
                    parts.last().map(|p| p.value.clone()).unwrap_or_default()
                }
                SelectItem::UnnamedExpr(Expr::Function(f)) => f.to_string(),
                SelectItem::ExprWithAlias { alias, .. } => alias.value.clone(),
                _ => "?column?".to_string(),
            })
            .collect();

        let visible_columns: Vec<RelationColumn> = field_names
            .iter()
            .map(|name| RelationColumn {
                lookup_name: name.to_lowercase(),
                output_name: name.clone(),
            })
            .collect();

        if group_by_cols.is_empty() {
            let agg_values = compute_aggregates(projection, &relation.rows, &schema_cols)?;
            let mut row = relation.rows.first().cloned().unwrap_or_default();
            for (name, value) in &agg_values {
                row.insert(name.to_lowercase(), value.clone());
            }
            if let Some(having_expr) = having {
                if !self
                    .evaluate_grouped_predicate_for_relation(
                        having_expr,
                        &relation.rows,
                        &schema_cols,
                        &row,
                        ctx,
                    )
                    .await?
                {
                    return Ok(RelationResult {
                        rows: Vec::new(),
                        columns: visible_columns,
                    });
                }
            }
            return Ok(RelationResult {
                rows: vec![row],
                columns: visible_columns,
            });
        }

        let mut buckets: HashMap<Vec<String>, Vec<Row>> = HashMap::new();
        for row in relation.rows {
            let key = group_by_cols
                .iter()
                .map(|col| {
                    let value = lookup_row_value(&row, col);
                    if value.is_null() {
                        "NULL".to_string()
                    } else {
                        value.to_string()
                    }
                })
                .collect::<Vec<_>>();
            buckets.entry(key).or_default().push(row);
        }

        let mut rows = Vec::new();
        for bucket_rows in buckets.into_values() {
            let agg_values = compute_aggregates(projection, &bucket_rows, &schema_cols)?;
            let mut row = bucket_rows.first().cloned().unwrap_or_default();
            for (name, value) in &agg_values {
                row.insert(name.to_lowercase(), value.clone());
            }
            if let Some(having_expr) = having {
                if !self
                    .evaluate_grouped_predicate_for_relation(
                        having_expr,
                        &bucket_rows,
                        &schema_cols,
                        &row,
                        ctx,
                    )
                    .await?
                {
                    continue;
                }
            }
            rows.push(row);
        }

        Ok(RelationResult {
            rows,
            columns: visible_columns,
        })
    }

    async fn project_relation(
        &self,
        relation: RelationResult,
        projection: &[SelectItem],
        ctx: &RelationContext,
    ) -> PgWireResult<RelationResult> {
        let schema_cols = relation.schema_cols();
        let single_source_wildcard = !relation
            .columns
            .iter()
            .any(|col| col.output_name.contains('.'));
        let visible_columns =
            expand_projection_columns(projection, &relation, single_source_wildcard)?;
        let projected_exprs = resolve_projection_exprs(projection, &schema_cols)?;

        let mut rows = Vec::with_capacity(relation.rows.len());
        for row in relation.rows {
            let mut enriched = row.clone();
            for (name, expr) in &projected_exprs {
                let value = self
                    .resolve_relation_value(expr, &row, &schema_cols, ctx)
                    .await?;
                enriched.insert(name.to_lowercase(), value);
            }
            rows.push(enriched);
        }

        Ok(RelationResult {
            rows,
            columns: visible_columns,
        })
    }

    async fn relation_order(
        &self,
        mut relation: RelationResult,
        order_by: &[OrderByExpr],
        ctx: &RelationContext,
    ) -> PgWireResult<RelationResult> {
        if order_by.is_empty() {
            return Ok(relation);
        }

        let schema_cols = relation.schema_cols();
        let mut keyed_rows = Vec::with_capacity(relation.rows.len());
        for row in relation.rows {
            let mut keys = Vec::with_capacity(order_by.len());
            for order_expr in order_by {
                keys.push(
                    self.resolve_relation_value(&order_expr.expr, &row, &schema_cols, ctx)
                        .await?,
                );
            }
            keyed_rows.push((keys, row));
        }

        keyed_rows.sort_by(|(left_keys, _), (right_keys, _)| {
            for (idx, order_expr) in order_by.iter().enumerate() {
                let mut cmp = left_keys[idx].cmp_order(&right_keys[idx]);
                if order_expr.asc == Some(false) {
                    cmp = cmp.reverse();
                }
                if cmp != std::cmp::Ordering::Equal {
                    return cmp;
                }
            }
            std::cmp::Ordering::Equal
        });

        relation.rows = keyed_rows.into_iter().map(|(_, row)| row).collect();
        Ok(relation)
    }

    fn apply_relation_limit_offset(
        &self,
        relation: RelationResult,
        limit: Option<&Expr>,
        offset: Option<&sqlparser::ast::Offset>,
    ) -> PgWireResult<RelationResult> {
        let rows = paginate_rows(
            relation.rows,
            offset
                .as_ref()
                .and_then(|o| expr_to_usize(&o.value))
                .unwrap_or(0),
            limit.and_then(expr_to_usize),
        );

        Ok(RelationResult {
            rows,
            columns: relation.columns,
        })
    }

    fn resolve_relation_value<'a>(
        &'a self,
        expr: &'a Expr,
        row: &'a Row,
        schema_cols: &'a [ColumnInfo],
        ctx: &'a RelationContext,
    ) -> BoxFuture<'a, PgWireResult<TypedValue>> {
        async move {
            let scoped_row = merge_scope_rows(&ctx.outer_rows, row);
            if !contains_subquery_expr(expr) {
                return Ok(resolve_value(expr, &scoped_row, schema_cols));
            }

            match expr {
                Expr::Subquery(subquery) => {
                    let child_ctx = ctx.with_outer_row(row.clone());
                    self.scalar_subquery_to_value(subquery.as_ref(), &child_ctx)
                        .await
                }
                Expr::Nested(inner) => {
                    self.resolve_relation_value(inner, row, schema_cols, ctx)
                        .await
                }
                Expr::UnaryOp {
                    op: sqlparser::ast::UnaryOperator::Minus,
                    expr,
                } => {
                    let value = self
                        .resolve_relation_value(expr, row, schema_cols, ctx)
                        .await?;
                    Ok(match value {
                        TypedValue::Int(i) => TypedValue::Int(-i),
                        TypedValue::Float(f) => TypedValue::Float(-f),
                        _ => TypedValue::Null,
                    })
                }
                Expr::Cast {
                    expr, data_type, ..
                } => {
                    let value = self
                        .resolve_relation_value(expr, row, schema_cols, ctx)
                        .await?;
                    Ok(value
                        .cast_to_data_type(data_type)
                        .unwrap_or(TypedValue::Null))
                }
                Expr::BinaryOp { left, op, right }
                    if matches!(
                        op,
                        sqlparser::ast::BinaryOperator::Plus
                            | sqlparser::ast::BinaryOperator::Minus
                            | sqlparser::ast::BinaryOperator::Multiply
                            | sqlparser::ast::BinaryOperator::Divide
                            | sqlparser::ast::BinaryOperator::Modulo
                    ) =>
                {
                    let left_val = self
                        .resolve_relation_value(left, row, schema_cols, ctx)
                        .await?;
                    let right_val = self
                        .resolve_relation_value(right, row, schema_cols, ctx)
                        .await?;
                    Ok(apply_arithmetic_values(&left_val, op, &right_val))
                }
                Expr::Function(function) => Err(user_error(format!(
                    "subqueries inside function '{}' are not supported yet",
                    function.name
                ))),
                Expr::Substring { .. } | Expr::Trim { .. } | Expr::Case { .. } => Err(user_error(
                    "subqueries inside complex scalar expressions are not supported yet"
                        .to_string(),
                )),
                _ => Ok(resolve_value(expr, &scoped_row, schema_cols)),
            }
        }
        .boxed()
    }

    async fn scalar_subquery_to_value(
        &self,
        query: &sqlparser::ast::Query,
        ctx: &RelationContext,
    ) -> PgWireResult<TypedValue> {
        let relation = self.execute_query_to_relation_boxed(query, ctx).await?;
        if relation.columns.len() != 1 {
            return Err(user_error(
                "scalar subquery must return exactly one column".to_string(),
            ));
        }
        if relation.rows.len() > 1 {
            return Err(user_error(
                "scalar subquery returned more than one row".to_string(),
            ));
        }
        Ok(relation
            .rows
            .first()
            .map(|row| lookup_row_value(row, &relation.columns[0].lookup_name))
            .unwrap_or(TypedValue::Null))
    }

    fn evaluate_predicate_for_relation<'a>(
        &'a self,
        expr: &'a Expr,
        row: &'a Row,
        schema_cols: &'a [ColumnInfo],
        ctx: &'a RelationContext,
    ) -> BoxFuture<'a, PgWireResult<bool>> {
        async move {
            let scoped_row = merge_scope_rows(&ctx.outer_rows, row);
            if !contains_subquery_expr(expr) {
                return Ok(evaluate_predicate(expr, &scoped_row, schema_cols));
            }

            match expr {
                Expr::Exists { subquery, negated } => {
                    let child_ctx = ctx.with_outer_row(row.clone());
                    let exists = !self
                        .execute_query_to_relation_boxed(subquery.as_ref(), &child_ctx)
                        .await?
                        .rows
                        .is_empty();
                    Ok(if *negated { !exists } else { exists })
                }
                Expr::InSubquery {
                    expr,
                    subquery,
                    negated,
                } => {
                    let left_val = self
                        .resolve_relation_value(expr, row, schema_cols, ctx)
                        .await?;
                    let child_ctx = ctx.with_outer_row(row.clone());
                    let relation = self
                        .execute_query_to_relation_boxed(subquery.as_ref(), &child_ctx)
                        .await?;
                    if relation.columns.len() != 1 {
                        return Err(user_error(
                            "IN subquery must return exactly one column".to_string(),
                        ));
                    }
                    let found = relation.rows.iter().any(|sub_row| {
                        left_val
                            .sql_eq(&lookup_row_value(sub_row, &relation.columns[0].lookup_name))
                    });
                    Ok(if *negated { !found } else { found })
                }
                Expr::BinaryOp { left, op, right } => match op {
                    sqlparser::ast::BinaryOperator::And => Ok(self
                        .evaluate_predicate_for_relation(left, row, schema_cols, ctx)
                        .await?
                        && self
                            .evaluate_predicate_for_relation(right, row, schema_cols, ctx)
                            .await?),
                    sqlparser::ast::BinaryOperator::Or => Ok(self
                        .evaluate_predicate_for_relation(left, row, schema_cols, ctx)
                        .await?
                        || self
                            .evaluate_predicate_for_relation(right, row, schema_cols, ctx)
                            .await?),
                    sqlparser::ast::BinaryOperator::Eq
                    | sqlparser::ast::BinaryOperator::NotEq
                    | sqlparser::ast::BinaryOperator::Lt
                    | sqlparser::ast::BinaryOperator::LtEq
                    | sqlparser::ast::BinaryOperator::Gt
                    | sqlparser::ast::BinaryOperator::GtEq => {
                        let left_val = self
                            .resolve_relation_value(left, row, schema_cols, ctx)
                            .await?;
                        let right_val = self
                            .resolve_relation_value(right, row, schema_cols, ctx)
                            .await?;
                        Ok(compare_typed_values(&left_val, op, &right_val))
                    }
                    _ => Ok(evaluate_predicate(expr, &scoped_row, schema_cols)),
                },
                Expr::UnaryOp {
                    op: sqlparser::ast::UnaryOperator::Not,
                    expr,
                } => Ok(!self
                    .evaluate_predicate_for_relation(expr, row, schema_cols, ctx)
                    .await?),
                Expr::IsNull(inner) => Ok(self
                    .resolve_relation_value(inner, row, schema_cols, ctx)
                    .await?
                    .is_null()),
                Expr::IsNotNull(inner) => Ok(!self
                    .resolve_relation_value(inner, row, schema_cols, ctx)
                    .await?
                    .is_null()),
                Expr::Between {
                    expr,
                    low,
                    high,
                    negated,
                } => {
                    let value = self
                        .resolve_relation_value(expr, row, schema_cols, ctx)
                        .await?;
                    let low_val = self
                        .resolve_relation_value(low, row, schema_cols, ctx)
                        .await?;
                    let high_val = self
                        .resolve_relation_value(high, row, schema_cols, ctx)
                        .await?;
                    let in_range = (value.sql_eq(&low_val) || value.sql_gt(&low_val))
                        && (value.sql_eq(&high_val) || value.sql_lt(&high_val));
                    Ok(if *negated { !in_range } else { in_range })
                }
                Expr::Like {
                    expr,
                    pattern,
                    negated,
                    ..
                } => {
                    let value = self
                        .resolve_relation_value(expr, row, schema_cols, ctx)
                        .await?;
                    let pattern = self
                        .resolve_relation_value(pattern, row, schema_cols, ctx)
                        .await?;
                    let matches = if let TypedValue::Text(pattern_text) = pattern {
                        value.sql_like(&pattern_text)
                    } else {
                        false
                    };
                    Ok(if *negated { !matches } else { matches })
                }
                Expr::InList {
                    expr,
                    list,
                    negated,
                } => {
                    let value = self
                        .resolve_relation_value(expr, row, schema_cols, ctx)
                        .await?;
                    let found = {
                        let mut found = false;
                        for item in list {
                            let candidate = self
                                .resolve_relation_value(item, row, schema_cols, ctx)
                                .await?;
                            if value.sql_eq(&candidate) {
                                found = true;
                                break;
                            }
                        }
                        found
                    };
                    Ok(if *negated { !found } else { found })
                }
                Expr::Nested(inner) => {
                    self.evaluate_predicate_for_relation(inner, row, schema_cols, ctx)
                        .await
                }
                _ => Ok(evaluate_predicate(expr, &scoped_row, schema_cols)),
            }
        }
        .boxed()
    }

    fn evaluate_grouped_predicate_for_relation<'a>(
        &'a self,
        expr: &'a Expr,
        rows: &'a [Row],
        schema_cols: &'a [ColumnInfo],
        grouped_row: &'a Row,
        ctx: &'a RelationContext,
    ) -> BoxFuture<'a, PgWireResult<bool>> {
        async move {
            if !contains_subquery_expr(expr) {
                return evaluate_grouped_predicate(expr, rows, schema_cols);
            }

            match expr {
                Expr::Exists { subquery, negated } => {
                    let child_ctx = ctx.with_outer_row(grouped_row.clone());
                    let exists = !self
                        .execute_query_to_relation_boxed(subquery.as_ref(), &child_ctx)
                        .await?
                        .rows
                        .is_empty();
                    Ok(if *negated { !exists } else { exists })
                }
                Expr::InSubquery {
                    expr,
                    subquery,
                    negated,
                } => {
                    let left_val = self
                        .eval_grouped_value_for_relation(expr, rows, schema_cols, grouped_row, ctx)
                        .await?;
                    let child_ctx = ctx.with_outer_row(grouped_row.clone());
                    let relation = self
                        .execute_query_to_relation_boxed(subquery.as_ref(), &child_ctx)
                        .await?;
                    if relation.columns.len() != 1 {
                        return Err(user_error(
                            "IN subquery must return exactly one column".to_string(),
                        ));
                    }
                    let found = relation.rows.iter().any(|sub_row| {
                        left_val
                            .sql_eq(&lookup_row_value(sub_row, &relation.columns[0].lookup_name))
                    });
                    Ok(if *negated { !found } else { found })
                }
                Expr::BinaryOp { left, op, right } => match op {
                    sqlparser::ast::BinaryOperator::And => Ok(self
                        .evaluate_grouped_predicate_for_relation(
                            left,
                            rows,
                            schema_cols,
                            grouped_row,
                            ctx,
                        )
                        .await?
                        && self
                            .evaluate_grouped_predicate_for_relation(
                                right,
                                rows,
                                schema_cols,
                                grouped_row,
                                ctx,
                            )
                            .await?),
                    sqlparser::ast::BinaryOperator::Or => Ok(self
                        .evaluate_grouped_predicate_for_relation(
                            left,
                            rows,
                            schema_cols,
                            grouped_row,
                            ctx,
                        )
                        .await?
                        || self
                            .evaluate_grouped_predicate_for_relation(
                                right,
                                rows,
                                schema_cols,
                                grouped_row,
                                ctx,
                            )
                            .await?),
                    sqlparser::ast::BinaryOperator::Eq
                    | sqlparser::ast::BinaryOperator::NotEq
                    | sqlparser::ast::BinaryOperator::Lt
                    | sqlparser::ast::BinaryOperator::LtEq
                    | sqlparser::ast::BinaryOperator::Gt
                    | sqlparser::ast::BinaryOperator::GtEq => {
                        let left_val = self
                            .eval_grouped_value_for_relation(
                                left,
                                rows,
                                schema_cols,
                                grouped_row,
                                ctx,
                            )
                            .await?;
                        let right_val = self
                            .eval_grouped_value_for_relation(
                                right,
                                rows,
                                schema_cols,
                                grouped_row,
                                ctx,
                            )
                            .await?;
                        Ok(compare_typed_values(&left_val, op, &right_val))
                    }
                    _ => evaluate_grouped_predicate(expr, rows, schema_cols),
                },
                Expr::UnaryOp {
                    op: sqlparser::ast::UnaryOperator::Not,
                    expr,
                } => Ok(!self
                    .evaluate_grouped_predicate_for_relation(
                        expr,
                        rows,
                        schema_cols,
                        grouped_row,
                        ctx,
                    )
                    .await?),
                Expr::Nested(inner) => {
                    self.evaluate_grouped_predicate_for_relation(
                        inner,
                        rows,
                        schema_cols,
                        grouped_row,
                        ctx,
                    )
                    .await
                }
                _ => evaluate_grouped_predicate(expr, rows, schema_cols),
            }
        }
        .boxed()
    }

    async fn eval_grouped_value_for_relation(
        &self,
        expr: &Expr,
        rows: &[Row],
        schema_cols: &[ColumnInfo],
        grouped_row: &Row,
        ctx: &RelationContext,
    ) -> PgWireResult<TypedValue> {
        match expr {
            Expr::Function(function) if is_aggregate_function_name(&function.name.to_string()) => {
                let func_name = function.name.to_string().to_uppercase();
                let arg_expr = extract_function_arg_expr(function);
                compute_single_aggregate_expr(&func_name, arg_expr.as_ref(), rows, schema_cols)
            }
            Expr::Subquery(subquery) => {
                let child_ctx = ctx.with_outer_row(grouped_row.clone());
                self.scalar_subquery_to_value(subquery.as_ref(), &child_ctx)
                    .await
            }
            _ => {
                self.resolve_relation_value(expr, grouped_row, schema_cols, ctx)
                    .await
            }
        }
    }

    /// Handle aggregate queries (COUNT, SUM, AVG, MIN, MAX) with optional GROUP BY.
    fn handle_aggregate_result<'a>(
        &self,
        projection: &[SelectItem],
        rows: &[Row],
        schema_cols: &[ColumnInfo],
        group_by: &sqlparser::ast::GroupByExpr,
        having: Option<&Expr>,
        order_by: Option<&sqlparser::ast::OrderBy>,
        limit: Option<usize>,
        offset: usize,
    ) -> PgWireResult<Response<'a>> {
        let t_aggregate = std::time::Instant::now();
        let (rows, field_names) = self.materialize_aggregate_result(
            projection,
            rows,
            schema_cols,
            group_by,
            having,
            order_by,
            limit,
            offset,
        )?;
        record_stage_duration("aggregate", t_aggregate.elapsed().as_secs_f64());
        self.named_rows_to_response(&rows, &field_names)
    }

    // ── Query handlers (SHOW, DESCRIBE, INFORMATION_SCHEMA) ──

    async fn handle_show_databases<'a>(&self) -> PgWireResult<Vec<Response<'a>>> {
        let neo4j = self.neo4j.read().await;
        let dbs = neo4j.show_databases().await.map_err(to_err)?;

        let fields = Arc::new(vec![text_field("Database")]);
        let mut rows: Vec<DataRow> = Vec::new();
        for db in &dbs {
            let mut encoder = DataRowEncoder::new(fields.clone());
            encoder.encode_field(&db.name)?;
            rows.push(encoder.finish()?);
        }

        Ok(vec![Response::Query(query_response(fields, rows))])
    }

    async fn handle_show_tables<'a>(&self) -> PgWireResult<Vec<Response<'a>>> {
        let current_db = self.current_db.read().await.clone();
        let neo4j = self.neo4j.read().await;
        let tables = neo4j.show_tables(&current_db).await.map_err(to_err)?;

        let fields = Arc::new(vec![text_field("Table")]);
        let mut rows: Vec<DataRow> = Vec::new();
        for t in &tables {
            let mut encoder = DataRowEncoder::new(fields.clone());
            encoder.encode_field(&t.name)?;
            rows.push(encoder.finish()?);
        }

        Ok(vec![Response::Query(query_response(fields, rows))])
    }

    async fn handle_describe<'a>(&self, table_name: &str) -> PgWireResult<Vec<Response<'a>>> {
        let current_db = self.current_db.read().await.clone();
        let parts: Vec<&str> = table_name.split('.').collect();
        let (db_name, tbl_name) = match parts.len() {
            2 => (parts[0].to_string(), parts[1]),
            _ => (current_db, table_name),
        };

        let neo4j = self.neo4j.read().await;
        let cols = neo4j
            .describe_table(&db_name, tbl_name)
            .await
            .map_err(to_err)?;

        let fields = Arc::new(vec![
            text_field("Field"),
            text_field("Type"),
            text_field("Null"),
            text_field("Key"),
        ]);

        let mut rows: Vec<DataRow> = Vec::new();
        for col in &cols {
            let mut encoder = DataRowEncoder::new(fields.clone());
            encoder.encode_field(&col.name)?;
            encoder.encode_field(&col.data_type)?;
            encoder.encode_field(&(if col.nullable { "YES" } else { "NO" }))?;
            encoder.encode_field(&(if col.is_pk { "PRI" } else { "" }))?;
            rows.push(encoder.finish()?);
        }

        Ok(vec![Response::Query(query_response(fields, rows))])
    }

    async fn handle_info_schema_tables<'a>(&self) -> PgWireResult<Response<'a>> {
        let current_db = self.current_db.read().await.clone();
        let neo4j = self.neo4j.read().await;
        let tables = neo4j
            .information_schema_tables(&current_db)
            .await
            .map_err(to_err)?;

        let fields = Arc::new(vec![
            text_field("table_catalog"),
            text_field("table_schema"),
            text_field("table_name"),
        ]);

        let mut rows: Vec<DataRow> = Vec::new();
        for (catalog, schema_name, name) in &tables {
            let mut encoder = DataRowEncoder::new(fields.clone());
            encoder.encode_field(catalog)?;
            encoder.encode_field(schema_name)?;
            encoder.encode_field(name)?;
            rows.push(encoder.finish()?);
        }

        Ok(Response::Query(query_response(fields, rows)))
    }

    async fn handle_info_schema_columns<'a>(&self) -> PgWireResult<Response<'a>> {
        let current_db = self.current_db.read().await.clone();
        let neo4j = self.neo4j.read().await;
        let cols = neo4j
            .information_schema_columns(&current_db)
            .await
            .map_err(to_err)?;

        let fields = Arc::new(vec![
            text_field("table_catalog"),
            text_field("table_schema"),
            text_field("table_name"),
            text_field("column_name"),
            text_field("data_type"),
            text_field("is_nullable"),
        ]);

        let mut rows: Vec<DataRow> = Vec::new();
        for (catalog, schema_name, table, column, dtype, nullable) in &cols {
            let mut encoder = DataRowEncoder::new(fields.clone());
            encoder.encode_field(catalog)?;
            encoder.encode_field(schema_name)?;
            encoder.encode_field(table)?;
            encoder.encode_field(column)?;
            encoder.encode_field(dtype)?;
            encoder.encode_field(nullable)?;
            rows.push(encoder.finish()?);
        }

        Ok(Response::Query(query_response(fields, rows)))
    }
}

// ── Helpers ──

fn text_field(name: &str) -> FieldInfo {
    FieldInfo::new(
        name.to_string(),
        None,
        None,
        pgwire::api::Type::VARCHAR,
        FieldFormat::Text,
    )
}

fn query_response<'a>(fields: Arc<Vec<FieldInfo>>, rows: Vec<DataRow>) -> QueryResponse<'a> {
    let row_stream = stream::iter(rows.into_iter().map(Ok));
    QueryResponse::new(fields, row_stream)
}

fn classify_statement(stmt: &Statement) -> &'static str {
    match stmt {
        Statement::Query(_) => "select",
        Statement::Insert(_) => "insert",
        Statement::Update { .. } => "update",
        Statement::Delete(_) => "delete",
        Statement::CreateDatabase { .. }
        | Statement::CreateTable(_)
        | Statement::CreateIndex(_) => "ddl",
        Statement::Drop { .. } | Statement::AlterTable { .. } => "ddl",
        _ => "other",
    }
}

fn to_err(e: anyhow::Error) -> PgWireError {
    PgWireError::UserError(Box::new(ErrorInfo::new(
        "ERROR".to_string(),
        "42000".to_string(),
        e.to_string(),
    )))
}

fn user_error(msg: String) -> PgWireError {
    PgWireError::UserError(Box::new(ErrorInfo::new(
        "ERROR".to_string(),
        "42000".to_string(),
        msg,
    )))
}

/// Convert a SQL expression (literal) to raw bytes for storage.
fn expr_to_bytes(expr: &Expr) -> PgWireResult<Vec<u8>> {
    match expr {
        Expr::Value(val) => value_to_bytes(val),
        Expr::UnaryOp { op, expr } => {
            // Handle negative numbers: -1, -3.14
            if matches!(op, sqlparser::ast::UnaryOperator::Minus) {
                let inner = expr_to_bytes(expr)?;
                let inner_str = String::from_utf8(inner)
                    .map_err(|_| user_error("invalid value encoding".to_string()))?;
                Ok(format!("-{}", inner_str).into_bytes())
            } else {
                Err(user_error(format!("unsupported expression: {:?}", expr)))
            }
        }
        _ => Err(user_error(format!("unsupported expression: {:?}", expr))),
    }
}

fn value_to_bytes(val: &Value) -> PgWireResult<Vec<u8>> {
    match val {
        Value::Number(n, _) => Ok(n.as_bytes().to_vec()),
        Value::SingleQuotedString(s) => {
            if s.is_empty() {
                Ok(EMPTY_TEXT_SENTINEL.as_bytes().to_vec())
            } else {
                Ok(s.as_bytes().to_vec())
            }
        }
        Value::DoubleQuotedString(s) => {
            if s.is_empty() {
                Ok(EMPTY_TEXT_SENTINEL.as_bytes().to_vec())
            } else {
                Ok(s.as_bytes().to_vec())
            }
        }
        Value::Boolean(b) => Ok(if *b {
            b"true".to_vec()
        } else {
            b"false".to_vec()
        }),
        Value::Null => Ok(Vec::new()),
        _ => Err(user_error(format!("unsupported value type: {:?}", val))),
    }
}

/// Check if projection is exactly COUNT(*) with no other expressions.
fn is_count_star_only(items: &[SelectItem]) -> bool {
    if items.len() != 1 {
        return false;
    }
    match &items[0] {
        SelectItem::UnnamedExpr(Expr::Function(f))
        | SelectItem::ExprWithAlias {
            expr: Expr::Function(f),
            ..
        } => {
            let name = f.name.to_string().to_uppercase();
            if name != "COUNT" {
                return false;
            }
            match &f.args {
                sqlparser::ast::FunctionArguments::List(list) => {
                    list.args.len() == 1
                        && matches!(
                            &list.args[0],
                            FunctionArg::Unnamed(FunctionArgExpr::Wildcard)
                        )
                }
                _ => false,
            }
        }
        _ => false,
    }
}

/// Resolve SELECT projection to the list of columns to return.
fn resolve_projection(
    items: &[SelectItem],
    schema_cols: &[ColumnInfo],
) -> PgWireResult<Vec<ColumnInfo>> {
    let mut cols = Vec::new();
    for item in items {
        match item {
            SelectItem::Wildcard(_) => {
                cols.extend(schema_cols.iter().cloned());
            }
            SelectItem::UnnamedExpr(Expr::Identifier(ident)) => {
                let name = ident.value.to_lowercase();
                let col = schema_cols
                    .iter()
                    .find(|c| c.name.to_lowercase() == name)
                    .ok_or_else(|| user_error(format!("column '{}' not found", ident.value)))?;
                cols.push(col.clone());
            }
            SelectItem::UnnamedExpr(Expr::CompoundIdentifier(parts)) => {
                let name = parts
                    .last()
                    .map(|p| p.value.to_lowercase())
                    .unwrap_or_default();
                let col = schema_cols
                    .iter()
                    .find(|c| c.name.to_lowercase() == name)
                    .ok_or_else(|| user_error(format!("column '{}' not found", name)))?;
                cols.push(col.clone());
            }
            SelectItem::ExprWithAlias { expr, .. } => {
                if let Expr::Identifier(ident) = expr {
                    let name = ident.value.to_lowercase();
                    let col = schema_cols
                        .iter()
                        .find(|c| c.name.to_lowercase() == name)
                        .ok_or_else(|| user_error(format!("column '{}' not found", ident.value)))?;
                    cols.push(col.clone());
                }
            }
            // Aggregate expressions handled separately
            _ => {}
        }
    }
    if cols.is_empty() {
        // If only aggregates, return all columns (they're needed for computation)
        return Ok(schema_cols.to_vec());
    }
    Ok(cols)
}

/// Check if projection contains expressions (arithmetic, function calls, etc.)
/// that require evaluation rather than simple column lookup.
fn has_expression_projection(items: &[SelectItem]) -> bool {
    items.iter().any(|item| match item {
        SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
            projection_expr_needs_eval(expr)
        }
        _ => false,
    })
}

fn projection_expr_needs_eval(expr: &Expr) -> bool {
    match expr {
        Expr::Identifier(_) | Expr::CompoundIdentifier(_) => false,
        Expr::Function(f) => !is_aggregate_function_name(&f.name.to_string()),
        _ => true,
    }
}

fn resolve_insert_target_columns(
    target_schema: &[ColumnInfo],
    column_names: &[String],
) -> PgWireResult<Vec<ColumnInfo>> {
    column_names
        .iter()
        .map(|name| {
            target_schema
                .iter()
                .find(|col| col.name.eq_ignore_ascii_case(name))
                .cloned()
                .ok_or_else(|| user_error(format!("column '{}' not found", name)))
        })
        .collect()
}

fn validate_insert_select_row_compatibility(
    values: &[TypedValue],
    target_columns: &[ColumnInfo],
) -> PgWireResult<()> {
    for (value, target_col) in values.iter().zip(target_columns) {
        if !typed_value_matches_storage_type(value, &target_col.data_type) {
            return Err(user_error(format!(
                "INSERT ... SELECT value for target column '{}' is not compatible with type {}",
                target_col.name, target_col.data_type
            )));
        }
    }
    Ok(())
}

fn typed_value_matches_storage_type(value: &TypedValue, data_type: &str) -> bool {
    if value.is_null() {
        return true;
    }

    let dt_upper = data_type.to_uppercase();
    if matches!(
        value,
        TypedValue::Text(text) if text == EMPTY_TEXT_SENTINEL
    ) {
        return dt_upper.contains("CHAR") || dt_upper == "TEXT";
    }

    match dt_upper.as_str() {
        "TINYINT" | "SMALLINT" | "INT" | "INTEGER" | "BIGINT" => match value {
            TypedValue::Int(_) => true,
            TypedValue::Float(f) => f.is_finite() && f.fract() == 0.0,
            TypedValue::Text(text) => text.parse::<i64>().is_ok(),
            _ => false,
        },
        "FLOAT" | "DOUBLE" | "REAL" => match value {
            TypedValue::Int(_) | TypedValue::Float(_) => true,
            TypedValue::Text(text) => text.parse::<f64>().is_ok(),
            _ => false,
        },
        "BOOLEAN" | "BOOL" => match value {
            TypedValue::Boolean(_) => true,
            TypedValue::Text(text) => matches!(
                text.to_lowercase().as_str(),
                "true" | "false" | "1" | "0" | "t" | "f"
            ),
            _ => false,
        },
        dt if dt.starts_with("DECIMAL") || dt.starts_with("NUMERIC") => match value {
            TypedValue::Int(_) | TypedValue::Float(_) => true,
            TypedValue::Text(text) => text.parse::<f64>().is_ok(),
            _ => false,
        },
        dt if dt == "DATE" || dt == "DATETIME" || dt == "TIMESTAMP" => {
            matches!(value, TypedValue::Text(_))
        }
        dt if dt.contains("CHAR") || dt == "TEXT" => true,
        _ => true,
    }
}

fn typed_value_to_bytes(value: &TypedValue) -> Vec<u8> {
    match value {
        TypedValue::Null => Vec::new(),
        TypedValue::Text(text) if text.is_empty() => EMPTY_TEXT_SENTINEL.as_bytes().to_vec(),
        TypedValue::Text(text) => text.as_bytes().to_vec(),
        TypedValue::Int(i) => i.to_string().into_bytes(),
        TypedValue::Float(f) => f.to_string().into_bytes(),
        TypedValue::Boolean(flag) => {
            if *flag {
                b"true".to_vec()
            } else {
                b"false".to_vec()
            }
        }
    }
}

fn is_aggregate_function_name(name: &str) -> bool {
    matches!(
        name.to_uppercase().as_str(),
        "COUNT" | "SUM" | "AVG" | "MIN" | "MAX"
    )
}

fn validate_select_expressions(
    select: &sqlparser::ast::Select,
    query: &sqlparser::ast::Query,
) -> PgWireResult<()> {
    validate_select_body_expressions(select)?;

    if let Some(order_by) = query.order_by.as_ref() {
        for expr in &order_by.exprs {
            validate_supported_expr(&expr.expr)?;
        }
    }

    Ok(())
}

fn validate_select_body_expressions(select: &sqlparser::ast::Select) -> PgWireResult<()> {
    if let Some(selection) = select.selection.as_ref() {
        validate_supported_expr(selection)?;
    }

    if let Some(having) = select.having.as_ref() {
        validate_supported_expr(having)?;
    }

    for item in &select.projection {
        match item {
            SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
                validate_supported_expr(expr)?;
            }
            _ => {}
        }
    }

    if let sqlparser::ast::GroupByExpr::Expressions(exprs, _) = &select.group_by {
        for expr in exprs {
            validate_supported_expr(expr)?;
        }
    }

    for join in &select.from {
        for joined in &join.joins {
            if let JoinOperator::Inner(JoinConstraint::On(expr))
            | JoinOperator::LeftOuter(JoinConstraint::On(expr)) = &joined.join_operator
            {
                validate_supported_expr(expr)?;
            }
        }
    }

    Ok(())
}

fn validate_query_surface_boundaries(
    select: &sqlparser::ast::Select,
    query: &sqlparser::ast::Query,
) -> PgWireResult<()> {
    if query.with.is_some() {
        return Err(user_error(
            "WITH / CTE queries are not supported yet".to_string(),
        ));
    }

    if let Some(selection) = select.selection.as_ref() {
        validate_no_subqueries(selection, "subqueries in SELECT filters")?;
    }
    if let Some(having) = select.having.as_ref() {
        validate_no_subqueries(having, "subqueries in HAVING")?;
    }
    if let Some(order_by) = query.order_by.as_ref() {
        for expr in &order_by.exprs {
            validate_no_subqueries(&expr.expr, "subqueries in ORDER BY")?;
        }
    }
    for item in &select.projection {
        match item {
            SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
                validate_no_subqueries(expr, "subqueries in projection expressions")?;
            }
            _ => {}
        }
    }
    for table in &select.from {
        if !matches!(table.relation, sqlparser::ast::TableFactor::Table { .. }) {
            return Err(user_error(
                "derived tables in FROM are not supported yet".to_string(),
            ));
        }
        for join in &table.joins {
            if !matches!(join.relation, sqlparser::ast::TableFactor::Table { .. }) {
                return Err(user_error(
                    "derived tables in JOIN clauses are not supported yet".to_string(),
                ));
            }
        }
    }

    Ok(())
}

fn validate_supported_expr(expr: &Expr) -> PgWireResult<()> {
    match expr {
        Expr::BinaryOp { left, right, .. } => {
            validate_supported_expr(left)?;
            validate_supported_expr(right)?;
        }
        Expr::UnaryOp { expr, .. }
        | Expr::Nested(expr)
        | Expr::IsNull(expr)
        | Expr::IsNotNull(expr) => validate_supported_expr(expr)?,
        Expr::Between {
            expr, low, high, ..
        } => {
            validate_supported_expr(expr)?;
            validate_supported_expr(low)?;
            validate_supported_expr(high)?;
        }
        Expr::InList { expr, list, .. } => {
            validate_supported_expr(expr)?;
            for item in list {
                validate_supported_expr(item)?;
            }
        }
        Expr::Like { expr, pattern, .. } => {
            validate_supported_expr(expr)?;
            validate_supported_expr(pattern)?;
        }
        Expr::Function(f) => validate_supported_function(f)?,
        Expr::Cast {
            expr, data_type, ..
        } => {
            validate_supported_expr(expr)?;
            if !TypedValue::supports_cast_type(data_type) {
                return Err(user_error(format!(
                    "CAST to {} is not supported",
                    data_type
                )));
            }
        }
        Expr::Substring {
            expr,
            substring_from,
            substring_for,
            ..
        } => {
            validate_supported_expr(expr)?;
            if let Some(from) = substring_from.as_deref() {
                validate_supported_expr(from)?;
            }
            if let Some(for_expr) = substring_for.as_deref() {
                validate_supported_expr(for_expr)?;
            }
        }
        Expr::Trim {
            expr,
            trim_what,
            trim_characters,
            ..
        } => {
            validate_supported_expr(expr)?;
            if let Some(trim_what) = trim_what.as_deref() {
                validate_supported_expr(trim_what)?;
            }
            if let Some(chars) = trim_characters.as_deref() {
                for char_expr in chars {
                    validate_supported_expr(char_expr)?;
                }
            }
        }
        Expr::Case {
            operand,
            conditions,
            results,
            else_result,
            ..
        } => {
            if let Some(operand) = operand.as_deref() {
                validate_supported_expr(operand)?;
            }
            for condition in conditions {
                validate_supported_expr(condition)?;
            }
            for result in results {
                validate_supported_expr(result)?;
            }
            if let Some(else_result) = else_result.as_deref() {
                validate_supported_expr(else_result)?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn validate_supported_function(function: &Function) -> PgWireResult<()> {
    let name = function.name.to_string().to_uppercase();
    if function.to_string().to_uppercase().contains(" OVER ") {
        return Err(user_error(
            "window functions are not supported yet".to_string(),
        ));
    }
    let arg_exprs = function_exprs(function)?;

    for expr in &arg_exprs {
        validate_supported_expr(expr)?;
    }

    match name.as_str() {
        "COUNT" => {}
        "SUM" | "AVG" | "MIN" | "MAX" => {
            if arg_exprs.len() != 1 {
                return Err(user_error(format!(
                    "{} requires exactly one argument",
                    name
                )));
            }
        }
        "COALESCE" => {
            if arg_exprs.is_empty() {
                return Err(user_error(
                    "COALESCE requires at least one argument".to_string(),
                ));
            }
        }
        "NULLIF" => {
            if arg_exprs.len() != 2 {
                return Err(user_error(
                    "NULLIF requires exactly two arguments".to_string(),
                ));
            }
        }
        "LOWER" | "UPPER" | "LENGTH" => {
            if arg_exprs.len() != 1 {
                return Err(user_error(format!(
                    "{} requires exactly one argument",
                    name
                )));
            }
        }
        "CONCAT" => {
            if arg_exprs.is_empty() {
                return Err(user_error(
                    "CONCAT requires at least one argument".to_string(),
                ));
            }
        }
        "NOW" | "CURRENT_DATE" | "CURRENT_TIME" | "CURRENT_TIMESTAMP" | "LOCALTIME"
        | "LOCALTIMESTAMP" => {
            if !arg_exprs.is_empty() {
                return Err(user_error(format!("{} does not accept arguments", name)));
            }
        }
        other if !is_aggregate_function_name(other) => {
            return Err(user_error(format!("function '{}' is not supported", other)));
        }
        _ => {}
    }

    Ok(())
}

fn function_exprs(function: &Function) -> PgWireResult<Vec<&Expr>> {
    match &function.args {
        FunctionArguments::None => Ok(Vec::new()),
        FunctionArguments::List(list) => {
            let mut exprs = Vec::with_capacity(list.args.len());
            for arg in &list.args {
                match arg {
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(expr)) => exprs.push(expr),
                    FunctionArg::Unnamed(FunctionArgExpr::Wildcard) => {}
                    _ => {
                        return Err(user_error(format!(
                            "function '{}' uses unsupported argument form",
                            function.name
                        )))
                    }
                }
            }
            Ok(exprs)
        }
        FunctionArguments::Subquery(_) => Err(user_error(format!(
            "function '{}' does not support subquery arguments",
            function.name
        ))),
    }
}

/// Resolve projection items to (display_name, Expr) pairs for expression evaluation.
fn resolve_projection_exprs(
    items: &[SelectItem],
    schema_cols: &[ColumnInfo],
) -> PgWireResult<Vec<(String, Expr)>> {
    let mut result = Vec::new();
    for item in items {
        match item {
            SelectItem::Wildcard(_) => {
                for col in schema_cols {
                    result.push((
                        col.name.clone(),
                        Expr::Identifier(sqlparser::ast::Ident::new(&col.name)),
                    ));
                }
            }
            SelectItem::UnnamedExpr(expr) => {
                result.push((expr.to_string(), expr.clone()));
            }
            SelectItem::ExprWithAlias { expr, alias } => {
                result.push((alias.value.clone(), expr.clone()));
            }
            _ => {}
        }
    }
    Ok(result)
}

/// Extract aggregate function calls from the projection.
fn extract_aggregates(items: &[SelectItem]) -> Vec<&sqlparser::ast::Function> {
    let mut aggs = Vec::new();
    for item in items {
        match item {
            SelectItem::UnnamedExpr(Expr::Function(f)) => {
                if is_aggregate_function_name(&f.name.to_string()) {
                    aggs.push(f);
                }
            }
            SelectItem::ExprWithAlias {
                expr: Expr::Function(f),
                ..
            } => {
                if is_aggregate_function_name(&f.name.to_string()) {
                    aggs.push(f);
                }
            }
            _ => {}
        }
    }
    aggs
}

/// Extract GROUP BY column names.
fn extract_group_by_cols(group_by: &sqlparser::ast::GroupByExpr) -> Vec<String> {
    match group_by {
        sqlparser::ast::GroupByExpr::Expressions(exprs, _) => exprs
            .iter()
            .filter_map(|e| match e {
                Expr::Identifier(ident) => Some(ident.value.to_lowercase()),
                Expr::CompoundIdentifier(parts) => Some(
                    parts
                        .iter()
                        .map(|p| p.value.to_lowercase())
                        .collect::<Vec<_>>()
                        .join("."),
                ),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Compute aggregate values for a set of rows.
fn compute_aggregates(
    projection: &[SelectItem],
    rows: &[Row],
    schema_cols: &[ColumnInfo],
) -> PgWireResult<Vec<(String, TypedValue)>> {
    let mut result = Vec::new();

    for item in projection {
        match item {
            SelectItem::UnnamedExpr(Expr::Identifier(ident)) => {
                // Non-aggregate column in GROUP BY result: take from first row
                let val = rows
                    .first()
                    .map(|r| lookup_row_value(r, &ident.value.to_lowercase()))
                    .unwrap_or(TypedValue::Null);
                result.push((ident.value.clone(), val));
            }
            SelectItem::UnnamedExpr(Expr::CompoundIdentifier(parts)) => {
                let full = parts
                    .iter()
                    .map(|p| p.value.to_lowercase())
                    .collect::<Vec<_>>()
                    .join(".");
                let display = parts.last().map(|p| p.value.clone()).unwrap_or_default();
                let val = rows
                    .first()
                    .map(|r| lookup_row_value(r, &full))
                    .unwrap_or(TypedValue::Null);
                result.push((display, val));
            }
            SelectItem::UnnamedExpr(Expr::Function(f))
            | SelectItem::ExprWithAlias {
                expr: Expr::Function(f),
                ..
            } => {
                let name = f.name.to_string().to_uppercase();
                let alias = match item {
                    SelectItem::ExprWithAlias { alias, .. } => alias.value.clone(),
                    _ => f.to_string(),
                };

                let arg_expr = extract_function_arg_expr(f);
                let val =
                    compute_single_aggregate_expr(&name, arg_expr.as_ref(), rows, schema_cols)?;
                result.push((alias, val));
            }
            _ => {}
        }
    }

    Ok(result)
}

/// Extract the full expression argument from an aggregate function call.
/// Returns None for wildcard (COUNT(*)), Some(Expr) for column or expression args.
fn extract_function_arg_expr(f: &sqlparser::ast::Function) -> Option<Expr> {
    match &f.args {
        sqlparser::ast::FunctionArguments::List(list) => {
            if list.args.is_empty() {
                return None;
            }
            match &list.args[0] {
                FunctionArg::Unnamed(FunctionArgExpr::Wildcard) => None,
                FunctionArg::Unnamed(FunctionArgExpr::Expr(expr)) => Some(expr.clone()),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Evaluate a row's value for an aggregate argument expression.
fn eval_agg_arg(expr: &Expr, row: &Row, schema_cols: &[ColumnInfo]) -> TypedValue {
    resolve_value(expr, row, schema_cols)
}

/// Compute a single aggregate function over rows using expression evaluation.
fn compute_single_aggregate_expr(
    func_name: &str,
    arg_expr: Option<&Expr>,
    rows: &[Row],
    schema_cols: &[ColumnInfo],
) -> PgWireResult<TypedValue> {
    match func_name {
        "COUNT" => {
            if let Some(expr) = arg_expr {
                let count = rows
                    .iter()
                    .filter(|r| !eval_agg_arg(expr, r, schema_cols).is_null())
                    .count();
                Ok(TypedValue::Int(count as i64))
            } else {
                Ok(TypedValue::Int(rows.len() as i64))
            }
        }
        "SUM" => {
            let expr =
                arg_expr.ok_or_else(|| user_error("SUM requires an argument".to_string()))?;
            if rows.is_empty() {
                return Ok(TypedValue::Null);
            }
            let mut sum = 0.0f64;
            let mut has_value = false;
            for row in rows {
                let val = eval_agg_arg(expr, row, schema_cols);
                if let Some(n) = val.as_f64() {
                    sum += n;
                    has_value = true;
                }
            }
            if has_value {
                if sum == (sum as i64) as f64 {
                    Ok(TypedValue::Int(sum as i64))
                } else {
                    Ok(TypedValue::Float(sum))
                }
            } else {
                Ok(TypedValue::Null)
            }
        }
        "AVG" => {
            let expr =
                arg_expr.ok_or_else(|| user_error("AVG requires an argument".to_string()))?;
            if rows.is_empty() {
                return Ok(TypedValue::Null);
            }
            let mut sum = 0.0f64;
            let mut count = 0u64;
            for row in rows {
                let val = eval_agg_arg(expr, row, schema_cols);
                if let Some(n) = val.as_f64() {
                    sum += n;
                    count += 1;
                }
            }
            if count > 0 {
                Ok(TypedValue::Float(sum / count as f64))
            } else {
                Ok(TypedValue::Null)
            }
        }
        "MIN" => {
            let expr =
                arg_expr.ok_or_else(|| user_error("MIN requires an argument".to_string()))?;
            let mut min: Option<TypedValue> = None;
            for row in rows {
                let val = eval_agg_arg(expr, row, schema_cols);
                if val.is_null() {
                    continue;
                }
                min = Some(match min {
                    None => val,
                    Some(ref current) if val.sql_lt(current) => val,
                    Some(current) => current,
                });
            }
            Ok(min.unwrap_or(TypedValue::Null))
        }
        "MAX" => {
            let expr =
                arg_expr.ok_or_else(|| user_error("MAX requires an argument".to_string()))?;
            let mut max: Option<TypedValue> = None;
            for row in rows {
                let val = eval_agg_arg(expr, row, schema_cols);
                if val.is_null() {
                    continue;
                }
                max = Some(match max {
                    None => val,
                    Some(ref current) if val.sql_gt(current) => val,
                    Some(current) => current,
                });
            }
            Ok(max.unwrap_or(TypedValue::Null))
        }
        _ => Err(user_error(format!("unsupported aggregate: {}", func_name))),
    }
}

/// Sort rows by ORDER BY expressions.
fn sort_rows(
    rows: &mut [Row],
    order_by: &[OrderByExpr],
    schema_cols: &[ColumnInfo],
) -> PgWireResult<()> {
    rows.sort_by(|a, b| {
        for ob in order_by {
            let a_val = resolve_value(&ob.expr, a, schema_cols);
            let b_val = resolve_value(&ob.expr, b, schema_cols);

            let cmp = a_val.cmp_order(&b_val);
            let cmp = if ob.asc == Some(false) {
                cmp.reverse()
            } else {
                cmp
            };
            if cmp != std::cmp::Ordering::Equal {
                return cmp;
            }
        }
        std::cmp::Ordering::Equal
    });
    Ok(())
}

/// Lookup a value in a row, handling both plain and qualified (alias.col) keys.
fn lookup_row_value(row: &Row, col_name: &str) -> TypedValue {
    row.get(col_name)
        .or_else(|| {
            // Aggregate/grouped rows often store the projected display name only
            // (for example `name`) while ORDER BY may still reference the original
            // qualified path (for example `c.name`). Fall back to the last segment
            // before trying the fuzzy suffix match used for joined rows.
            col_name
                .rsplit('.')
                .next()
                .filter(|last| *last != col_name)
                .and_then(|last| row.get(last))
        })
        .or_else(|| {
            // Try fuzzy match: find key ending with .col_name
            row.iter()
                .find(|(k, _)| k.ends_with(&format!(".{}", col_name)))
                .map(|(_, v)| v)
        })
        .cloned()
        .unwrap_or(TypedValue::Null)
}

fn object_name_to_string(name: &ObjectName) -> String {
    name.0
        .iter()
        .map(|part| part.to_string())
        .collect::<Vec<_>>()
        .join(".")
}

fn object_name_last(name: &ObjectName) -> String {
    name.0
        .last()
        .map(|part| part.to_string())
        .unwrap_or_else(|| name.to_string())
}

fn normalize_join_key(name: &str) -> String {
    name.to_lowercase()
}

fn rename_relation_columns(
    mut relation: RelationResult,
    names: &[String],
) -> PgWireResult<RelationResult> {
    if relation.columns.len() != names.len() {
        return Err(user_error(format!(
            "column alias count ({}) does not match relation column count ({})",
            names.len(),
            relation.columns.len()
        )));
    }
    let original_columns = relation.columns.clone();
    let mut rows = Vec::with_capacity(relation.rows.len());
    for row in &relation.rows {
        let mut renamed = Row::new();
        for (column, name) in original_columns.iter().zip(names.iter()) {
            renamed.insert(
                name.to_lowercase(),
                lookup_row_value(row, &column.lookup_name),
            );
        }
        rows.push(renamed);
    }
    relation.columns = names
        .iter()
        .map(|name| RelationColumn {
            lookup_name: name.to_lowercase(),
            output_name: name.clone(),
        })
        .collect();
    relation.rows = rows;
    Ok(relation)
}

fn prefix_relation_as_source(relation: RelationResult, source_label: &str) -> RelationResult {
    let prefix = source_label.to_lowercase();
    let columns = relation
        .columns
        .iter()
        .map(|column| RelationColumn {
            lookup_name: format!("{}.{}", prefix, column.output_name.to_lowercase()),
            output_name: column.output_name.clone(),
        })
        .collect::<Vec<_>>();

    let rows = relation
        .rows
        .iter()
        .map(|row| {
            let mut output = Row::new();
            for column in &relation.columns {
                output.insert(
                    format!("{}.{}", prefix, column.output_name.to_lowercase()),
                    lookup_row_value(row, &column.lookup_name),
                );
            }
            output
        })
        .collect();

    RelationResult { rows, columns }
}

fn qualify_relation_for_join_wildcard(relation: &mut RelationResult) {
    for column in &mut relation.columns {
        column.output_name = column.lookup_name.clone();
    }
}

fn normalize_relation_for_set_op(relation: RelationResult) -> RelationResult {
    let field_names = relation.field_names();
    let rows = relation
        .rows
        .iter()
        .map(|row| {
            let mut normalized = Row::new();
            for name in &field_names {
                normalized.insert(
                    name.to_lowercase(),
                    lookup_row_value(row, &name.to_lowercase()),
                );
            }
            normalized
        })
        .collect::<Vec<_>>();
    RelationResult::from_named_rows(rows, field_names)
}

fn deduplicate_relation_rows(relation: RelationResult) -> RelationResult {
    let field_names = relation.field_names();
    let mut seen = HashSet::new();
    let rows = relation
        .rows
        .into_iter()
        .filter(|row| {
            let key = field_names
                .iter()
                .map(|name| format!("{:?}", lookup_row_value(row, &name.to_lowercase())))
                .collect::<Vec<_>>()
                .join("|");
            seen.insert(key)
        })
        .collect();
    RelationResult {
        rows,
        columns: relation.columns,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueClass {
    Integer,
    Float,
    Text,
    Boolean,
}

impl ValueClass {
    fn label(self) -> &'static str {
        match self {
            Self::Integer => "INT",
            Self::Float => "FLOAT",
            Self::Text => "TEXT",
            Self::Boolean => "BOOLEAN",
        }
    }
}

fn typed_value_class(value: &TypedValue) -> Option<ValueClass> {
    match value {
        TypedValue::Null => None,
        TypedValue::Int(_) => Some(ValueClass::Integer),
        TypedValue::Float(_) => Some(ValueClass::Float),
        TypedValue::Text(_) => Some(ValueClass::Text),
        TypedValue::Boolean(_) => Some(ValueClass::Boolean),
    }
}

fn classes_are_union_compatible(left: ValueClass, right: ValueClass) -> bool {
    left == right
        || matches!(
            (left, right),
            (ValueClass::Integer, ValueClass::Float) | (ValueClass::Float, ValueClass::Integer)
        )
}

fn infer_relation_column_class(
    relation: &RelationResult,
    column: &RelationColumn,
) -> Option<ValueClass> {
    relation
        .rows
        .iter()
        .find_map(|row| typed_value_class(&lookup_row_value(row, &column.lookup_name)))
}

fn validate_set_operation_column_compatibility(
    op: &sqlparser::ast::SetOperator,
    left: &RelationResult,
    right: &RelationResult,
) -> PgWireResult<()> {
    for (idx, (left_col, right_col)) in left.columns.iter().zip(&right.columns).enumerate() {
        let left_class = infer_relation_column_class(left, left_col);
        let right_class = infer_relation_column_class(right, right_col);
        if let (Some(left_class), Some(right_class)) = (left_class, right_class) {
            if !classes_are_union_compatible(left_class, right_class) {
                return Err(user_error(format!(
                    "{} requires compatible column types at position {} (left={}, right={})",
                    op,
                    idx + 1,
                    left_class.label(),
                    right_class.label()
                )));
            }
        }
    }
    Ok(())
}

fn is_union_all_quantifier(set_quantifier: &sqlparser::ast::SetQuantifier) -> bool {
    matches!(set_quantifier, sqlparser::ast::SetQuantifier::All)
}

fn merge_scope_rows(outer_rows: &[Row], current_row: &Row) -> Row {
    let mut merged = Row::new();
    for row in outer_rows {
        merged.extend(row.clone());
    }
    merged.extend(current_row.clone());
    merged
}

fn query_can_use_optimized_relation_fast_path(query: &sqlparser::ast::Query) -> bool {
    if query.with.is_some() {
        return false;
    }

    match query.body.as_ref() {
        SetExpr::Select(select) => select_can_use_optimized_relation_fast_path(select, query),
        SetExpr::Query(_) | SetExpr::SetOperation { .. } => false,
        _ => false,
    }
}

fn select_can_use_optimized_relation_fast_path(
    select: &sqlparser::ast::Select,
    query: &sqlparser::ast::Query,
) -> bool {
    if select.from.iter().any(|twj| {
        !matches!(twj.relation, sqlparser::ast::TableFactor::Table { .. })
            || twj
                .joins
                .iter()
                .any(|join| !matches!(join.relation, sqlparser::ast::TableFactor::Table { .. }))
    }) {
        return false;
    }

    if let Some(selection) = select.selection.as_ref() {
        if contains_subquery_expr(selection) {
            return false;
        }
    }
    if let Some(having) = select.having.as_ref() {
        if contains_subquery_expr(having) {
            return false;
        }
    }
    if let Some(order_by) = query.order_by.as_ref() {
        if order_by
            .exprs
            .iter()
            .any(|expr| contains_subquery_expr(&expr.expr))
        {
            return false;
        }
    }
    !select.projection.iter().any(|item| match item {
        SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
            contains_subquery_expr(expr)
        }
        _ => false,
    })
}

fn contains_subquery_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Subquery(_) | Expr::InSubquery { .. } | Expr::Exists { .. } => true,
        Expr::BinaryOp { left, right, .. } => {
            contains_subquery_expr(left) || contains_subquery_expr(right)
        }
        Expr::UnaryOp { expr, .. }
        | Expr::Nested(expr)
        | Expr::IsNull(expr)
        | Expr::IsNotNull(expr) => contains_subquery_expr(expr),
        Expr::Between {
            expr, low, high, ..
        } => {
            contains_subquery_expr(expr)
                || contains_subquery_expr(low)
                || contains_subquery_expr(high)
        }
        Expr::InList { expr, list, .. } => {
            contains_subquery_expr(expr) || list.iter().any(contains_subquery_expr)
        }
        Expr::Like { expr, pattern, .. } => {
            contains_subquery_expr(expr) || contains_subquery_expr(pattern)
        }
        Expr::Function(function) => function_exprs(function)
            .map(|exprs| exprs.into_iter().any(contains_subquery_expr))
            .unwrap_or(false),
        Expr::Cast { expr, .. } => contains_subquery_expr(expr),
        Expr::Substring {
            expr,
            substring_from,
            substring_for,
            ..
        } => {
            contains_subquery_expr(expr)
                || substring_from
                    .as_deref()
                    .map(contains_subquery_expr)
                    .unwrap_or(false)
                || substring_for
                    .as_deref()
                    .map(contains_subquery_expr)
                    .unwrap_or(false)
        }
        Expr::Trim {
            expr,
            trim_what,
            trim_characters,
            ..
        } => {
            contains_subquery_expr(expr)
                || trim_what
                    .as_deref()
                    .map(contains_subquery_expr)
                    .unwrap_or(false)
                || trim_characters
                    .as_deref()
                    .map(|chars| chars.iter().any(contains_subquery_expr))
                    .unwrap_or(false)
        }
        Expr::Case {
            operand,
            conditions,
            results,
            else_result,
            ..
        } => {
            operand
                .as_deref()
                .map(contains_subquery_expr)
                .unwrap_or(false)
                || conditions.iter().any(contains_subquery_expr)
                || results.iter().any(contains_subquery_expr)
                || else_result
                    .as_deref()
                    .map(contains_subquery_expr)
                    .unwrap_or(false)
        }
        _ => false,
    }
}

fn apply_arithmetic_values(
    left: &TypedValue,
    op: &sqlparser::ast::BinaryOperator,
    right: &TypedValue,
) -> TypedValue {
    if left.is_null() || right.is_null() {
        return TypedValue::Null;
    }
    let Some(left_num) = left.as_f64() else {
        return TypedValue::Null;
    };
    let Some(right_num) = right.as_f64() else {
        return TypedValue::Null;
    };
    let result = match op {
        sqlparser::ast::BinaryOperator::Plus => left_num + right_num,
        sqlparser::ast::BinaryOperator::Minus => left_num - right_num,
        sqlparser::ast::BinaryOperator::Multiply => left_num * right_num,
        sqlparser::ast::BinaryOperator::Divide => {
            if right_num == 0.0 {
                return TypedValue::Null;
            }
            left_num / right_num
        }
        sqlparser::ast::BinaryOperator::Modulo => {
            if right_num == 0.0 {
                return TypedValue::Null;
            }
            left_num % right_num
        }
        _ => return TypedValue::Null,
    };
    if matches!(left, TypedValue::Int(_))
        && matches!(right, TypedValue::Int(_))
        && result == (result as i64) as f64
        && !matches!(op, sqlparser::ast::BinaryOperator::Divide)
    {
        TypedValue::Int(result as i64)
    } else {
        TypedValue::Float(result)
    }
}

fn compare_typed_values(
    left: &TypedValue,
    op: &sqlparser::ast::BinaryOperator,
    right: &TypedValue,
) -> bool {
    match op {
        sqlparser::ast::BinaryOperator::Eq => left.sql_eq(right),
        sqlparser::ast::BinaryOperator::NotEq => {
            !left.sql_eq(right) && !left.is_null() && !right.is_null()
        }
        sqlparser::ast::BinaryOperator::Lt => left.sql_lt(right),
        sqlparser::ast::BinaryOperator::LtEq => left.sql_lt(right) || left.sql_eq(right),
        sqlparser::ast::BinaryOperator::Gt => left.sql_gt(right),
        sqlparser::ast::BinaryOperator::GtEq => left.sql_gt(right) || left.sql_eq(right),
        _ => false,
    }
}

fn expand_projection_columns(
    items: &[SelectItem],
    relation: &RelationResult,
    single_source_wildcard: bool,
) -> PgWireResult<Vec<RelationColumn>> {
    let mut columns = Vec::new();
    for item in items {
        match item {
            SelectItem::Wildcard(_) => {
                for column in &relation.columns {
                    columns.push(RelationColumn {
                        lookup_name: if single_source_wildcard {
                            column.output_name.to_lowercase()
                        } else {
                            column.lookup_name.clone()
                        },
                        output_name: if single_source_wildcard {
                            column.output_name.clone()
                        } else {
                            column.lookup_name.clone()
                        },
                    });
                }
            }
            SelectItem::UnnamedExpr(Expr::Identifier(ident)) => columns.push(RelationColumn {
                lookup_name: ident.value.to_lowercase(),
                output_name: ident.value.clone(),
            }),
            SelectItem::UnnamedExpr(Expr::CompoundIdentifier(parts)) => {
                let lookup_name = parts
                    .iter()
                    .map(|part| part.value.to_lowercase())
                    .collect::<Vec<_>>()
                    .join(".");
                let output_name = parts
                    .last()
                    .map(|part| part.value.clone())
                    .unwrap_or_default();
                columns.push(RelationColumn {
                    lookup_name,
                    output_name,
                });
            }
            SelectItem::UnnamedExpr(expr) => columns.push(RelationColumn {
                lookup_name: expr.to_string().to_lowercase(),
                output_name: expr.to_string(),
            }),
            SelectItem::ExprWithAlias { alias, .. } => columns.push(RelationColumn {
                lookup_name: alias.value.to_lowercase(),
                output_name: alias.value.clone(),
            }),
            SelectItem::QualifiedWildcard(kind, _) => {
                let prefix = object_name_to_string(kind).to_lowercase();
                let matching = relation
                    .columns
                    .iter()
                    .filter(|column| column.lookup_name.starts_with(&format!("{prefix}.")))
                    .cloned()
                    .collect::<Vec<_>>();
                if matching.is_empty() {
                    return Err(user_error(format!(
                        "qualified wildcard '{}.*' did not match any columns",
                        kind
                    )));
                }
                columns.extend(matching);
            }
        }
    }
    Ok(columns)
}

fn materialize_named_projection(
    rows: &[Row],
    projected_cols: &[ColumnInfo],
) -> (Vec<Row>, Vec<String>) {
    let field_names: Vec<String> = projected_cols.iter().map(|col| col.name.clone()).collect();
    let output_rows = rows
        .iter()
        .map(|row| {
            let mut output = Row::new();
            for col in projected_cols {
                output.insert(
                    col.name.to_lowercase(),
                    lookup_row_value(row, &col.name.to_lowercase()),
                );
            }
            output
        })
        .collect();
    (output_rows, field_names)
}

fn materialize_expression_projection(
    rows: &[Row],
    projected_exprs: &[(String, Expr)],
    schema_cols: &[ColumnInfo],
) -> (Vec<Row>, Vec<String>) {
    let field_names: Vec<String> = projected_exprs
        .iter()
        .map(|(name, _)| name.clone())
        .collect();
    let output_rows = rows
        .iter()
        .map(|row| {
            let mut output = Row::new();
            for (name, expr) in projected_exprs {
                output.insert(name.to_lowercase(), resolve_value(expr, row, schema_cols));
            }
            output
        })
        .collect();
    (output_rows, field_names)
}

fn validate_no_subqueries(expr: &Expr, context: &str) -> PgWireResult<()> {
    match expr {
        Expr::Subquery(_) => Err(user_error(format!("{} are not supported yet", context))),
        Expr::InSubquery { .. } => Err(user_error(format!("{} are not supported yet", context))),
        Expr::Exists { .. } => Err(user_error(format!("{} are not supported yet", context))),
        Expr::BinaryOp { left, right, .. } => {
            validate_no_subqueries(left, context)?;
            validate_no_subqueries(right, context)
        }
        Expr::UnaryOp { expr, .. }
        | Expr::Nested(expr)
        | Expr::IsNull(expr)
        | Expr::IsNotNull(expr) => validate_no_subqueries(expr, context),
        Expr::Between {
            expr, low, high, ..
        } => {
            validate_no_subqueries(expr, context)?;
            validate_no_subqueries(low, context)?;
            validate_no_subqueries(high, context)
        }
        Expr::InList { expr, list, .. } => {
            validate_no_subqueries(expr, context)?;
            for item in list {
                validate_no_subqueries(item, context)?;
            }
            Ok(())
        }
        Expr::Like { expr, pattern, .. } => {
            validate_no_subqueries(expr, context)?;
            validate_no_subqueries(pattern, context)
        }
        Expr::Function(function) => {
            for arg in function_exprs(function)? {
                validate_no_subqueries(arg, context)?;
            }
            Ok(())
        }
        Expr::Cast { expr, .. } => validate_no_subqueries(expr, context),
        Expr::Substring {
            expr,
            substring_from,
            substring_for,
            ..
        } => {
            validate_no_subqueries(expr, context)?;
            if let Some(from) = substring_from.as_deref() {
                validate_no_subqueries(from, context)?;
            }
            if let Some(for_expr) = substring_for.as_deref() {
                validate_no_subqueries(for_expr, context)?;
            }
            Ok(())
        }
        Expr::Trim {
            expr,
            trim_what,
            trim_characters,
            ..
        } => {
            validate_no_subqueries(expr, context)?;
            if let Some(trim_what) = trim_what.as_deref() {
                validate_no_subqueries(trim_what, context)?;
            }
            if let Some(chars) = trim_characters.as_deref() {
                for char_expr in chars {
                    validate_no_subqueries(char_expr, context)?;
                }
            }
            Ok(())
        }
        Expr::Case {
            operand,
            conditions,
            results,
            else_result,
            ..
        } => {
            if let Some(operand) = operand.as_deref() {
                validate_no_subqueries(operand, context)?;
            }
            for condition in conditions {
                validate_no_subqueries(condition, context)?;
            }
            for result in results {
                validate_no_subqueries(result, context)?;
            }
            if let Some(else_result) = else_result.as_deref() {
                validate_no_subqueries(else_result, context)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn evaluate_grouped_predicate(
    expr: &Expr,
    rows: &[Row],
    schema_cols: &[ColumnInfo],
) -> PgWireResult<bool> {
    match expr {
        Expr::BinaryOp { left, op, right } => match op {
            sqlparser::ast::BinaryOperator::And => {
                Ok(evaluate_grouped_predicate(left, rows, schema_cols)?
                    && evaluate_grouped_predicate(right, rows, schema_cols)?)
            }
            sqlparser::ast::BinaryOperator::Or => {
                Ok(evaluate_grouped_predicate(left, rows, schema_cols)?
                    || evaluate_grouped_predicate(right, rows, schema_cols)?)
            }
            sqlparser::ast::BinaryOperator::Eq => Ok(eval_grouped_value(left, rows, schema_cols)?
                .sql_eq(&eval_grouped_value(right, rows, schema_cols)?)),
            sqlparser::ast::BinaryOperator::NotEq => {
                let left_val = eval_grouped_value(left, rows, schema_cols)?;
                let right_val = eval_grouped_value(right, rows, schema_cols)?;
                Ok(!left_val.sql_eq(&right_val) && !left_val.is_null() && !right_val.is_null())
            }
            sqlparser::ast::BinaryOperator::Lt => Ok(eval_grouped_value(left, rows, schema_cols)?
                .sql_lt(&eval_grouped_value(right, rows, schema_cols)?)),
            sqlparser::ast::BinaryOperator::LtEq => {
                let left_val = eval_grouped_value(left, rows, schema_cols)?;
                let right_val = eval_grouped_value(right, rows, schema_cols)?;
                Ok(left_val.sql_lt(&right_val) || left_val.sql_eq(&right_val))
            }
            sqlparser::ast::BinaryOperator::Gt => Ok(eval_grouped_value(left, rows, schema_cols)?
                .sql_gt(&eval_grouped_value(right, rows, schema_cols)?)),
            sqlparser::ast::BinaryOperator::GtEq => {
                let left_val = eval_grouped_value(left, rows, schema_cols)?;
                let right_val = eval_grouped_value(right, rows, schema_cols)?;
                Ok(left_val.sql_gt(&right_val) || left_val.sql_eq(&right_val))
            }
            _ => Ok(false),
        },
        Expr::UnaryOp {
            op: sqlparser::ast::UnaryOperator::Not,
            expr,
        } => Ok(!evaluate_grouped_predicate(expr, rows, schema_cols)?),
        Expr::IsNull(inner) => Ok(eval_grouped_value(inner, rows, schema_cols)?.is_null()),
        Expr::IsNotNull(inner) => Ok(!eval_grouped_value(inner, rows, schema_cols)?.is_null()),
        Expr::Between {
            expr: inner,
            low,
            high,
            negated,
        } => {
            let val = eval_grouped_value(inner, rows, schema_cols)?;
            let lo = eval_grouped_value(low, rows, schema_cols)?;
            let hi = eval_grouped_value(high, rows, schema_cols)?;
            let in_range =
                (val.sql_eq(&lo) || val.sql_gt(&lo)) && (val.sql_eq(&hi) || val.sql_lt(&hi));
            Ok(if *negated { !in_range } else { in_range })
        }
        Expr::InList {
            expr: inner,
            list,
            negated,
        } => {
            let value = eval_grouped_value(inner, rows, schema_cols)?;
            let found = list.iter().any(|item| {
                eval_grouped_value(item, rows, schema_cols)
                    .map(|candidate| value.sql_eq(&candidate))
                    .unwrap_or(false)
            });
            Ok(if *negated { !found } else { found })
        }
        Expr::Nested(inner) => evaluate_grouped_predicate(inner, rows, schema_cols),
        _ => Ok(false),
    }
}

fn eval_grouped_value(
    expr: &Expr,
    rows: &[Row],
    schema_cols: &[ColumnInfo],
) -> PgWireResult<TypedValue> {
    match expr {
        Expr::Function(function) if is_aggregate_function_name(&function.name.to_string()) => {
            let name = function.name.to_string().to_uppercase();
            let arg_expr = extract_function_arg_expr(function);
            compute_single_aggregate_expr(&name, arg_expr.as_ref(), rows, schema_cols)
        }
        _ => Ok(rows
            .first()
            .map(|row| resolve_value(expr, row, schema_cols))
            .unwrap_or(TypedValue::Null)),
    }
}

#[derive(Clone)]
struct ColumnarAggregatePlan {
    items: Vec<ColumnarProjectionItem>,
    state_specs: Vec<AggregateSpec>,
    field_names: Vec<String>,
    group_exprs: Vec<Expr>,
}

#[derive(Clone)]
enum ColumnarProjectionItem {
    Scalar {
        output_name: String,
        expr: Expr,
    },
    Aggregate {
        output_name: String,
        state_index: usize,
    },
}

#[derive(Clone)]
struct AggregateSpec {
    func_name: String,
    arg_expr: Option<Expr>,
}

#[derive(Clone)]
enum AggregateState {
    Count(i64),
    Sum { sum: f64, has_value: bool },
    Avg { sum: f64, count: u64 },
    Min(Option<TypedValue>),
    Max(Option<TypedValue>),
}

struct AggregateBucket {
    first_row_idx: Option<usize>,
    states: Vec<AggregateState>,
}

fn build_columnar_aggregate_plan(
    projection: &[SelectItem],
    group_by: &sqlparser::ast::GroupByExpr,
) -> Option<ColumnarAggregatePlan> {
    let group_exprs = match group_by {
        sqlparser::ast::GroupByExpr::Expressions(exprs, _) => exprs.clone(),
        _ => return None,
    };
    if !group_exprs.iter().all(is_supported_columnar_expr) {
        return None;
    }

    let mut items = Vec::new();
    let mut state_specs = Vec::new();
    let mut field_names = Vec::new();

    for item in projection {
        match item {
            SelectItem::UnnamedExpr(Expr::Identifier(ident)) => {
                field_names.push(ident.value.clone());
                items.push(ColumnarProjectionItem::Scalar {
                    output_name: ident.value.clone(),
                    expr: Expr::Identifier(ident.clone()),
                });
            }
            SelectItem::UnnamedExpr(Expr::CompoundIdentifier(parts)) => {
                let output_name = parts.last()?.value.clone();
                field_names.push(output_name.clone());
                items.push(ColumnarProjectionItem::Scalar {
                    output_name,
                    expr: Expr::CompoundIdentifier(parts.clone()),
                });
            }
            SelectItem::ExprWithAlias { expr, alias }
                if matches!(expr, Expr::Identifier(_) | Expr::CompoundIdentifier(_)) =>
            {
                if !is_supported_columnar_expr(expr) {
                    return None;
                }
                field_names.push(alias.value.clone());
                items.push(ColumnarProjectionItem::Scalar {
                    output_name: alias.value.clone(),
                    expr: expr.clone(),
                });
            }
            SelectItem::UnnamedExpr(Expr::Function(f))
            | SelectItem::ExprWithAlias {
                expr: Expr::Function(f),
                ..
            } => {
                let arg_expr = extract_function_arg_expr(f);
                if let Some(expr) = arg_expr.as_ref() {
                    if !is_supported_columnar_expr(expr) {
                        return None;
                    }
                }

                let output_name = match item {
                    SelectItem::ExprWithAlias { alias, .. } => alias.value.clone(),
                    _ => f.to_string(),
                };
                field_names.push(output_name.clone());

                let state_index = state_specs.len();
                state_specs.push(AggregateSpec {
                    func_name: f.name.to_string().to_uppercase(),
                    arg_expr,
                });
                items.push(ColumnarProjectionItem::Aggregate {
                    output_name,
                    state_index,
                });
            }
            _ => return None,
        }
    }

    Some(ColumnarAggregatePlan {
        items,
        state_specs,
        field_names,
        group_exprs,
    })
}

fn compute_columnar_aggregate_rows(
    plan: &ColumnarAggregatePlan,
    table_cols: &TableColumns,
    bitmap: &Bitmap,
    schema_cols: &[ColumnInfo],
) -> PgWireResult<Vec<Row>> {
    if plan.group_exprs.is_empty() {
        let mut bucket = AggregateBucket {
            first_row_idx: None,
            states: plan.state_specs.iter().map(init_aggregate_state).collect(),
        };
        for idx in bitmap.iter_set_bits() {
            if bucket.first_row_idx.is_none() {
                bucket.first_row_idx = Some(idx);
            }
            update_aggregate_states(
                &mut bucket.states,
                &plan.state_specs,
                idx,
                table_cols,
                schema_cols,
            );
        }
        return Ok(vec![aggregate_bucket_to_row(
            plan,
            &bucket,
            table_cols,
            schema_cols,
        )]);
    }

    let mut buckets: HashMap<Vec<String>, AggregateBucket> = HashMap::new();
    for idx in bitmap.iter_set_bits() {
        let key: Vec<String> = plan
            .group_exprs
            .iter()
            .map(|expr| {
                typed_value_to_group_key(&resolve_columnar_value(
                    expr,
                    idx,
                    table_cols,
                    schema_cols,
                ))
            })
            .collect();
        let bucket = buckets.entry(key).or_insert_with(|| AggregateBucket {
            first_row_idx: Some(idx),
            states: plan.state_specs.iter().map(init_aggregate_state).collect(),
        });
        update_aggregate_states(
            &mut bucket.states,
            &plan.state_specs,
            idx,
            table_cols,
            schema_cols,
        );
    }

    Ok(buckets
        .values()
        .map(|bucket| aggregate_bucket_to_row(plan, bucket, table_cols, schema_cols))
        .collect())
}

fn aggregate_bucket_to_row(
    plan: &ColumnarAggregatePlan,
    bucket: &AggregateBucket,
    table_cols: &TableColumns,
    schema_cols: &[ColumnInfo],
) -> Row {
    let mut row = Row::new();
    for item in &plan.items {
        match item {
            ColumnarProjectionItem::Scalar { output_name, expr } => {
                let val = bucket
                    .first_row_idx
                    .map(|idx| resolve_columnar_value(expr, idx, table_cols, schema_cols))
                    .unwrap_or(TypedValue::Null);
                row.insert(output_name.to_lowercase(), val);
            }
            ColumnarProjectionItem::Aggregate {
                output_name,
                state_index,
            } => {
                row.insert(
                    output_name.to_lowercase(),
                    finalize_aggregate_state(&bucket.states[*state_index]),
                );
            }
        }
    }
    row
}

fn init_aggregate_state(spec: &AggregateSpec) -> AggregateState {
    match spec.func_name.as_str() {
        "COUNT" => AggregateState::Count(0),
        "SUM" => AggregateState::Sum {
            sum: 0.0,
            has_value: false,
        },
        "AVG" => AggregateState::Avg { sum: 0.0, count: 0 },
        "MIN" => AggregateState::Min(None),
        "MAX" => AggregateState::Max(None),
        _ => AggregateState::Count(0),
    }
}

fn update_aggregate_states(
    states: &mut [AggregateState],
    specs: &[AggregateSpec],
    row_idx: usize,
    table_cols: &TableColumns,
    schema_cols: &[ColumnInfo],
) {
    for (state, spec) in states.iter_mut().zip(specs.iter()) {
        match state {
            AggregateState::Count(count) => {
                if let Some(expr) = spec.arg_expr.as_ref() {
                    if !resolve_columnar_value(expr, row_idx, table_cols, schema_cols).is_null() {
                        *count += 1;
                    }
                } else {
                    *count += 1;
                }
            }
            AggregateState::Sum { sum, has_value } => {
                if let Some(expr) = spec.arg_expr.as_ref() {
                    if let Some(val) =
                        resolve_columnar_value(expr, row_idx, table_cols, schema_cols).as_f64()
                    {
                        *sum += val;
                        *has_value = true;
                    }
                }
            }
            AggregateState::Avg { sum, count } => {
                if let Some(expr) = spec.arg_expr.as_ref() {
                    if let Some(val) =
                        resolve_columnar_value(expr, row_idx, table_cols, schema_cols).as_f64()
                    {
                        *sum += val;
                        *count += 1;
                    }
                }
            }
            AggregateState::Min(current) => {
                if let Some(expr) = spec.arg_expr.as_ref() {
                    let val = resolve_columnar_value(expr, row_idx, table_cols, schema_cols);
                    if !val.is_null() {
                        let next = match current.take() {
                            None => val,
                            Some(existing) if val.sql_lt(&existing) => val,
                            Some(existing) => existing,
                        };
                        *current = Some(next);
                    }
                }
            }
            AggregateState::Max(current) => {
                if let Some(expr) = spec.arg_expr.as_ref() {
                    let val = resolve_columnar_value(expr, row_idx, table_cols, schema_cols);
                    if !val.is_null() {
                        let next = match current.take() {
                            None => val,
                            Some(existing) if val.sql_gt(&existing) => val,
                            Some(existing) => existing,
                        };
                        *current = Some(next);
                    }
                }
            }
        }
    }
}

fn finalize_aggregate_state(state: &AggregateState) -> TypedValue {
    match state {
        AggregateState::Count(count) => TypedValue::Int(*count),
        AggregateState::Sum { sum, has_value } => {
            if !has_value {
                TypedValue::Null
            } else if *sum == (*sum as i64) as f64 {
                TypedValue::Int(*sum as i64)
            } else {
                TypedValue::Float(*sum)
            }
        }
        AggregateState::Avg { sum, count } => {
            if *count == 0 {
                TypedValue::Null
            } else {
                TypedValue::Float(*sum / *count as f64)
            }
        }
        AggregateState::Min(val) | AggregateState::Max(val) => {
            val.clone().unwrap_or(TypedValue::Null)
        }
    }
}

fn is_supported_columnar_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Identifier(_) | Expr::CompoundIdentifier(_) | Expr::Value(_) => true,
        Expr::UnaryOp {
            op: sqlparser::ast::UnaryOperator::Minus,
            expr,
        } => is_supported_columnar_expr(expr),
        Expr::BinaryOp { left, op, right } => {
            matches!(
                op,
                sqlparser::ast::BinaryOperator::Plus
                    | sqlparser::ast::BinaryOperator::Minus
                    | sqlparser::ast::BinaryOperator::Multiply
                    | sqlparser::ast::BinaryOperator::Divide
                    | sqlparser::ast::BinaryOperator::Modulo
            ) && is_supported_columnar_expr(left)
                && is_supported_columnar_expr(right)
        }
        Expr::Nested(inner) => is_supported_columnar_expr(inner),
        _ => false,
    }
}

fn resolve_columnar_value(
    expr: &Expr,
    row_idx: usize,
    table_cols: &TableColumns,
    schema_cols: &[ColumnInfo],
) -> TypedValue {
    match expr {
        Expr::Identifier(ident) => {
            let col_name = ident.value.to_lowercase();
            schema_cols
                .iter()
                .find(|c| c.name.to_lowercase() == col_name)
                .map(|col| table_cols.get_typed_value(row_idx, col))
                .unwrap_or(TypedValue::Null)
        }
        Expr::CompoundIdentifier(parts) => {
            let col_name = parts
                .last()
                .map(|part| part.value.to_lowercase())
                .unwrap_or_default();
            schema_cols
                .iter()
                .find(|c| c.name.to_lowercase() == col_name)
                .map(|col| table_cols.get_typed_value(row_idx, col))
                .unwrap_or(TypedValue::Null)
        }
        Expr::Value(_) => resolve_literal(expr).unwrap_or(TypedValue::Null),
        Expr::UnaryOp {
            op: sqlparser::ast::UnaryOperator::Minus,
            expr,
        } => match resolve_columnar_value(expr, row_idx, table_cols, schema_cols) {
            TypedValue::Int(i) => TypedValue::Int(-i),
            TypedValue::Float(f) => TypedValue::Float(-f),
            _ => TypedValue::Null,
        },
        Expr::BinaryOp { left, op, right } => {
            let lval = resolve_columnar_value(left, row_idx, table_cols, schema_cols);
            let rval = resolve_columnar_value(right, row_idx, table_cols, schema_cols);
            eval_arithmetic_typed(&lval, op, &rval)
        }
        Expr::Nested(inner) => resolve_columnar_value(inner, row_idx, table_cols, schema_cols),
        _ => TypedValue::Null,
    }
}

fn eval_arithmetic_typed(
    lval: &TypedValue,
    op: &sqlparser::ast::BinaryOperator,
    rval: &TypedValue,
) -> TypedValue {
    if lval.is_null() || rval.is_null() {
        return TypedValue::Null;
    }

    match (lval.as_f64(), rval.as_f64()) {
        (Some(l), Some(r)) => {
            let result = match op {
                sqlparser::ast::BinaryOperator::Plus => l + r,
                sqlparser::ast::BinaryOperator::Minus => l - r,
                sqlparser::ast::BinaryOperator::Multiply => l * r,
                sqlparser::ast::BinaryOperator::Divide => {
                    if r == 0.0 {
                        return TypedValue::Null;
                    }
                    l / r
                }
                sqlparser::ast::BinaryOperator::Modulo => {
                    if r == 0.0 {
                        return TypedValue::Null;
                    }
                    l % r
                }
                _ => return TypedValue::Null,
            };

            if matches!(lval, TypedValue::Int(_))
                && matches!(rval, TypedValue::Int(_))
                && result == (result as i64) as f64
                && !matches!(op, sqlparser::ast::BinaryOperator::Divide)
            {
                TypedValue::Int(result as i64)
            } else {
                TypedValue::Float(result)
            }
        }
        _ => TypedValue::Null,
    }
}

fn typed_value_to_group_key(val: &TypedValue) -> String {
    if val.is_null() {
        "NULL".to_string()
    } else {
        val.to_string()
    }
}

fn query_offset_limit(query: &sqlparser::ast::Query) -> (usize, Option<usize>) {
    (
        query
            .offset
            .as_ref()
            .and_then(|o| expr_to_usize(&o.value))
            .unwrap_or(0),
        query.limit.as_ref().and_then(|l| expr_to_usize(l)),
    )
}

fn paginate_rows<T>(rows: Vec<T>, offset: usize, limit: Option<usize>) -> Vec<T> {
    if let Some(limit) = limit {
        rows.into_iter().skip(offset).take(limit).collect()
    } else {
        rows.into_iter().skip(offset).collect()
    }
}

fn record_stage_duration(stage: &'static str, elapsed_seconds: f64) {
    metrics::counter!("snowdrop_query_stage_total", "stage" => stage).increment(1);
    metrics::histogram!("snowdrop_query_stage_duration_seconds", "stage" => stage)
        .record(elapsed_seconds);
}

fn record_cardinality(kind: &'static str, value: usize) {
    metrics::histogram!("snowdrop_query_cardinality_rows", "kind" => kind).record(value as f64);
}

fn should_use_index_scan(op: &str, candidate_rows: usize, total_rows_hint: Option<usize>) -> bool {
    if candidate_rows == 0 {
        return true;
    }

    if op == "eq" && candidate_rows <= 512 {
        return true;
    }

    if let Some(total_rows) = total_rows_hint {
        if total_rows > 0 && candidate_rows * 100 > total_rows * 20 {
            return false;
        }
    }

    candidate_rows <= 512
}

/// Try to extract a usize from a SQL expression (for LIMIT/OFFSET).
fn expr_to_usize(expr: &Expr) -> Option<usize> {
    match expr {
        Expr::Value(Value::Number(n, _)) => n.parse::<usize>().ok(),
        _ => None,
    }
}

/// Parse a simple WHERE clause: `col = value`
fn parse_simple_where(selection: Option<&Expr>) -> PgWireResult<(String, String)> {
    let expr = selection.ok_or_else(|| user_error("WHERE clause is required".to_string()))?;

    match expr {
        Expr::BinaryOp { left, op, right } => {
            if !matches!(op, sqlparser::ast::BinaryOperator::Eq) {
                return Err(user_error(format!(
                    "only = operator supported in WHERE for M3, got {:?}",
                    op
                )));
            }

            let col_name = match left.as_ref() {
                Expr::Identifier(ident) => ident.value.clone(),
                Expr::CompoundIdentifier(parts) => {
                    parts.last().map(|i| i.value.clone()).unwrap_or_default()
                }
                _ => {
                    return Err(user_error(
                        "WHERE left side must be a column name".to_string(),
                    ))
                }
            };

            let val_str = match right.as_ref() {
                Expr::Value(Value::Number(n, _)) => n.clone(),
                Expr::Value(Value::SingleQuotedString(s)) => s.clone(),
                Expr::Value(Value::DoubleQuotedString(s)) => s.clone(),
                Expr::Value(Value::Boolean(b)) => b.to_string(),
                Expr::UnaryOp {
                    op: sqlparser::ast::UnaryOperator::Minus,
                    expr,
                } => {
                    let inner = match expr.as_ref() {
                        Expr::Value(Value::Number(n, _)) => n.clone(),
                        _ => {
                            return Err(user_error("unsupported WHERE value".to_string()));
                        }
                    };
                    format!("-{}", inner)
                }
                _ => {
                    return Err(user_error(format!("unsupported WHERE value: {:?}", right)));
                }
            };

            Ok((col_name, val_str))
        }
        _ => Err(user_error(
            "WHERE clause must be a simple col = value expression".to_string(),
        )),
    }
}

// ── JOIN helpers ──

/// Extract table name and optional alias from a TableFactor.
fn extract_table_info(
    factor: &sqlparser::ast::TableFactor,
) -> PgWireResult<(ObjectName, Option<String>)> {
    match factor {
        sqlparser::ast::TableFactor::Table { name, alias, .. } => {
            let alias_str = alias.as_ref().map(|a| a.name.value.clone());
            Ok((name.clone(), alias_str))
        }
        _ => Err(user_error("JOIN requires table names".to_string())),
    }
}

/// Extract equi-join conditions from ON expression.
/// Returns pairs of (left_col_name, right_col_name).
fn extract_join_conditions(expr: &Expr) -> PgWireResult<Vec<(String, String)>> {
    match expr {
        Expr::BinaryOp { left, op, right } => match op {
            sqlparser::ast::BinaryOperator::Eq => {
                let left_col = extract_column_name(left)?;
                let right_col = extract_column_name(right)?;
                Ok(vec![(left_col, right_col)])
            }
            sqlparser::ast::BinaryOperator::And => {
                let mut conds = extract_join_conditions(left)?;
                conds.extend(extract_join_conditions(right)?);
                Ok(conds)
            }
            _ => Err(user_error(
                "only equi-join (=) conditions supported in ON clause".to_string(),
            )),
        },
        Expr::Nested(inner) => extract_join_conditions(inner),
        _ => Err(user_error("unsupported ON clause expression".to_string())),
    }
}

/// Extract column reference, preserving qualifier if present.
/// Returns "alias.col" for CompoundIdentifier, "col" for bare Identifier.
fn extract_column_name(expr: &Expr) -> PgWireResult<String> {
    match expr {
        Expr::Identifier(ident) => Ok(ident.value.to_lowercase()),
        Expr::CompoundIdentifier(parts) => Ok(parts
            .iter()
            .map(|p| p.value.to_lowercase())
            .collect::<Vec<_>>()
            .join(".")),
        _ => Err(user_error(
            "ON condition must reference column names".to_string(),
        )),
    }
}

/// Prefix all keys in a row with "alias.".
fn prefix_row(row: Row, prefix: &str) -> Row {
    let prefix_lower = prefix.to_lowercase();
    row.into_iter()
        .map(|(k, v)| (format!("{}.{}", prefix_lower, k), v))
        .collect()
}

/// Build combined schema for joined tables with prefixed column names.

/// Resolve projection for JOIN queries. Handles alias.col, *, and bare col names.
fn resolve_join_projection(
    items: &[SelectItem],
    combined_schema: &[ColumnInfo],
) -> PgWireResult<Vec<(String, String)>> {
    let mut result: Vec<(String, String)> = Vec::new(); // (display_name, row_key)

    for item in items {
        match item {
            SelectItem::Wildcard(_) => {
                for col in combined_schema {
                    result.push((col.name.clone(), col.name.clone()));
                }
            }
            SelectItem::UnnamedExpr(Expr::CompoundIdentifier(parts)) => {
                let qualified = parts
                    .iter()
                    .map(|p| p.value.to_lowercase())
                    .collect::<Vec<_>>()
                    .join(".");
                let display = parts
                    .last()
                    .map(|p| p.value.to_lowercase())
                    .unwrap_or_default();
                result.push((display, qualified));
            }
            SelectItem::UnnamedExpr(Expr::Identifier(ident)) => {
                let name = ident.value.to_lowercase();
                let key = find_column_key(&name, combined_schema)?;
                result.push((name, key));
            }
            SelectItem::ExprWithAlias { expr, alias } => {
                let key = match expr {
                    Expr::CompoundIdentifier(parts) => parts
                        .iter()
                        .map(|p| p.value.to_lowercase())
                        .collect::<Vec<_>>()
                        .join("."),
                    Expr::Identifier(ident) => {
                        find_column_key(&ident.value.to_lowercase(), combined_schema)?
                    }
                    _ => {
                        return Err(user_error(
                            "unsupported expression in JOIN projection".to_string(),
                        ))
                    }
                };
                result.push((alias.value.clone(), key));
            }
            _ => {}
        }
    }

    Ok(result)
}

/// Find the fully qualified column key for an unqualified column name.
/// Extract all column names referenced in an AST expression tree.
fn collect_expr_columns(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::Identifier(ident) => {
            out.insert(ident.value.to_lowercase());
        }
        Expr::CompoundIdentifier(parts) => {
            if let Some(last) = parts.last() {
                out.insert(last.value.to_lowercase());
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_expr_columns(left, out);
            collect_expr_columns(right, out);
        }
        Expr::UnaryOp { expr, .. } => collect_expr_columns(expr, out),
        Expr::Nested(inner) => collect_expr_columns(inner, out),
        Expr::Between {
            expr, low, high, ..
        } => {
            collect_expr_columns(expr, out);
            collect_expr_columns(low, out);
            collect_expr_columns(high, out);
        }
        Expr::InList { expr, list, .. } => {
            collect_expr_columns(expr, out);
            for item in list {
                collect_expr_columns(item, out);
            }
        }
        Expr::IsNull(inner) | Expr::IsNotNull(inner) => collect_expr_columns(inner, out),
        Expr::Like { expr, pattern, .. } => {
            collect_expr_columns(expr, out);
            collect_expr_columns(pattern, out);
        }
        Expr::Function(f) => {
            if let sqlparser::ast::FunctionArguments::List(list) = &f.args {
                for arg in &list.args {
                    if let FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) = arg {
                        collect_expr_columns(e, out);
                    }
                }
            }
        }
        Expr::Cast { expr, .. } => collect_expr_columns(expr, out),
        Expr::Substring {
            expr,
            substring_from,
            substring_for,
            ..
        } => {
            collect_expr_columns(expr, out);
            if let Some(from) = substring_from.as_deref() {
                collect_expr_columns(from, out);
            }
            if let Some(for_expr) = substring_for.as_deref() {
                collect_expr_columns(for_expr, out);
            }
        }
        Expr::Trim {
            expr,
            trim_what,
            trim_characters,
            ..
        } => {
            collect_expr_columns(expr, out);
            if let Some(trim_what) = trim_what.as_deref() {
                collect_expr_columns(trim_what, out);
            }
            if let Some(chars) = trim_characters.as_deref() {
                for char_expr in chars {
                    collect_expr_columns(char_expr, out);
                }
            }
        }
        Expr::Case {
            operand,
            conditions,
            results,
            else_result,
            ..
        } => {
            if let Some(op) = operand {
                collect_expr_columns(op, out);
            }
            for c in conditions {
                collect_expr_columns(c, out);
            }
            for r in results {
                collect_expr_columns(r, out);
            }
            if let Some(e) = else_result {
                collect_expr_columns(e, out);
            }
        }
        _ => {}
    }
}

fn collect_expr_refs(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::Identifier(ident) => {
            out.insert(ident.value.to_lowercase());
        }
        Expr::CompoundIdentifier(parts) => {
            out.insert(
                parts
                    .iter()
                    .map(|p| p.value.to_lowercase())
                    .collect::<Vec<_>>()
                    .join("."),
            );
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_expr_refs(left, out);
            collect_expr_refs(right, out);
        }
        Expr::UnaryOp { expr, .. } => collect_expr_refs(expr, out),
        Expr::Nested(inner) => collect_expr_refs(inner, out),
        Expr::Between {
            expr, low, high, ..
        } => {
            collect_expr_refs(expr, out);
            collect_expr_refs(low, out);
            collect_expr_refs(high, out);
        }
        Expr::InList { expr, list, .. } => {
            collect_expr_refs(expr, out);
            for item in list {
                collect_expr_refs(item, out);
            }
        }
        Expr::IsNull(inner) | Expr::IsNotNull(inner) => collect_expr_refs(inner, out),
        Expr::Like { expr, pattern, .. } => {
            collect_expr_refs(expr, out);
            collect_expr_refs(pattern, out);
        }
        Expr::Function(f) => {
            if let sqlparser::ast::FunctionArguments::List(list) = &f.args {
                for arg in &list.args {
                    if let FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) = arg {
                        collect_expr_refs(e, out);
                    }
                }
            }
        }
        Expr::Cast { expr, .. } => collect_expr_refs(expr, out),
        Expr::Substring {
            expr,
            substring_from,
            substring_for,
            ..
        } => {
            collect_expr_refs(expr, out);
            if let Some(from) = substring_from.as_deref() {
                collect_expr_refs(from, out);
            }
            if let Some(for_expr) = substring_for.as_deref() {
                collect_expr_refs(for_expr, out);
            }
        }
        Expr::Trim {
            expr,
            trim_what,
            trim_characters,
            ..
        } => {
            collect_expr_refs(expr, out);
            if let Some(trim_what) = trim_what.as_deref() {
                collect_expr_refs(trim_what, out);
            }
            if let Some(chars) = trim_characters.as_deref() {
                for char_expr in chars {
                    collect_expr_refs(char_expr, out);
                }
            }
        }
        Expr::Case {
            operand,
            conditions,
            results,
            else_result,
            ..
        } => {
            if let Some(op) = operand {
                collect_expr_refs(op, out);
            }
            for c in conditions {
                collect_expr_refs(c, out);
            }
            for r in results {
                collect_expr_refs(r, out);
            }
            if let Some(e) = else_result {
                collect_expr_refs(e, out);
            }
        }
        _ => {}
    }
}

fn collect_conjuncts(expr: &Expr, out: &mut Vec<Expr>) {
    match expr {
        Expr::BinaryOp {
            left,
            op: sqlparser::ast::BinaryOperator::And,
            right,
        } => {
            collect_conjuncts(left, out);
            collect_conjuncts(right, out);
        }
        Expr::Nested(inner) => collect_conjuncts(inner, out),
        _ => out.push(expr.clone()),
    }
}

fn combine_conjuncts(exprs: Vec<Expr>) -> Option<Expr> {
    let mut iter = exprs.into_iter();
    let first = iter.next()?;
    Some(iter.fold(first, |acc, expr| Expr::BinaryOp {
        left: Box::new(acc),
        op: sqlparser::ast::BinaryOperator::And,
        right: Box::new(expr),
    }))
}

fn expr_matches_table(expr: &Expr, table_prefix: &str, schema_cols: &[ColumnInfo]) -> bool {
    let mut refs = HashSet::new();
    collect_expr_refs(expr, &mut refs);
    if refs.is_empty() {
        return false;
    }

    let prefix = table_prefix.to_lowercase();
    refs.into_iter().all(|reference| {
        if let Some((qualifier, col_name)) = reference.split_once('.') {
            qualifier == prefix
                && schema_cols
                    .iter()
                    .any(|col| col.name.to_lowercase() == col_name)
        } else {
            schema_cols
                .iter()
                .any(|col| col.name.to_lowercase() == reference)
        }
    })
}

fn take_pushdown_predicate(
    conjuncts: &mut Vec<Expr>,
    table_prefix: &str,
    schema_cols: &[ColumnInfo],
) -> Option<Expr> {
    let mut selected = Vec::new();
    let mut remaining = Vec::new();

    for expr in conjuncts.drain(..) {
        if expr_matches_table(&expr, table_prefix, schema_cols) {
            selected.push(expr);
        } else {
            remaining.push(expr);
        }
    }

    *conjuncts = remaining;
    combine_conjuncts(selected)
}

fn projection_needs_all_columns(items: &[SelectItem]) -> bool {
    items.iter().any(|item| {
        matches!(
            item,
            SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _)
        )
    })
}

fn collect_join_query_refs(
    select: &sqlparser::ast::Select,
    query: &sqlparser::ast::Query,
) -> HashSet<String> {
    let mut refs = HashSet::new();

    if let Some(selection) = select.selection.as_ref() {
        collect_expr_refs(selection, &mut refs);
    }

    for item in &select.projection {
        match item {
            SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
                collect_expr_refs(expr, &mut refs);
            }
            _ => {}
        }
    }

    if let Some(order_by) = query.order_by.as_ref() {
        for expr in &order_by.exprs {
            collect_expr_refs(&expr.expr, &mut refs);
        }
    }

    if let sqlparser::ast::GroupByExpr::Expressions(exprs, _) = &select.group_by {
        for expr in exprs {
            collect_expr_refs(expr, &mut refs);
        }
    }

    for join in &select.from[0].joins {
        if let JoinOperator::Inner(JoinConstraint::On(expr))
        | JoinOperator::LeftOuter(JoinConstraint::On(expr)) = &join.join_operator
        {
            collect_expr_refs(expr, &mut refs);
        }
    }

    refs
}

fn compute_join_needed_col_ids(
    schema_cols: &[ColumnInfo],
    table_prefix: &str,
    refs: &HashSet<String>,
    include_all: bool,
) -> HashSet<u32> {
    if include_all {
        return schema_cols.iter().map(|c| c.id).collect();
    }

    let prefix = table_prefix.to_lowercase();
    let mut ids: HashSet<u32> = schema_cols
        .iter()
        .filter(|col| {
            let name = col.name.to_lowercase();
            refs.contains(&name) || refs.contains(&format!("{}.{}", prefix, name))
        })
        .map(|col| col.id)
        .collect();

    if ids.is_empty() {
        ids.extend(schema_cols.iter().map(|c| c.id));
    }

    ids
}

/// Compute the set of column IDs needed by a query (WHERE + SELECT + ORDER BY + GROUP BY).
fn compute_needed_col_ids(
    selection: Option<&Expr>,
    projection: &[SelectItem],
    order_by: Option<&sqlparser::ast::OrderBy>,
    group_by: &sqlparser::ast::GroupByExpr,
    schema_cols: &[ColumnInfo],
) -> HashSet<u32> {
    let mut col_names = HashSet::new();
    let mut needs_all = false;

    // WHERE clause columns
    if let Some(expr) = selection {
        collect_expr_columns(expr, &mut col_names);
    }

    // SELECT projection columns
    for item in projection {
        match item {
            SelectItem::Wildcard(_) => {
                needs_all = true;
            }
            SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
                collect_expr_columns(expr, &mut col_names);
            }
            SelectItem::QualifiedWildcard(_, _) => {
                needs_all = true;
            }
        }
    }

    // ORDER BY columns
    if let Some(ob) = order_by {
        for oe in &ob.exprs {
            collect_expr_columns(&oe.expr, &mut col_names);
        }
    }

    // GROUP BY columns
    if let sqlparser::ast::GroupByExpr::Expressions(exprs, _) = group_by {
        for expr in exprs {
            collect_expr_columns(expr, &mut col_names);
        }
    }

    if needs_all {
        return schema_cols.iter().map(|c| c.id).collect();
    }

    schema_cols
        .iter()
        .filter(|c| col_names.contains(&c.name.to_lowercase()))
        .map(|c| c.id)
        .collect()
}

fn find_column_key(col_name: &str, combined_schema: &[ColumnInfo]) -> PgWireResult<String> {
    let matches: Vec<&ColumnInfo> = combined_schema
        .iter()
        .filter(|c| c.name.ends_with(&format!(".{}", col_name)))
        .collect();

    match matches.len() {
        1 => Ok(matches[0].name.clone()),
        0 => Err(user_error(format!("column '{}' not found", col_name))),
        _ => Err(user_error(format!(
            "column '{}' is ambiguous, qualify with table alias",
            col_name
        ))),
    }
}

// ── Columnar predicate evaluation ──

/// Apply a WHERE predicate using columnar SIMD filters.
/// Decomposes AND-connected conditions into per-column bitmap filters.
/// Falls back to row-by-row for unsupported patterns.
fn apply_columnar_predicate(
    expr: &Expr,
    table_cols: &TableColumns,
    schema_cols: &[ColumnInfo],
) -> Bitmap {
    match expr {
        Expr::BinaryOp {
            left,
            op: sqlparser::ast::BinaryOperator::And,
            right,
        } => {
            let mut left_bm = apply_columnar_predicate(left, table_cols, schema_cols);
            let right_bm = apply_columnar_predicate(right, table_cols, schema_cols);
            left_bm.and_with(&right_bm);
            left_bm
        }
        Expr::BinaryOp {
            left,
            op: sqlparser::ast::BinaryOperator::Or,
            right,
        } => {
            let left_bm = apply_columnar_predicate(left, table_cols, schema_cols);
            let right_bm = apply_columnar_predicate(right, table_cols, schema_cols);
            let mut result = left_bm;
            for (rw, ow) in result.words.iter_mut().zip(right_bm.words.iter()) {
                *rw |= ow;
            }
            result
        }
        Expr::Nested(inner) => apply_columnar_predicate(inner, table_cols, schema_cols),
        Expr::Between {
            expr: col_expr,
            low,
            high,
            negated,
        } => {
            if let Some(col_id) = resolve_col_id(col_expr, schema_cols) {
                if let (Some(lo_val), Some(hi_val)) = (resolve_literal(low), resolve_literal(high))
                {
                    if let Some((typed_col, null_bm)) = table_cols.columns.get(&col_id) {
                        let bm_ge =
                            column_store::filter_column(typed_col, null_bm, CmpOp::GtEq, &lo_val);
                        let bm_le =
                            column_store::filter_column(typed_col, null_bm, CmpOp::LtEq, &hi_val);
                        let mut bm = bm_ge;
                        bm.and_with(&bm_le);
                        if *negated {
                            for w in bm.words.iter_mut() {
                                *w = !*w;
                            }
                            // Clear trailing bits
                            let remainder = bm.len % 64;
                            if remainder > 0 {
                                if let Some(last) = bm.words.last_mut() {
                                    *last &= (1u64 << remainder) - 1;
                                }
                            }
                        }
                        return bm;
                    }
                }
            }
            fallback_predicate(expr, table_cols, schema_cols)
        }
        Expr::BinaryOp { left, op, right } => {
            if let Some(cmp_op) = sql_op_to_cmp(op) {
                // Try column OP literal
                if let (Some(col_id), Some(literal)) =
                    (resolve_col_id(left, schema_cols), resolve_literal(right))
                {
                    if let Some((typed_col, null_bm)) = table_cols.columns.get(&col_id) {
                        return column_store::filter_column(typed_col, null_bm, cmp_op, &literal);
                    }
                }
                // Try literal OP column (reversed)
                if let (Some(literal), Some(col_id)) =
                    (resolve_literal(left), resolve_col_id(right, schema_cols))
                {
                    let reversed_op = reverse_cmp(cmp_op);
                    if let Some((typed_col, null_bm)) = table_cols.columns.get(&col_id) {
                        return column_store::filter_column(
                            typed_col,
                            null_bm,
                            reversed_op,
                            &literal,
                        );
                    }
                }
            }
            fallback_predicate(expr, table_cols, schema_cols)
        }
        _ => fallback_predicate(expr, table_cols, schema_cols),
    }
}

/// Row-by-row fallback for predicates that can't be handled columnarly.
fn fallback_predicate(
    expr: &Expr,
    table_cols: &TableColumns,
    schema_cols: &[ColumnInfo],
) -> Bitmap {
    let mut bm = Bitmap::all_false(table_cols.row_count);
    for idx in 0..table_cols.row_count {
        let mut row = Row::new();
        for col in schema_cols {
            let val = table_cols.get_typed_value(idx, col);
            row.insert(col.name.to_lowercase(), val);
        }
        if evaluate_predicate(expr, &row, schema_cols) {
            bm.set(idx);
        }
    }
    bm
}

/// Resolve an AST expression to a column ID (for simple column references).
fn resolve_col_id(expr: &Expr, schema_cols: &[ColumnInfo]) -> Option<u32> {
    match expr {
        Expr::Identifier(ident) => {
            let name = ident.value.to_lowercase();
            schema_cols
                .iter()
                .find(|c| c.name.to_lowercase() == name)
                .map(|c| c.id)
        }
        Expr::CompoundIdentifier(parts) => {
            let name = parts.last()?.value.to_lowercase();
            schema_cols
                .iter()
                .find(|c| c.name.to_lowercase() == name)
                .map(|c| c.id)
        }
        _ => None,
    }
}

/// Resolve an AST expression to a literal TypedValue.
fn resolve_literal(expr: &Expr) -> Option<TypedValue> {
    match expr {
        Expr::Value(Value::Number(n, _)) => {
            if let Ok(i) = n.parse::<i64>() {
                Some(TypedValue::Int(i))
            } else if let Ok(f) = n.parse::<f64>() {
                Some(TypedValue::Float(f))
            } else {
                None
            }
        }
        Expr::Value(Value::SingleQuotedString(s)) | Expr::Value(Value::DoubleQuotedString(s)) => {
            Some(TypedValue::Text(s.clone()))
        }
        Expr::Value(Value::Boolean(b)) => Some(TypedValue::Boolean(*b)),
        Expr::UnaryOp {
            op: sqlparser::ast::UnaryOperator::Minus,
            expr,
        } => {
            if let Some(TypedValue::Int(i)) = resolve_literal(expr) {
                Some(TypedValue::Int(-i))
            } else if let Some(TypedValue::Float(f)) = resolve_literal(expr) {
                Some(TypedValue::Float(-f))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn sql_op_to_cmp(op: &sqlparser::ast::BinaryOperator) -> Option<CmpOp> {
    match op {
        sqlparser::ast::BinaryOperator::Eq => Some(CmpOp::Eq),
        sqlparser::ast::BinaryOperator::Lt => Some(CmpOp::Lt),
        sqlparser::ast::BinaryOperator::LtEq => Some(CmpOp::LtEq),
        sqlparser::ast::BinaryOperator::Gt => Some(CmpOp::Gt),
        sqlparser::ast::BinaryOperator::GtEq => Some(CmpOp::GtEq),
        _ => None,
    }
}

fn reverse_cmp(op: CmpOp) -> CmpOp {
    match op {
        CmpOp::Lt => CmpOp::Gt,
        CmpOp::LtEq => CmpOp::GtEq,
        CmpOp::Gt => CmpOp::Lt,
        CmpOp::GtEq => CmpOp::LtEq,
        CmpOp::Eq => CmpOp::Eq,
    }
}

/// Extract a single index-usable predicate from a WHERE clause.
/// Returns (column, op_type, encoded_value, optional_high_for_range).
/// op_type: "eq", "gt", "gte", "lt", "lte", "range"
fn extract_index_candidate<'a>(
    expr: &Expr,
    indexed_cols: &[&'a ColumnInfo],
) -> Option<(&'a ColumnInfo, String, String, Option<String>)> {
    match expr {
        // Simple comparison: col op literal
        Expr::BinaryOp { left, op, right } => {
            use sqlparser::ast::BinaryOperator;

            // Handle AND: try left first, then right
            if matches!(op, BinaryOperator::And) {
                let left_result = extract_index_candidate(left, indexed_cols);
                if left_result.is_some() {
                    return left_result;
                }
                return extract_index_candidate(right, indexed_cols);
            }

            // col op literal
            let (col_name, literal, op_str) = if let (Expr::Identifier(ident), Some(lit)) =
                (left.as_ref(), resolve_literal(right))
            {
                let op_str = match op {
                    BinaryOperator::Eq => "eq",
                    BinaryOperator::Gt => "gt",
                    BinaryOperator::GtEq => "gte",
                    BinaryOperator::Lt => "lt",
                    BinaryOperator::LtEq => "lte",
                    _ => return None,
                };
                (ident.value.to_lowercase(), lit, op_str)
            } else if let (Some(lit), Expr::Identifier(ident)) =
                (resolve_literal(left), right.as_ref())
            {
                // literal op col -> reverse the op
                let op_str = match op {
                    BinaryOperator::Eq => "eq",
                    BinaryOperator::Gt => "lte",
                    BinaryOperator::GtEq => "lt",
                    BinaryOperator::Lt => "gte",
                    BinaryOperator::LtEq => "gt",
                    _ => return None,
                };
                (ident.value.to_lowercase(), lit, op_str)
            } else {
                return None;
            };

            // Check if the column is indexed
            let col = indexed_cols
                .iter()
                .find(|c| c.name.to_lowercase() == col_name)?;

            let encoded = encode_typed_value(&literal, &col.data_type);
            Some((col, op_str.to_string(), encoded, None))
        }
        // BETWEEN: col BETWEEN low AND high
        Expr::Between {
            expr: col_expr,
            low,
            high,
            negated,
            ..
        } => {
            if *negated {
                return None;
            }
            if let Expr::Identifier(ident) = col_expr.as_ref() {
                let col_name = ident.value.to_lowercase();
                let col = indexed_cols
                    .iter()
                    .find(|c| c.name.to_lowercase() == col_name)?;
                let low_val = resolve_literal(low)?;
                let high_val = resolve_literal(high)?;
                let encoded_low = encode_typed_value(&low_val, &col.data_type);
                let encoded_high = encode_typed_value(&high_val, &col.data_type);
                Some((col, "range".to_string(), encoded_low, Some(encoded_high)))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Encode a TypedValue for index lookup using the same encoding as index storage.
fn encode_typed_value(val: &TypedValue, data_type: &str) -> String {
    let raw = val.to_string();
    encode_index_value(raw.as_bytes(), data_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use executor::typed_value::TypedValue;
    use sqlparser::dialect::PostgreSqlDialect;
    use sqlparser::parser::Parser;

    fn parse_order_by(sql: &str) -> Vec<OrderByExpr> {
        let stmts = Parser::parse_sql(&PostgreSqlDialect {}, sql).unwrap();
        match &stmts[0] {
            Statement::Query(query) => query.order_by.as_ref().unwrap().exprs.clone(),
            _ => panic!("expected query"),
        }
    }

    #[test]
    fn lookup_row_value_falls_back_from_qualified_to_display_name() {
        let mut row = Row::new();
        row.insert("name".into(), TypedValue::Text("Acme".into()));

        let val = lookup_row_value(&row, "c.name");
        assert_eq!(val, TypedValue::Text("Acme".into()));
    }

    #[test]
    fn sort_rows_orders_aggregate_rows_by_qualified_order_by_expr() {
        let mut rows = vec![
            {
                let mut row = Row::new();
                row.insert("name".into(), TypedValue::Text("Cyan".into()));
                row.insert("sum(o.total)".into(), TypedValue::Float(25.0));
                row
            },
            {
                let mut row = Row::new();
                row.insert("name".into(), TypedValue::Text("Beta".into()));
                row.insert("sum(o.total)".into(), TypedValue::Float(50.0));
                row
            },
            {
                let mut row = Row::new();
                row.insert("name".into(), TypedValue::Text("Acme".into()));
                row.insert("sum(o.total)".into(), TypedValue::Float(495.0));
                row
            },
        ];

        let order_by = parse_order_by(
            "SELECT c.name, SUM(o.total) FROM customers c INNER JOIN orders o ON c.id = o.customer_id GROUP BY c.name ORDER BY c.name",
        );

        sort_rows(&mut rows, &order_by, &[]).unwrap();

        let names: Vec<String> = rows
            .iter()
            .map(|row| lookup_row_value(row, "name").to_string())
            .collect();
        assert_eq!(names, vec!["Acme", "Beta", "Cyan"]);
    }

    #[test]
    fn rename_relation_columns_reads_original_lookup_keys_before_rewrite() {
        let relation = RelationResult {
            rows: vec![{
                let mut row = Row::new();
                row.insert("derived.customer_id".into(), TypedValue::Int(7));
                row.insert("derived.total_spent".into(), TypedValue::Float(125.0));
                row
            }],
            columns: vec![
                RelationColumn {
                    lookup_name: "derived.customer_id".into(),
                    output_name: "customer_id".into(),
                },
                RelationColumn {
                    lookup_name: "derived.total_spent".into(),
                    output_name: "total_spent".into(),
                },
            ],
        };

        let renamed =
            rename_relation_columns(relation, &["customer".to_string(), "amount".to_string()])
                .unwrap();

        assert_eq!(
            lookup_row_value(&renamed.rows[0], "customer"),
            TypedValue::Int(7)
        );
        assert_eq!(
            lookup_row_value(&renamed.rows[0], "amount"),
            TypedValue::Float(125.0)
        );
    }

    #[test]
    fn expand_projection_columns_keeps_compound_identifier_lookup_name() {
        let relation = RelationResult {
            rows: Vec::new(),
            columns: vec![
                RelationColumn {
                    lookup_name: "c.name".into(),
                    output_name: "name".into(),
                },
                RelationColumn {
                    lookup_name: "o.id".into(),
                    output_name: "id".into(),
                },
            ],
        };

        let columns = expand_projection_columns(
            &[SelectItem::UnnamedExpr(Expr::CompoundIdentifier(vec![
                sqlparser::ast::Ident::new("o"),
                sqlparser::ast::Ident::new("id"),
            ]))],
            &relation,
            false,
        )
        .unwrap();

        assert_eq!(columns.len(), 1);
        assert_eq!(columns[0].lookup_name, "o.id");
        assert_eq!(columns[0].output_name, "id");
    }
}
