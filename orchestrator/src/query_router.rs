use futures::stream;
use pgwire::api::results::{DataRowEncoder, FieldFormat, FieldInfo, QueryResponse, Response, Tag};
use pgwire::error::{ErrorInfo, PgWireError, PgWireResult};
use pgwire::messages::data::DataRow;
use schema::{ColumnType, Neo4jConnection};
use sqlparser::ast::{AlterTableOperation, ColumnOption, ObjectName, Statement};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::warn;

pub struct QueryRouter {
    neo4j: Arc<RwLock<Neo4jConnection>>,
    current_db: Arc<RwLock<String>>,
}

impl QueryRouter {
    pub fn new(neo4j: Neo4jConnection) -> Self {
        Self {
            neo4j: Arc::new(RwLock::new(neo4j)),
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
