use executor::{KvrocksConnection, WriteEngine};
use futures::stream;
use pgwire::api::results::{DataRowEncoder, FieldFormat, FieldInfo, QueryResponse, Response, Tag};
use pgwire::error::{ErrorInfo, PgWireError, PgWireResult};
use pgwire::messages::data::DataRow;
use schema::{ColumnType, Neo4jConnection};
use sqlparser::ast::{
    AlterTableOperation, ColumnOption, Expr, ObjectName, SetExpr, Statement, Value, Values,
};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::warn;
use wal::WalFile;

pub struct QueryRouter {
    neo4j: Arc<RwLock<Neo4jConnection>>,
    kvrocks: Arc<KvrocksConnection>,
    wal: Arc<RwLock<WalFile>>,
    current_db: Arc<RwLock<String>>,
}

impl QueryRouter {
    pub fn new(neo4j: Neo4jConnection, kvrocks: KvrocksConnection, wal: WalFile) -> Self {
        Self {
            neo4j: Arc::new(RwLock::new(neo4j)),
            kvrocks: Arc::new(kvrocks),
            wal: Arc::new(RwLock::new(wal)),
            current_db: Arc::new(RwLock::new("default".to_string())),
        }
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
            Statement::Update { table, assignments, selection, .. } => {
                self.handle_update(&table.relation, &assignments, selection.as_ref()).await
            }
            Statement::Delete(delete) => {
                self.handle_delete(&delete.from, delete.selection.as_ref()).await
            }
            Statement::Query(q) => {
                let q_str = q.to_string().to_uppercase();
                if q_str.contains("INFORMATION_SCHEMA.TABLES") {
                    return self.handle_info_schema_tables().await;
                }
                if q_str.contains("INFORMATION_SCHEMA.COLUMNS") {
                    return self.handle_info_schema_columns().await;
                }
                // TODO(M4): SELECT
                Ok(Response::EmptyQuery)
            }
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
        Ok(Response::Execution(Tag::new("CREATE DATABASE")))
    }

    async fn handle_drop_database<'a>(&self, name: &str) -> PgWireResult<Response<'a>> {
        let neo4j = self.neo4j.read().await;
        neo4j.drop_database(name).await.map_err(to_err)?;
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
        neo4j.drop_table(db_name, table_name).await.map_err(to_err)?;
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

                Ok(Response::Execution(Tag::new("ALTER TABLE")))
            }
            other => Err(user_error(format!(
                "ALTER TABLE {:?} is not supported",
                other
            ))),
        }
    }

    // ── DML handlers (M3) ──

    async fn handle_insert<'a>(
        &self,
        insert: sqlparser::ast::Insert,
    ) -> PgWireResult<Response<'a>> {
        let current_db = self.current_db.read().await.clone();
        let table_name_obj = insert.table_name.clone();
        let (db_name, table_name) = self.resolve_table_name(&table_name_obj, &current_db);

        let column_names: Vec<String> = insert.columns.iter().map(|c| c.value.clone()).collect();

        // Extract values from the INSERT source
        let rows_data = match insert.source.as_ref() {
            Some(query) => match query.body.as_ref() {
                SetExpr::Values(Values { rows, .. }) => rows.clone(),
                _ => return Err(user_error("INSERT must use VALUES".to_string())),
            },
            None => return Err(user_error("INSERT must have values".to_string())),
        };

        let mut total_inserted = 0u64;
        let neo4j = self.neo4j.read().await;

        for row in &rows_data {
            let values: Vec<Vec<u8>> = row
                .iter()
                .map(|expr| expr_to_bytes(expr))
                .collect::<Result<_, _>>()?;

            let mut wal = self.wal.write().await;
            let mut engine = WriteEngine::new(&self.kvrocks, &neo4j, &mut wal);
            let n = engine
                .insert(&db_name, &table_name, &column_names, &values)
                .await
                .map_err(to_err)?;
            total_inserted += n;
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
        let mut engine = WriteEngine::new(&self.kvrocks, &neo4j, &mut wal);
        let n = engine
            .update(&db_name, &table_name, &set_pairs, &where_col, &where_val)
            .await
            .map_err(to_err)?;

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
            sqlparser::ast::FromTable::WithFromKeyword(tables) => {
                tables.first().ok_or_else(|| user_error("DELETE requires FROM".to_string()))?
            }
            sqlparser::ast::FromTable::WithoutKeyword(tables) => {
                tables.first().ok_or_else(|| user_error("DELETE requires FROM".to_string()))?
            }
        };

        let table_name_obj = match &table_with_joins.relation {
            sqlparser::ast::TableFactor::Table { name, .. } => name.clone(),
            _ => return Err(user_error("DELETE requires a table name".to_string())),
        };
        let (db_name, table_name) = self.resolve_table_name(&table_name_obj, &current_db);

        let (where_col, where_val) = parse_simple_where(selection)?;

        let neo4j = self.neo4j.read().await;
        let mut wal = self.wal.write().await;
        let mut engine = WriteEngine::new(&self.kvrocks, &neo4j, &mut wal);
        let n = engine
            .delete(&db_name, &table_name, &where_col, &where_val)
            .await
            .map_err(to_err)?;

        Ok(Response::Execution(
            Tag::new("DELETE").with_rows(n as usize),
        ))
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
        Value::SingleQuotedString(s) => Ok(s.as_bytes().to_vec()),
        Value::DoubleQuotedString(s) => Ok(s.as_bytes().to_vec()),
        Value::Boolean(b) => Ok(if *b { b"true".to_vec() } else { b"false".to_vec() }),
        Value::Null => Ok(Vec::new()),
        _ => Err(user_error(format!("unsupported value type: {:?}", val))),
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
                    return Err(user_error(format!(
                        "unsupported WHERE value: {:?}",
                        right
                    )));
                }
            };

            Ok((col_name, val_str))
        }
        _ => Err(user_error(
            "WHERE clause must be a simple col = value expression".to_string(),
        )),
    }
}
