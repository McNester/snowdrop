use executor::{bytes_to_row, evaluate_predicate, resolve_value, KvrocksConnection, Row, TypedValue, WriteEngine};
use futures::stream;
use pgwire::api::results::{DataRowEncoder, FieldFormat, FieldInfo, QueryResponse, Response, Tag};
use pgwire::error::{ErrorInfo, PgWireError, PgWireResult};
use pgwire::messages::data::DataRow;
use schema::{ColumnInfo, ColumnType, Neo4jConnection};
use sqlparser::ast::{
    AlterTableOperation, ColumnOption, Expr, FunctionArg, FunctionArgExpr, JoinConstraint,
    JoinOperator, ObjectName, OrderByExpr, SelectItem, SetExpr, Statement, Value, Values,
};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use std::collections::HashMap;
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
                self.handle_select(*q).await
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

    // ── SELECT handler (M4) ──

    async fn handle_select<'a>(&self, query: sqlparser::ast::Query) -> PgWireResult<Response<'a>> {
        let select = match query.body.as_ref() {
            SetExpr::Select(sel) => sel.clone(),
            _ => return Err(user_error("only simple SELECT supported".to_string())),
        };

        // Resolve table from FROM clause
        if select.from.is_empty() {
            return Err(user_error("SELECT requires FROM clause".to_string()));
        }

        // Detect JOIN queries
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

        let neo4j = self.neo4j.read().await;
        let schema_cols = neo4j
            .describe_table(&db_name, &table_name)
            .await
            .map_err(to_err)?;
        let db = neo4j
            .get_database(&db_name)
            .await
            .map_err(to_err)?
            .ok_or_else(|| user_error(format!("database '{}' does not exist", db_name)))?;
        let table = neo4j
            .get_table(&db_name, &table_name)
            .await
            .map_err(to_err)?
            .ok_or_else(|| user_error(format!("table '{}' does not exist", table_name)))?;

        // Detect aggregates vs. plain select
        let agg_exprs = extract_aggregates(&select.projection);
        let is_aggregate = !agg_exprs.is_empty();
        let has_expr_proj = has_expression_projection(&select.projection);

        // Determine projected columns for non-aggregate queries
        let projected_cols = if is_aggregate || has_expr_proj {
            schema_cols.clone() // need all columns for expression/aggregate evaluation
        } else {
            resolve_projection(&select.projection, &schema_cols)?
        };

        // Fast path: COUNT(*) without WHERE can skip MGET entirely
        if is_aggregate && select.selection.is_none() && is_count_star_only(&select.projection) {
            let count = self.count_rows_fast(db.id, table.id).await?;
            return self.make_count_star_response(count);
        }

        // Fetch all rows from Kvrocks
        let rows = self
            .fetch_rows(db.id, table.id, &schema_cols)
            .await?;

        // Apply WHERE filter
        let filtered: Vec<Row> = if let Some(ref selection) = select.selection {
            rows.into_iter()
                .filter(|row| evaluate_predicate(selection, row, &schema_cols))
                .collect()
        } else {
            rows
        };

        // Handle aggregates
        if is_aggregate {
            let agg_offset = query.offset.as_ref().and_then(|o| expr_to_usize(&o.value)).unwrap_or(0);
            let agg_limit = query.limit.as_ref().and_then(|l| expr_to_usize(l));
            return self.handle_aggregate_result(
                &select.projection,
                &filtered,
                &schema_cols,
                &select.group_by,
                query.order_by.as_ref(),
                agg_limit,
                agg_offset,
            );
        }

        // Apply ORDER BY
        let mut result_rows = filtered;
        if let Some(ref order_by) = query.order_by {
            sort_rows(&mut result_rows, &order_by.exprs, &schema_cols)?;
        }

        // Apply OFFSET and LIMIT
        let offset = query.offset.as_ref().and_then(|o| expr_to_usize(&o.value)).unwrap_or(0);
        let limit = query.limit.as_ref().and_then(|l| expr_to_usize(l));

        let paged: Vec<Row> = if let Some(lim) = limit {
            result_rows.into_iter().skip(offset).take(lim).collect()
        } else {
            result_rows.into_iter().skip(offset).collect()
        };

        // Build pgwire response
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
        let neo4j = self.neo4j.read().await;

        // Parse left (base) table
        let (left_name, left_alias) = extract_table_info(&select.from[0].relation)?;
        let (left_db, left_tbl) = self.resolve_table_name(&left_name, &current_db);
        let left_schema = neo4j.describe_table(&left_db, &left_tbl).await.map_err(to_err)?;
        let left_db_info = neo4j.get_database(&left_db).await.map_err(to_err)?
            .ok_or_else(|| user_error(format!("database '{}' does not exist", left_db)))?;
        let left_tbl_info = neo4j.get_table(&left_db, &left_tbl).await.map_err(to_err)?
            .ok_or_else(|| user_error(format!("table '{}' does not exist", left_tbl)))?;

        let left_prefix = left_alias.as_deref().unwrap_or(&left_tbl);
        let left_rows = self.fetch_rows(left_db_info.id, left_tbl_info.id, &left_schema).await?;
        let mut joined_rows: Vec<Row> = left_rows
            .into_iter()
            .map(|r| prefix_row(r, left_prefix))
            .collect();
        let mut combined_schema = left_schema
            .iter()
            .map(|c| ColumnInfo {
                name: format!("{}.{}", left_prefix, c.name.to_lowercase()),
                ..c.clone()
            })
            .collect::<Vec<_>>();

        // Process each JOIN in sequence
        for join in &select.from[0].joins {
            let (right_name, right_alias) = extract_table_info(&join.relation)?;
            let (right_db, right_tbl) = self.resolve_table_name(&right_name, &current_db);
            let right_schema = neo4j.describe_table(&right_db, &right_tbl).await.map_err(to_err)?;
            let right_db_info = neo4j.get_database(&right_db).await.map_err(to_err)?
                .ok_or_else(|| user_error(format!("database '{}' does not exist", right_db)))?;
            let right_tbl_info = neo4j.get_table(&right_db, &right_tbl).await.map_err(to_err)?
                .ok_or_else(|| user_error(format!("table '{}' does not exist", right_tbl)))?;

            let (is_left_join, on_expr) = match &join.join_operator {
                JoinOperator::Inner(JoinConstraint::On(expr)) => (false, expr.clone()),
                JoinOperator::LeftOuter(JoinConstraint::On(expr)) => (true, expr.clone()),
                _ => return Err(user_error(
                    "only INNER JOIN ... ON and LEFT JOIN ... ON are supported".to_string(),
                )),
            };

            let join_conds = extract_join_conditions(&on_expr)?;
            let right_prefix = right_alias.as_deref().unwrap_or(&right_tbl);

            let right_rows = self.fetch_rows(right_db_info.id, right_tbl_info.id, &right_schema).await?;
            let right_prefixed: Vec<Row> = right_rows
                .into_iter()
                .map(|r| prefix_row(r, right_prefix))
                .collect();

            // Build hash map on right side join keys.
            // extract_join_conditions returns (ast_left, ast_right) but the AST
            // left may actually reference the right table. Detect and swap.
            let rp_lower = right_prefix.to_lowercase();
            let oriented_conds: Vec<(String, String)> = join_conds
                .iter()
                .map(|(a, b)| {
                    let a_is_right = a.starts_with(&format!("{}.", rp_lower));
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
                        format!("{}.{}", rp_lower, rc)
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

            // Perform this join step
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

            // Extend combined schema with right table columns
            combined_schema.extend(right_schema.iter().map(|c| ColumnInfo {
                name: format!("{}.{}", right_prefix, c.name.to_lowercase()),
                ..c.clone()
            }));
        }

        // Apply WHERE filter
        if let Some(ref selection) = select.selection {
            joined_rows.retain(|row| evaluate_predicate(selection, row, &combined_schema));
        }

        // Check for aggregates
        let agg_exprs = extract_aggregates(&select.projection);
        if !agg_exprs.is_empty() {
            let agg_offset = query.offset.as_ref().and_then(|o| expr_to_usize(&o.value)).unwrap_or(0);
            let agg_limit = query.limit.as_ref().and_then(|l| expr_to_usize(l));
            return self.handle_aggregate_result(
                &select.projection,
                &joined_rows,
                &combined_schema,
                &select.group_by,
                query.order_by.as_ref(),
                agg_limit,
                agg_offset,
            );
        }

        // Resolve projection
        let projected = resolve_join_projection(
            &select.projection, &combined_schema,
        )?;

        // Apply ORDER BY
        if let Some(ref order_by) = query.order_by {
            sort_rows(&mut joined_rows, &order_by.exprs, &combined_schema)?;
        }

        // Apply OFFSET and LIMIT
        let offset = query.offset.as_ref().and_then(|o| expr_to_usize(&o.value)).unwrap_or(0);
        let limit = query.limit.as_ref().and_then(|l| expr_to_usize(l));

        let paged: Vec<Row> = if let Some(lim) = limit {
            joined_rows.into_iter().skip(offset).take(lim).collect()
        } else {
            joined_rows.into_iter().skip(offset).collect()
        };

        // Build response
        let fields = Arc::new(
            projected.iter().map(|(name, _)| text_field(name)).collect::<Vec<_>>(),
        );

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

        Ok(Response::Query(query_response(fields, data_rows)))
    }

    /// Fast row count: SCAN keys and count unique row_ids without MGET.
    async fn count_rows_fast(&self, db_id: u32, table_id: u32) -> PgWireResult<i64> {
        let pattern = keys::table_scan_pattern(db_id, table_id)
            .map_err(|e| user_error(e.to_string()))?;
        let all_keys = self.kvrocks.scan_keys(&pattern).await.map_err(to_err)?;

        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for key in &all_keys {
            let parts: Vec<&str> = key.split(':').collect();
            if parts.len() == 4 {
                seen.insert(parts[2]);
            }
        }
        Ok(seen.len() as i64)
    }

    /// Build a simple COUNT(*) = N response.
    fn make_count_star_response<'a>(&self, count: i64) -> PgWireResult<Response<'a>> {
        let fields = Arc::new(vec![text_field("COUNT(*)")]);
        let mut encoder = DataRowEncoder::new(fields.clone());
        encoder.encode_field(&count.to_string())?;
        let data_rows = vec![encoder.finish()?];
        Ok(Response::Query(query_response(fields, data_rows)))
    }

    /// Fetch all rows for a table from Kvrocks, deserialize into Row maps.
    async fn fetch_rows(
        &self,
        db_id: u32,
        table_id: u32,
        schema_cols: &[ColumnInfo],
    ) -> PgWireResult<Vec<Row>> {
        let pattern = keys::table_scan_pattern(db_id, table_id)
            .map_err(|e| user_error(e.to_string()))?;
        let all_keys = self.kvrocks.scan_keys(&pattern).await.map_err(to_err)?;

        if all_keys.is_empty() {
            return Ok(Vec::new());
        }

        // Single MGET for all keys at once
        let all_vals = self.kvrocks.mget(&all_keys).await.map_err(to_err)?;

        // Build key -> value lookup
        let mut key_val_map: HashMap<String, Vec<u8>> = HashMap::with_capacity(all_keys.len());
        for (key, val) in all_keys.iter().zip(all_vals.into_iter()) {
            if let Some(v) = val {
                key_val_map.insert(key.clone(), v);
            }
        }

        // Collect unique row_ids
        let mut row_ids: Vec<String> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for key in &all_keys {
            let parts: Vec<&str> = key.split(':').collect();
            if parts.len() == 4 && seen.insert(parts[2].to_string()) {
                row_ids.push(parts[2].to_string());
            }
        }

        // Build rows by constructing expected keys per row
        let mut rows = Vec::with_capacity(row_ids.len());
        for row_id_str in &row_ids {
            let row_id_raw = keys::decode_base62(row_id_str)
                .map_err(|e| user_error(e.to_string()))?;

            let mut col_values: Vec<(&ColumnInfo, Option<Vec<u8>>)> = Vec::new();
            for col in schema_cols {
                let key = keys::build_key(db_id, table_id, row_id_raw, col.id)
                    .map_err(|e| user_error(e.to_string()))?;
                let val = key_val_map.get(&key).cloned();
                col_values.push((col, val));
            }
            rows.push(bytes_to_row(&col_values));
        }

        Ok(rows)
    }

    /// Convert typed rows to a pgwire query response.
    fn rows_to_response<'a>(
        &self,
        rows: &[Row],
        projected_cols: &[ColumnInfo],
    ) -> PgWireResult<Response<'a>> {
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

        Ok(Response::Query(query_response(fields, data_rows)))
    }

    /// Build pgwire response by evaluating expression projections per row.
    fn expr_rows_to_response<'a>(
        &self,
        rows: &[Row],
        projected_exprs: &[(String, Expr)],
        schema_cols: &[ColumnInfo],
    ) -> PgWireResult<Response<'a>> {
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

        Ok(Response::Query(query_response(fields, data_rows)))
    }

    /// Handle aggregate queries (COUNT, SUM, AVG, MIN, MAX) with optional GROUP BY.
    fn handle_aggregate_result<'a>(
        &self,
        projection: &[SelectItem],
        rows: &[Row],
        schema_cols: &[ColumnInfo],
        group_by: &sqlparser::ast::GroupByExpr,
        order_by: Option<&sqlparser::ast::OrderBy>,
        limit: Option<usize>,
        offset: usize,
    ) -> PgWireResult<Response<'a>> {
        let group_by_cols = extract_group_by_cols(group_by);

        if group_by_cols.is_empty() {
            // Single aggregate over all rows
            let agg_row = compute_aggregates(projection, rows, schema_cols)?;
            let fields = Arc::new(
                agg_row.iter().map(|(name, _)| text_field(name)).collect::<Vec<_>>(),
            );
            let mut encoder = DataRowEncoder::new(fields.clone());
            for (_, val) in &agg_row {
                match val {
                    TypedValue::Null => encoder.encode_field(&None::<&str>)?,
                    tv => encoder.encode_field(&tv.to_string())?,
                }
            }
            let data_rows = vec![encoder.finish()?];
            Ok(Response::Query(query_response(fields, data_rows)))
        } else {
            // GROUP BY: bucket rows
            let mut buckets: HashMap<Vec<String>, Vec<&Row>> = HashMap::new();
            for row in rows {
                let key: Vec<String> = group_by_cols
                    .iter()
                    .map(|col| {
                        let val = lookup_row_value(row, col);
                        if val.is_null() { "NULL".to_string() } else { val.to_string() }
                    })
                    .collect();
                buckets.entry(key).or_default().push(row);
            }

            let field_names: Vec<String> = projection.iter().map(|item| {
                match item {
                    SelectItem::UnnamedExpr(Expr::Identifier(ident)) => ident.value.clone(),
                    SelectItem::UnnamedExpr(Expr::CompoundIdentifier(parts)) => {
                        parts.last().map(|p| p.value.clone()).unwrap_or_default()
                    }
                    SelectItem::UnnamedExpr(Expr::Function(f)) => f.to_string(),
                    SelectItem::ExprWithAlias { alias, .. } => alias.value.clone(),
                    _ => "?column?".to_string(),
                }
            }).collect();

            // Compute aggregate rows as Row maps (for ORDER BY support)
            let mut agg_rows: Vec<Row> = Vec::new();
            for (_group_key, bucket_rows) in &buckets {
                let owned_rows: Vec<Row> = bucket_rows.iter().map(|r| (*r).clone()).collect();
                let agg_values = compute_aggregates(projection, &owned_rows, schema_cols)?;
                let mut row = Row::new();
                for (name, val) in &agg_values {
                    row.insert(name.to_lowercase(), val.clone());
                }
                agg_rows.push(row);
            }

            // Apply ORDER BY
            if let Some(ob) = order_by {
                sort_rows(&mut agg_rows, &ob.exprs, schema_cols)?;
            }

            // Apply OFFSET and LIMIT
            let paged: Vec<Row> = if let Some(lim) = limit {
                agg_rows.into_iter().skip(offset).take(lim).collect()
            } else {
                agg_rows.into_iter().skip(offset).collect()
            };

            let fields = Arc::new(
                field_names.iter().map(|n| text_field(n)).collect::<Vec<_>>(),
            );

            let mut data_rows: Vec<DataRow> = Vec::new();
            for row in &paged {
                let mut encoder = DataRowEncoder::new(fields.clone());
                for name in &field_names {
                    let val = lookup_row_value(row, &name.to_lowercase());
                    match val {
                        TypedValue::Null => encoder.encode_field(&None::<&str>)?,
                        tv => encoder.encode_field(&tv.to_string())?,
                    }
                }
                data_rows.push(encoder.finish()?);
            }

            Ok(Response::Query(query_response(fields, data_rows)))
        }
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

/// Check if projection is exactly COUNT(*) with no other expressions.
fn is_count_star_only(items: &[SelectItem]) -> bool {
    if items.len() != 1 {
        return false;
    }
    match &items[0] {
        SelectItem::UnnamedExpr(Expr::Function(f)) | SelectItem::ExprWithAlias { expr: Expr::Function(f), .. } => {
            let name = f.name.to_string().to_uppercase();
            if name != "COUNT" {
                return false;
            }
            match &f.args {
                sqlparser::ast::FunctionArguments::List(list) => {
                    list.args.len() == 1 && matches!(&list.args[0], FunctionArg::Unnamed(FunctionArgExpr::Wildcard))
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
                let name = parts.last().map(|p| p.value.to_lowercase()).unwrap_or_default();
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
        SelectItem::UnnamedExpr(Expr::BinaryOp { .. })
        | SelectItem::UnnamedExpr(Expr::Nested(..)) => true,
        SelectItem::ExprWithAlias { expr: Expr::BinaryOp { .. }, .. }
        | SelectItem::ExprWithAlias { expr: Expr::Nested(..), .. } => true,
        _ => false,
    })
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
                let name = f.name.to_string().to_uppercase();
                if matches!(name.as_str(), "COUNT" | "SUM" | "AVG" | "MIN" | "MAX") {
                    aggs.push(f);
                }
            }
            SelectItem::ExprWithAlias { expr: Expr::Function(f), .. } => {
                let name = f.name.to_string().to_uppercase();
                if matches!(name.as_str(), "COUNT" | "SUM" | "AVG" | "MIN" | "MAX") {
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
        sqlparser::ast::GroupByExpr::Expressions(exprs, _) => {
            exprs.iter().filter_map(|e| match e {
                Expr::Identifier(ident) => Some(ident.value.to_lowercase()),
                Expr::CompoundIdentifier(parts) => {
                    Some(parts.iter().map(|p| p.value.to_lowercase()).collect::<Vec<_>>().join("."))
                }
                _ => None,
            }).collect()
        }
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
                let full = parts.iter().map(|p| p.value.to_lowercase()).collect::<Vec<_>>().join(".");
                let display = parts.last().map(|p| p.value.clone()).unwrap_or_default();
                let val = rows
                    .first()
                    .map(|r| lookup_row_value(r, &full))
                    .unwrap_or(TypedValue::Null);
                result.push((display, val));
            }
            SelectItem::UnnamedExpr(Expr::Function(f))
            | SelectItem::ExprWithAlias { expr: Expr::Function(f), .. } => {
                let name = f.name.to_string().to_uppercase();
                let alias = match item {
                    SelectItem::ExprWithAlias { alias, .. } => alias.value.clone(),
                    _ => f.to_string(),
                };

                let arg_expr = extract_function_arg_expr(f);
                let val = compute_single_aggregate_expr(&name, arg_expr.as_ref(), rows, schema_cols)?;
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
            let expr = arg_expr.ok_or_else(|| user_error("SUM requires an argument".to_string()))?;
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
            let expr = arg_expr.ok_or_else(|| user_error("AVG requires an argument".to_string()))?;
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
            let expr = arg_expr.ok_or_else(|| user_error("MIN requires an argument".to_string()))?;
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
            let expr = arg_expr.ok_or_else(|| user_error("MAX requires an argument".to_string()))?;
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
    _schema_cols: &[ColumnInfo],
) -> PgWireResult<()> {
    rows.sort_by(|a, b| {
        for ob in order_by {
            let col_name = match &ob.expr {
                Expr::Identifier(ident) => ident.value.to_lowercase(),
                Expr::CompoundIdentifier(parts) => {
                    parts.iter().map(|p| p.value.to_lowercase()).collect::<Vec<_>>().join(".")
                }
                _ => continue,
            };
            let a_val = lookup_row_value(a, &col_name);
            let b_val = lookup_row_value(b, &col_name);

            let cmp = a_val.cmp_order(&b_val);
            let cmp = if ob.asc == Some(false) { cmp.reverse() } else { cmp };
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
            // Try fuzzy match: find key ending with .col_name
            row.iter().find(|(k, _)| k.ends_with(&format!(".{}", col_name))).map(|(_, v)| v)
        })
        .cloned()
        .unwrap_or(TypedValue::Null)
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
        _ => Err(user_error(
            "unsupported ON clause expression".to_string(),
        )),
    }
}

/// Extract column reference, preserving qualifier if present.
/// Returns "alias.col" for CompoundIdentifier, "col" for bare Identifier.
fn extract_column_name(expr: &Expr) -> PgWireResult<String> {
    match expr {
        Expr::Identifier(ident) => Ok(ident.value.to_lowercase()),
        Expr::CompoundIdentifier(parts) => {
            Ok(parts.iter().map(|p| p.value.to_lowercase()).collect::<Vec<_>>().join("."))
        }
        _ => Err(user_error("ON condition must reference column names".to_string())),
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
                let qualified = parts.iter().map(|p| p.value.to_lowercase()).collect::<Vec<_>>().join(".");
                let display = parts.last().map(|p| p.value.to_lowercase()).unwrap_or_default();
                result.push((display, qualified));
            }
            SelectItem::UnnamedExpr(Expr::Identifier(ident)) => {
                let name = ident.value.to_lowercase();
                let key = find_column_key(&name, combined_schema)?;
                result.push((name, key));
            }
            SelectItem::ExprWithAlias { expr, alias } => {
                let key = match expr {
                    Expr::CompoundIdentifier(parts) => {
                        parts.iter().map(|p| p.value.to_lowercase()).collect::<Vec<_>>().join(".")
                    }
                    Expr::Identifier(ident) => {
                        find_column_key(&ident.value.to_lowercase(), combined_schema)?
                    }
                    _ => return Err(user_error("unsupported expression in JOIN projection".to_string())),
                };
                result.push((alias.value.clone(), key));
            }
            _ => {}
        }
    }

    Ok(result)
}

/// Find the fully qualified column key for an unqualified column name.
fn find_column_key(
    col_name: &str,
    combined_schema: &[ColumnInfo],
) -> PgWireResult<String> {
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
