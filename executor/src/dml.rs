use crate::index::{build_index_entries_for_row, build_index_keys_for_row};
use crate::row_format::{encode_row, RowView};
use crate::storage::StorageBackend;
use anyhow::{bail, Context, Result};
use keys::{build_row_key, encode_row_id};
use schema::{
    ColumnInfo, ForeignKeyConstraintInfo, Neo4jConnection, TableConstraintsInfo,
    UniqueConstraintInfo,
};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use std::collections::{HashMap, HashSet};
use tracing::{debug, info};
use wal::{WalFile, WalPayload, WalPayloadV1, WalStatus};

pub struct WriteEngine<'a> {
    storage: &'a dyn StorageBackend,
    neo4j: &'a Neo4jConnection,
    wal: &'a mut WalFile,
}

impl<'a> WriteEngine<'a> {
    pub fn new(
        storage: &'a dyn StorageBackend,
        neo4j: &'a Neo4jConnection,
        wal: &'a mut WalFile,
    ) -> Self {
        Self {
            storage,
            neo4j,
            wal,
        }
    }

    /// INSERT INTO table VALUES (...)
    /// Returns the number of rows inserted (always 1).
    pub async fn insert(
        &mut self,
        db_name: &str,
        table_name: &str,
        column_names: &[String],
        values: &[Vec<u8>],
    ) -> Result<u64> {
        let db = self
            .neo4j
            .get_database(db_name)
            .await?
            .context(format!("database '{}' does not exist", db_name))?;
        let table = self
            .neo4j
            .get_table(db_name, table_name)
            .await?
            .context(format!(
                "table '{}' does not exist in database '{}'",
                table_name, db_name
            ))?;
        let schema_cols = self.neo4j.describe_table(db_name, table_name).await?;
        let constraints = self
            .neo4j
            .get_table_constraints(db_name, table_name)
            .await?;

        // Resolve which columns are being inserted
        let resolved = resolve_columns(&schema_cols, column_names, values)?;
        let resolved =
            apply_defaults_for_insert(self.neo4j, db_name, &schema_cols, &constraints, resolved)
                .await?;

        // Validate NOT NULL constraints after DEFAULT expansion, including PK columns.
        let resolved_columns: Vec<(u32, Vec<u8>)> = resolved
            .iter()
            .map(|(col, value)| (col.id, value.clone()))
            .collect();
        validate_not_null_columns(&schema_cols, &resolved_columns)?;
        validate_check_constraints(&schema_cols, &constraints, &resolved)?;

        let pk_key =
            build_primary_key_mapping_key(db.id, table.id, &schema_cols, &constraints, &resolved)?;
        self.check_constraint_key_available(&pk_key, None).await?;
        self.validate_unique_constraints(
            db.id,
            table.id,
            &schema_cols,
            &constraints,
            &resolved,
            None,
        )
        .await?;
        self.validate_foreign_keys(db_name, &schema_cols, &constraints, &resolved)
            .await?;

        // Generate row_id
        let row_id_raw: u64 = rand::random();
        let row_id = encode_row_id(row_id_raw)?;

        // Build WAL payload
        let wal_columns: Vec<(u32, Vec<u8>)> = resolved
            .iter()
            .map(|(col, val)| (col.id, val.clone()))
            .collect();

        let payload = WalPayload::V1(WalPayloadV1::Insert {
            db_id: db.id,
            table_id: table.id,
            row_id: row_id.clone(),
            columns: wal_columns.clone(),
        });

        // Step 1: WAL append (Pending)
        let (entry_id, file_offset) = self.wal.append(payload)?;
        debug!(entry_id, "WAL entry appended for INSERT");

        // Step 2: store constraint lookup mappings
        let mut constraint_pairs = vec![(pk_key, row_id.as_bytes().to_vec())];
        constraint_pairs.extend(build_unique_constraint_pairs(
            db.id,
            table.id,
            &schema_cols,
            &constraints,
            &resolved,
            &row_id,
        )?);
        self.storage.mset(&constraint_pairs).await?;

        // Step 3: WAL status -> Neo4jCommitted
        self.wal
            .update_status(file_offset, WalStatus::Neo4jCommitted)?;

        // Step 4: Storage write (row + index entries in one batch)
        let row_key = build_row_key(db.id, table.id, row_id_raw)?;
        let row_value = serialize_columns(&wal_columns);
        let mut pairs = vec![(row_key, row_value)];

        // Write index entries for any indexed columns
        let indexed_cols = get_indexed_cols(&schema_cols);
        if !indexed_cols.is_empty() {
            pairs.extend(build_index_entries_for_row(
                db.id,
                table.id,
                row_id_raw,
                &wal_columns,
                &indexed_cols,
            ));
        }

        self.storage.mset(&pairs).await?;

        // Step 5: WAL status -> Committed
        self.wal.update_status(file_offset, WalStatus::Committed)?;

        info!(db_name, table_name, %row_id, "row inserted");
        Ok(1)
    }

    /// UPDATE table SET col=val WHERE pk_col = pk_val
    /// Returns the number of rows updated.
    pub async fn update(
        &mut self,
        db_name: &str,
        table_name: &str,
        set_pairs: &[(String, Vec<u8>)],
        where_col: &str,
        where_val: &str,
    ) -> Result<u64> {
        let db = self
            .neo4j
            .get_database(db_name)
            .await?
            .context(format!("database '{}' does not exist", db_name))?;
        let table = self
            .neo4j
            .get_table(db_name, table_name)
            .await?
            .context(format!("table '{}' does not exist", table_name))?;
        let schema_cols = self.neo4j.describe_table(db_name, table_name).await?;
        let constraints = self
            .neo4j
            .get_table_constraints(db_name, table_name)
            .await?;

        // Find matching row_ids via WHERE clause
        let row_ids = self
            .find_rows_by_predicate(db.id, table.id, &schema_cols, where_col, where_val)
            .await?;

        if row_ids.is_empty() {
            return Ok(0);
        }

        let mut updated = 0u64;

        for (row_id_str, row_id_raw) in &row_ids {
            // Resolve SET columns
            let mut wal_columns: Vec<(u32, Vec<u8>)> = Vec::new();
            for (col_name, val) in set_pairs {
                let col = schema_cols
                    .iter()
                    .find(|c| c.name == *col_name)
                    .context(format!("column '{}' not found", col_name))?;
                wal_columns.push((col.id, val.clone()));
            }

            let payload = WalPayload::V1(WalPayloadV1::Update {
                db_id: db.id,
                table_id: table.id,
                row_id: row_id_str.clone(),
                columns: wal_columns.clone(),
            });

            // WAL append
            let (_entry_id, file_offset) = self.wal.append(payload)?;

            self.wal
                .update_status(file_offset, WalStatus::Neo4jCommitted)?;

            // Read-modify-write: merge updated columns into existing row
            let row_key = build_row_key(db.id, table.id, *row_id_raw)?;
            let existing_data = self.storage.get(&row_key).await?;
            let mut existing = if let Some(ref data) = existing_data {
                deserialize_columns(data)
            } else {
                Vec::new()
            };
            let existing_before_update = existing.clone();

            let old_pk_key = build_primary_key_mapping_key_from_columns(
                db.id,
                table.id,
                &schema_cols,
                &constraints,
                &existing_before_update,
            )?;
            let old_unique_keys = build_unique_constraint_keys_from_columns(
                db.id,
                table.id,
                &schema_cols,
                &constraints,
                &existing_before_update,
            )?;

            // Delete old index entries, then build new ones after merge
            let indexed_cols = get_indexed_cols(&schema_cols);
            if !indexed_cols.is_empty() {
                let old_keys = build_index_keys_for_row(
                    db.id,
                    table.id,
                    *row_id_raw,
                    &existing,
                    &indexed_cols,
                );
                if !old_keys.is_empty() {
                    self.storage.del(&old_keys).await?;
                }
            }

            for (col_id, val) in &wal_columns {
                if let Some(entry) = existing.iter_mut().find(|(id, _)| id == col_id) {
                    entry.1 = val.clone();
                } else {
                    existing.push((*col_id, val.clone()));
                }
            }

            validate_not_null_columns(&schema_cols, &existing)?;
            validate_check_constraints_columns(&schema_cols, &constraints, &existing)?;

            let new_pk_key = build_primary_key_mapping_key_from_columns(
                db.id,
                table.id,
                &schema_cols,
                &constraints,
                &existing,
            )?;
            if new_pk_key != old_pk_key {
                self.validate_no_referencing_rows(
                    db_name,
                    table_name,
                    &schema_cols,
                    &constraints,
                    &existing_before_update,
                )
                .await?;
                self.check_constraint_key_available(&new_pk_key, Some(row_id_str.as_str()))
                    .await?;
            }

            self.validate_unique_constraints_from_columns(
                db.id,
                table.id,
                &schema_cols,
                &constraints,
                &existing,
                Some(row_id_str.as_str()),
            )
            .await?;
            self.validate_foreign_keys_from_columns(db_name, &schema_cols, &constraints, &existing)
                .await?;

            let row_value = serialize_columns(&existing);
            let mut pairs = vec![(row_key, row_value)];

            // Write new index entries
            if !indexed_cols.is_empty() {
                pairs.extend(build_index_entries_for_row(
                    db.id,
                    table.id,
                    *row_id_raw,
                    &existing,
                    &indexed_cols,
                ));
            }

            self.storage.mset(&pairs).await?;

            let mut keys_to_delete = old_unique_keys;
            keys_to_delete.push(old_pk_key);
            self.storage.del(&keys_to_delete).await?;
            let mut constraint_pairs = vec![(new_pk_key, row_id_str.as_bytes().to_vec())];
            constraint_pairs.extend(build_unique_constraint_pairs_from_columns(
                db.id,
                table.id,
                &schema_cols,
                &constraints,
                &existing,
                row_id_str,
            )?);
            self.storage.mset(&constraint_pairs).await?;

            // WAL -> Committed
            self.wal.update_status(file_offset, WalStatus::Committed)?;
            updated += 1;
        }

        info!(db_name, table_name, updated, "rows updated");
        Ok(updated)
    }

    /// DELETE FROM table WHERE pk_col = pk_val
    /// Returns the number of rows deleted.
    pub async fn delete(
        &mut self,
        db_name: &str,
        table_name: &str,
        where_col: &str,
        where_val: &str,
    ) -> Result<u64> {
        let db = self
            .neo4j
            .get_database(db_name)
            .await?
            .context(format!("database '{}' does not exist", db_name))?;
        let table = self
            .neo4j
            .get_table(db_name, table_name)
            .await?
            .context(format!("table '{}' does not exist", table_name))?;
        let schema_cols = self.neo4j.describe_table(db_name, table_name).await?;
        let constraints = self
            .neo4j
            .get_table_constraints(db_name, table_name)
            .await?;

        let row_ids = self
            .find_rows_by_predicate(db.id, table.id, &schema_cols, where_col, where_val)
            .await?;

        if row_ids.is_empty() {
            return Ok(0);
        }

        let col_ids: Vec<u32> = schema_cols.iter().map(|c| c.id).collect();
        let mut deleted = 0u64;

        for (row_id_str, row_id_raw) in &row_ids {
            let payload = WalPayload::V1(WalPayloadV1::Delete {
                db_id: db.id,
                table_id: table.id,
                row_id: row_id_str.clone(),
                col_ids: col_ids.clone(),
            });

            let (_entry_id, file_offset) = self.wal.append(payload)?;

            // Delete RowMeta if exists
            self.neo4j
                .graph()
                .run(
                    neo4rs::query("MATCH (m:RowMeta {row_id: $row_id}) DELETE m")
                        .param("row_id", row_id_str.as_str()),
                )
                .await
                .ok();

            self.wal
                .update_status(file_offset, WalStatus::Neo4jCommitted)?;

            // Read row to get column values for index cleanup
            let row_key = build_row_key(db.id, table.id, *row_id_raw)?;
            let mut existing_row = Vec::new();
            if let Some(data) = self.storage.get(&row_key).await? {
                existing_row = deserialize_columns(&data);
            }
            self.validate_no_referencing_rows(
                db_name,
                table_name,
                &schema_cols,
                &constraints,
                &existing_row,
            )
            .await?;
            let indexed_cols = get_indexed_cols(&schema_cols);
            if !indexed_cols.is_empty() {
                if !existing_row.is_empty() {
                    let old_keys = build_index_keys_for_row(
                        db.id,
                        table.id,
                        *row_id_raw,
                        &existing_row,
                        &indexed_cols,
                    );
                    if !old_keys.is_empty() {
                        self.storage.del(&old_keys).await?;
                    }
                }
            }

            // Delete the row key
            self.storage.del(&[row_key]).await?;

            let mut keys_to_delete = build_unique_constraint_keys_from_columns(
                db.id,
                table.id,
                &schema_cols,
                &constraints,
                &existing_row,
            )?;
            keys_to_delete.push(build_primary_key_mapping_key_from_columns(
                db.id,
                table.id,
                &schema_cols,
                &constraints,
                &existing_row,
            )?);
            self.storage.del(&keys_to_delete).await?;

            self.wal.update_status(file_offset, WalStatus::Committed)?;
            deleted += 1;
        }

        info!(db_name, table_name, deleted, "rows deleted");
        Ok(deleted)
    }

    /// Find row_ids matching a simple WHERE col = val predicate.
    /// For PK columns: uses pk:{db}:{tbl}:{val} -> row_id mapping.
    /// For non-PK columns: scans all row keys (M3 scope -- no indexes yet).
    async fn find_rows_by_predicate(
        &self,
        db_id: u32,
        table_id: u32,
        schema_cols: &[ColumnInfo],
        where_col: &str,
        where_val: &str,
    ) -> Result<Vec<(String, u64)>> {
        let col = schema_cols
            .iter()
            .find(|c| c.name == where_col)
            .context(format!("column '{}' not found in WHERE clause", where_col))?;

        let single_pk = schema_cols
            .iter()
            .filter(|candidate| candidate.is_pk)
            .count()
            == 1;
        if col.is_pk && single_pk {
            // PK lookup via pk: key
            let pk_key = format!(
                "pk:{}:{}:{}",
                db_id,
                table_id,
                encode_constraint_values(&[where_val.as_bytes().to_vec()])
            );
            if let Some(row_id_bytes) = self.storage.get(&pk_key).await? {
                let row_id_str =
                    String::from_utf8(row_id_bytes).context("invalid row_id encoding")?;
                let row_id_raw = keys::decode_base62(&row_id_str)?;
                return Ok(vec![(row_id_str, row_id_raw)]);
            }
            return Ok(Vec::new());
        }

        // Non-PK: scan all rows (row-oriented) and check the target column
        let prefix = keys::table_scan_prefix(db_id, table_id)?;
        let mut matches = Vec::new();

        let all_kv = self.storage.scan_prefix(&prefix).await?;
        for (key, row_data) in &all_kv {
            if key.starts_with("pk:") {
                continue;
            }
            // Use RowView for zero-copy column lookup when possible
            let val_bytes = if let Some(view) = RowView::new(row_data) {
                view.find_col(col.id).map(|b| b.to_vec())
            } else {
                let cols = deserialize_columns(row_data);
                cols.into_iter()
                    .find(|(id, _)| *id == col.id)
                    .map(|(_, v)| v)
            };
            if let Some(val_bytes) = val_bytes {
                let val_str = String::from_utf8_lossy(&val_bytes);
                if val_str == where_val {
                    let parts: Vec<&str> = key.split(':').collect();
                    if parts.len() == 3 {
                        let row_id_raw = keys::decode_base62(parts[2])?;
                        matches.push((parts[2].to_string(), row_id_raw));
                    }
                }
            }
        }

        Ok(matches)
    }

    async fn check_constraint_key_available(
        &self,
        key: &str,
        existing_row_id: Option<&str>,
    ) -> Result<()> {
        if let Some(row_id) = self.storage.get(key).await? {
            let existing = String::from_utf8(row_id).context("invalid row id encoding")?;
            if existing_row_id != Some(existing.as_str()) {
                bail!("duplicate constraint value");
            }
        }
        Ok(())
    }

    async fn validate_unique_constraints(
        &self,
        db_id: u32,
        table_id: u32,
        schema_cols: &[ColumnInfo],
        constraints: &TableConstraintsInfo,
        resolved: &[(ColumnInfo, Vec<u8>)],
        existing_row_id: Option<&str>,
    ) -> Result<()> {
        for unique in &constraints.unique_constraints {
            if let Some(key) =
                unique_constraint_key_from_resolved(db_id, table_id, schema_cols, unique, resolved)?
            {
                self.check_constraint_key_available(&key, existing_row_id)
                    .await?;
            }
        }
        Ok(())
    }

    async fn validate_unique_constraints_from_columns(
        &self,
        db_id: u32,
        table_id: u32,
        schema_cols: &[ColumnInfo],
        constraints: &TableConstraintsInfo,
        columns: &[(u32, Vec<u8>)],
        existing_row_id: Option<&str>,
    ) -> Result<()> {
        for unique in &constraints.unique_constraints {
            if let Some(key) =
                unique_constraint_key_from_columns(db_id, table_id, schema_cols, unique, columns)?
            {
                self.check_constraint_key_available(&key, existing_row_id)
                    .await?;
            }
        }
        Ok(())
    }

    async fn validate_foreign_keys(
        &self,
        db_name: &str,
        schema_cols: &[ColumnInfo],
        constraints: &TableConstraintsInfo,
        resolved: &[(ColumnInfo, Vec<u8>)],
    ) -> Result<()> {
        for fk in &constraints.foreign_keys {
            let Some(values) =
                maybe_constraint_values_from_resolved(schema_cols, &fk.columns, resolved)?
            else {
                continue;
            };
            self.ensure_foreign_key_target_exists(db_name, fk, &values)
                .await?;
        }
        Ok(())
    }

    async fn validate_foreign_keys_from_columns(
        &self,
        db_name: &str,
        schema_cols: &[ColumnInfo],
        constraints: &TableConstraintsInfo,
        columns: &[(u32, Vec<u8>)],
    ) -> Result<()> {
        for fk in &constraints.foreign_keys {
            let Some(values) =
                maybe_constraint_values_from_columns(schema_cols, &fk.columns, columns)?
            else {
                continue;
            };
            self.ensure_foreign_key_target_exists(db_name, fk, &values)
                .await?;
        }
        Ok(())
    }

    async fn ensure_foreign_key_target_exists(
        &self,
        db_name: &str,
        fk: &ForeignKeyConstraintInfo,
        values: &[Vec<u8>],
    ) -> Result<()> {
        let ref_db_name = fk.ref_db_name.as_deref().unwrap_or(db_name);
        let ref_db = self
            .neo4j
            .get_database(ref_db_name)
            .await?
            .context(format!("database '{}' does not exist", ref_db_name))?;
        let ref_table = self
            .neo4j
            .get_table(ref_db_name, &fk.ref_table_name)
            .await?
            .context(format!(
                "referenced table '{}.{}' does not exist",
                ref_db_name, fk.ref_table_name
            ))?;
        let ref_key = format!(
            "pk:{}:{}:{}",
            ref_db.id,
            ref_table.id,
            encode_constraint_values(values)
        );
        if !self.storage.exists(&ref_key).await? {
            bail!(
                "FOREIGN KEY '{}' violation: referenced row is missing",
                fk.name
            );
        }
        Ok(())
    }

    async fn validate_no_referencing_rows(
        &self,
        db_name: &str,
        table_name: &str,
        schema_cols: &[ColumnInfo],
        constraints: &TableConstraintsInfo,
        columns: &[(u32, Vec<u8>)],
    ) -> Result<()> {
        let referenced_values = maybe_constraint_values_from_columns(
            schema_cols,
            &constraints.primary_key_columns,
            columns,
        )?
        .unwrap_or_default();
        if referenced_values.is_empty() {
            return Ok(());
        }

        let dbs = self.neo4j.show_databases().await?;
        for db in dbs {
            let tables = self.neo4j.show_tables(&db.name).await?;
            for table in tables {
                let table_constraints = self
                    .neo4j
                    .get_table_constraints(&db.name, &table.name)
                    .await?;
                for fk in &table_constraints.foreign_keys {
                    if fk.ref_table_name != table_name {
                        continue;
                    }
                    if fk.ref_db_name.as_deref().unwrap_or(db_name) != db_name {
                        continue;
                    }
                    if fk.ref_columns != constraints.primary_key_columns {
                        continue;
                    }
                    if self
                        .foreign_key_has_referencing_rows(
                            db.id,
                            table.id,
                            &db.name,
                            &table.name,
                            fk,
                            &referenced_values,
                        )
                        .await?
                    {
                        bail!(
                            "FOREIGN KEY '{}' prevents modifying referenced row",
                            fk.name
                        );
                    }
                }
            }
        }
        Ok(())
    }

    async fn foreign_key_has_referencing_rows(
        &self,
        db_id: u32,
        table_id: u32,
        db_name: &str,
        table_name: &str,
        fk: &ForeignKeyConstraintInfo,
        referenced_values: &[Vec<u8>],
    ) -> Result<bool> {
        let prefix = keys::table_scan_prefix(db_id, table_id)?;
        let schema_cols = self.neo4j.describe_table(db_name, table_name).await?;
        let fk_col_ids: Vec<u32> = fk
            .columns
            .iter()
            .map(|name| {
                schema_cols
                    .iter()
                    .find(|col| col.name == *name)
                    .map(|col| col.id)
                    .context(format!("FOREIGN KEY column '{}' not found", name))
            })
            .collect::<Result<_>>()?;

        for (key, row_data) in self.storage.scan_prefix(&prefix).await? {
            if key.starts_with("pk:") || key.starts_with("uk:") {
                continue;
            }
            let row_columns = deserialize_columns(&row_data);
            let candidate: Vec<Vec<u8>> = fk_col_ids
                .iter()
                .map(|col_id| {
                    row_columns
                        .iter()
                        .find(|(candidate_id, _)| candidate_id == col_id)
                        .map(|(_, value)| value.clone())
                        .unwrap_or_default()
                })
                .collect();
            if candidate == referenced_values {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// Filter schema columns to only indexed ones.
fn get_indexed_cols(schema_cols: &[ColumnInfo]) -> Vec<ColumnInfo> {
    schema_cols.iter().filter(|c| c.indexed).cloned().collect()
}

fn build_primary_key_mapping_key(
    db_id: u32,
    table_id: u32,
    schema_cols: &[ColumnInfo],
    constraints: &TableConstraintsInfo,
    resolved: &[(ColumnInfo, Vec<u8>)],
) -> Result<String> {
    let values =
        constraint_values_from_resolved(schema_cols, &constraints.primary_key_columns, resolved)?;
    Ok(format!(
        "pk:{}:{}:{}",
        db_id,
        table_id,
        encode_constraint_values(&values)
    ))
}

fn build_primary_key_mapping_key_from_columns(
    db_id: u32,
    table_id: u32,
    schema_cols: &[ColumnInfo],
    constraints: &TableConstraintsInfo,
    columns: &[(u32, Vec<u8>)],
) -> Result<String> {
    let values =
        constraint_values_from_columns(schema_cols, &constraints.primary_key_columns, columns)?;
    Ok(format!(
        "pk:{}:{}:{}",
        db_id,
        table_id,
        encode_constraint_values(&values)
    ))
}

fn build_unique_constraint_pairs(
    db_id: u32,
    table_id: u32,
    schema_cols: &[ColumnInfo],
    constraints: &TableConstraintsInfo,
    resolved: &[(ColumnInfo, Vec<u8>)],
    row_id: &str,
) -> Result<Vec<(String, Vec<u8>)>> {
    let mut pairs = Vec::new();
    for unique in &constraints.unique_constraints {
        if let Some(key) =
            unique_constraint_key_from_resolved(db_id, table_id, schema_cols, unique, resolved)?
        {
            pairs.push((key, row_id.as_bytes().to_vec()));
        }
    }
    Ok(pairs)
}

fn build_unique_constraint_pairs_from_columns(
    db_id: u32,
    table_id: u32,
    schema_cols: &[ColumnInfo],
    constraints: &TableConstraintsInfo,
    columns: &[(u32, Vec<u8>)],
    row_id: &str,
) -> Result<Vec<(String, Vec<u8>)>> {
    let mut pairs = Vec::new();
    for unique in &constraints.unique_constraints {
        if let Some(key) =
            unique_constraint_key_from_columns(db_id, table_id, schema_cols, unique, columns)?
        {
            pairs.push((key, row_id.as_bytes().to_vec()));
        }
    }
    Ok(pairs)
}

fn build_unique_constraint_keys_from_columns(
    db_id: u32,
    table_id: u32,
    schema_cols: &[ColumnInfo],
    constraints: &TableConstraintsInfo,
    columns: &[(u32, Vec<u8>)],
) -> Result<Vec<String>> {
    let mut keys = Vec::new();
    for unique in &constraints.unique_constraints {
        if let Some(key) =
            unique_constraint_key_from_columns(db_id, table_id, schema_cols, unique, columns)?
        {
            keys.push(key);
        }
    }
    Ok(keys)
}

fn unique_constraint_key_from_resolved(
    db_id: u32,
    table_id: u32,
    schema_cols: &[ColumnInfo],
    unique: &UniqueConstraintInfo,
    resolved: &[(ColumnInfo, Vec<u8>)],
) -> Result<Option<String>> {
    let values = maybe_constraint_values_from_resolved(schema_cols, &unique.columns, resolved)?;
    Ok(values.map(|vals| {
        format!(
            "uk:{}:{}:{}:{}",
            db_id,
            table_id,
            unique.name,
            encode_constraint_values(&vals)
        )
    }))
}

fn unique_constraint_key_from_columns(
    db_id: u32,
    table_id: u32,
    schema_cols: &[ColumnInfo],
    unique: &UniqueConstraintInfo,
    columns: &[(u32, Vec<u8>)],
) -> Result<Option<String>> {
    let values = maybe_constraint_values_from_columns(schema_cols, &unique.columns, columns)?;
    Ok(values.map(|vals| {
        format!(
            "uk:{}:{}:{}:{}",
            db_id,
            table_id,
            unique.name,
            encode_constraint_values(&vals)
        )
    }))
}

fn encode_constraint_values(values: &[Vec<u8>]) -> String {
    values
        .iter()
        .map(|value| {
            let mut out = String::with_capacity(value.len() * 2);
            for byte in value {
                use std::fmt::Write as _;
                let _ = write!(&mut out, "{byte:02x}");
            }
            out
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn constraint_values_from_resolved(
    schema_cols: &[ColumnInfo],
    columns: &[String],
    resolved: &[(ColumnInfo, Vec<u8>)],
) -> Result<Vec<Vec<u8>>> {
    columns
        .iter()
        .map(|column_name| {
            let col = schema_cols
                .iter()
                .find(|col| col.name == *column_name)
                .context(format!("constraint column '{}' not found", column_name))?;
            resolved
                .iter()
                .find(|(candidate, _)| candidate.id == col.id)
                .map(|(_, value)| value.clone())
                .context(format!(
                    "constraint column '{}' must have a value",
                    column_name
                ))
        })
        .collect()
}

fn constraint_values_from_columns(
    schema_cols: &[ColumnInfo],
    columns: &[String],
    values: &[(u32, Vec<u8>)],
) -> Result<Vec<Vec<u8>>> {
    columns
        .iter()
        .map(|column_name| {
            let col = schema_cols
                .iter()
                .find(|col| col.name == *column_name)
                .context(format!("constraint column '{}' not found", column_name))?;
            values
                .iter()
                .find(|(col_id, _)| *col_id == col.id)
                .map(|(_, value)| value.clone())
                .context(format!(
                    "constraint column '{}' must have a value",
                    column_name
                ))
        })
        .collect()
}

fn maybe_constraint_values_from_resolved(
    schema_cols: &[ColumnInfo],
    columns: &[String],
    resolved: &[(ColumnInfo, Vec<u8>)],
) -> Result<Option<Vec<Vec<u8>>>> {
    let values = constraint_values_from_resolved(schema_cols, columns, resolved)?;
    if values.iter().any(|value| value.is_empty()) {
        return Ok(None);
    }
    Ok(Some(values))
}

fn maybe_constraint_values_from_columns(
    schema_cols: &[ColumnInfo],
    columns: &[String],
    values: &[(u32, Vec<u8>)],
) -> Result<Option<Vec<Vec<u8>>>> {
    let values = constraint_values_from_columns(schema_cols, columns, values)?;
    if values.iter().any(|value| value.is_empty()) {
        return Ok(None);
    }
    Ok(Some(values))
}

/// Resolve column names to ColumnInfo + values.
/// If column_names is empty, assumes all columns in schema order.
fn resolve_columns(
    schema_cols: &[ColumnInfo],
    column_names: &[String],
    values: &[Vec<u8>],
) -> Result<Vec<(ColumnInfo, Vec<u8>)>> {
    if column_names.is_empty() {
        // All columns in schema order
        if values.len() != schema_cols.len() {
            bail!(
                "expected {} values, got {}",
                schema_cols.len(),
                values.len()
            );
        }
        Ok(schema_cols
            .iter()
            .zip(values.iter())
            .map(|(c, v)| (c.clone(), v.clone()))
            .collect())
    } else {
        if column_names.len() != values.len() {
            bail!(
                "column count ({}) does not match value count ({})",
                column_names.len(),
                values.len()
            );
        }
        let mut result = Vec::new();
        for (name, val) in column_names.iter().zip(values.iter()) {
            let col = schema_cols
                .iter()
                .find(|c| c.name == *name)
                .context(format!("column '{}' not found", name))?;
            result.push((col.clone(), val.clone()));
        }
        Ok(result)
    }
}

fn validate_not_null_columns(schema_cols: &[ColumnInfo], values: &[(u32, Vec<u8>)]) -> Result<()> {
    for col in schema_cols {
        if !col.nullable {
            let has_value = values
                .iter()
                .any(|(col_id, value)| *col_id == col.id && !value.is_empty());
            if !has_value {
                bail!("column '{}' cannot be NULL", col.name);
            }
        }
    }
    Ok(())
}

async fn apply_defaults_for_insert(
    neo4j: &Neo4jConnection,
    db_name: &str,
    schema_cols: &[ColumnInfo],
    constraints: &TableConstraintsInfo,
    mut resolved: Vec<(ColumnInfo, Vec<u8>)>,
) -> Result<Vec<(ColumnInfo, Vec<u8>)>> {
    let present: HashSet<u32> = resolved.iter().map(|(col, _)| col.id).collect();
    for col in schema_cols {
        if present.contains(&col.id) {
            continue;
        }
        if let Some(default_expr) = constraints.defaults.get(&col.name) {
            let bytes = evaluate_default_expr(default_expr)?;
            resolved.push((col.clone(), bytes));
        }
    }

    for fk in &constraints.foreign_keys {
        let ref_db_name = fk.ref_db_name.as_deref().unwrap_or(db_name);
        let ref_constraints = neo4j
            .get_table_constraints(ref_db_name, &fk.ref_table_name)
            .await?;
        if ref_constraints.primary_key_columns != fk.ref_columns {
            bail!(
                "FOREIGN KEY '{}' currently supports references to the referenced table primary key only",
                fk.name
            );
        }
    }

    Ok(resolved)
}

fn validate_check_constraints(
    schema_cols: &[ColumnInfo],
    constraints: &TableConstraintsInfo,
    resolved: &[(ColumnInfo, Vec<u8>)],
) -> Result<()> {
    let columns: Vec<(u32, Vec<u8>)> = resolved
        .iter()
        .map(|(col, value)| (col.id, value.clone()))
        .collect();
    validate_check_constraints_columns(schema_cols, constraints, &columns)
}

fn validate_check_constraints_columns(
    schema_cols: &[ColumnInfo],
    constraints: &TableConstraintsInfo,
    columns: &[(u32, Vec<u8>)],
) -> Result<()> {
    let row = row_from_columns(schema_cols, columns);
    for check in &constraints.check_constraints {
        let expr = parse_expression(&check.expr_sql)?;
        if !crate::filter::evaluate_predicate(&expr, &row, schema_cols) {
            bail!("CHECK constraint '{}' violated", check.name);
        }
    }
    Ok(())
}

fn row_from_columns(schema_cols: &[ColumnInfo], columns: &[(u32, Vec<u8>)]) -> crate::filter::Row {
    let mut row = HashMap::new();
    for col in schema_cols {
        let value = columns
            .iter()
            .find(|(col_id, _)| *col_id == col.id)
            .map(|(_, value)| value.clone())
            .unwrap_or_default();
        row.insert(
            col.name.to_lowercase(),
            crate::TypedValue::from_bytes(&value, &col.data_type),
        );
    }
    row
}

fn evaluate_default_expr(expr_sql: &str) -> Result<Vec<u8>> {
    let expr = parse_expression(expr_sql)?;
    let row = HashMap::new();
    let schema_cols = Vec::new();
    let value = crate::resolve_value(&expr, &row, &schema_cols);
    Ok(match value {
        crate::TypedValue::Null => Vec::new(),
        crate::TypedValue::Text(text) if text.is_empty() => {
            crate::typed_value::EMPTY_TEXT_SENTINEL.as_bytes().to_vec()
        }
        crate::TypedValue::Text(text) => text.into_bytes(),
        crate::TypedValue::Int(i) => i.to_string().into_bytes(),
        crate::TypedValue::Float(f) => f.to_string().into_bytes(),
        crate::TypedValue::Boolean(flag) => {
            if flag {
                b"true".to_vec()
            } else {
                b"false".to_vec()
            }
        }
    })
}

fn parse_expression(expr_sql: &str) -> Result<sqlparser::ast::Expr> {
    let sql = format!("SELECT {expr_sql}");
    let stmts = Parser::parse_sql(&PostgreSqlDialect {}, &sql)?;
    let stmt = stmts
        .into_iter()
        .next()
        .context("expression parser returned no statement")?;
    match stmt {
        sqlparser::ast::Statement::Query(query) => match *query.body {
            sqlparser::ast::SetExpr::Select(select) => {
                let item = select
                    .projection
                    .into_iter()
                    .next()
                    .context("expression parser returned empty projection")?;
                match item {
                    sqlparser::ast::SelectItem::UnnamedExpr(expr) => Ok(expr),
                    sqlparser::ast::SelectItem::ExprWithAlias { expr, .. } => Ok(expr),
                    _ => bail!("unsupported expression shape"),
                }
            }
            _ => bail!("unsupported expression shape"),
        },
        _ => bail!("unsupported expression shape"),
    }
}

/// Serialize column data into the zero-copy binary row format.
pub fn serialize_columns(columns: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let borrowed: Vec<(u32, &[u8])> = columns.iter().map(|(id, v)| (*id, v.as_slice())).collect();
    encode_row(&borrowed)
}

/// Deserialize column data from either the new row format or legacy bincode.
pub fn deserialize_columns(data: &[u8]) -> Vec<(u32, Vec<u8>)> {
    if let Some(view) = RowView::new(data) {
        view.to_owned_columns()
    } else {
        // Legacy bincode fallback
        bincode::deserialize(data).unwrap_or_default()
    }
}

/// Replay WAL entries to storage (called on startup for Neo4jCommitted entries).
pub async fn replay_wal_entries(
    storage: &dyn StorageBackend,
    neo4j: &Neo4jConnection,
    entries: &[wal::WalEntry],
    wal_file: &WalFile,
) -> Result<()> {
    let mut idx_cache: HashMap<(u32, u32), Vec<ColumnInfo>> = HashMap::new();
    let mut schema_cache: HashMap<(u32, u32), Vec<ColumnInfo>> = HashMap::new();
    let mut constraints_cache: HashMap<(u32, u32), TableConstraintsInfo> = HashMap::new();

    for entry in entries {
        match &entry.payload {
            WalPayload::V1(WalPayloadV1::Insert {
                db_id,
                table_id,
                row_id,
                columns,
            }) => {
                let row_id_raw = keys::decode_base62(row_id)?;
                let row_key = build_row_key(*db_id, *table_id, row_id_raw)?;
                let row_value = serialize_columns(columns);
                let mut pairs = vec![(row_key, row_value)];
                let schema_cols =
                    lookup_schema_cols(neo4j, &mut schema_cache, *db_id, *table_id).await?;
                let constraints =
                    lookup_constraints(neo4j, &mut constraints_cache, *db_id, *table_id).await?;

                let pk_key = build_primary_key_mapping_key_from_columns(
                    *db_id,
                    *table_id,
                    &schema_cols,
                    &constraints,
                    columns,
                )?;
                pairs.push((pk_key, row_id.as_bytes().to_vec()));
                pairs.extend(build_unique_constraint_pairs_from_columns(
                    *db_id,
                    *table_id,
                    &schema_cols,
                    &constraints,
                    columns,
                    row_id,
                )?);

                // Replay index entries
                let indexed = lookup_indexed_cols(neo4j, &mut idx_cache, *db_id, *table_id).await?;
                if !indexed.is_empty() {
                    pairs.extend(build_index_entries_for_row(
                        *db_id, *table_id, row_id_raw, columns, &indexed,
                    ));
                }

                storage.mset(&pairs).await?;
                info!(entry_id = entry.header.entry_id, "replayed INSERT");
            }
            WalPayload::V1(WalPayloadV1::Update {
                db_id,
                table_id,
                row_id,
                columns,
            }) => {
                let row_id_raw = keys::decode_base62(row_id)?;
                let row_key = build_row_key(*db_id, *table_id, row_id_raw)?;
                let schema_cols =
                    lookup_schema_cols(neo4j, &mut schema_cache, *db_id, *table_id).await?;
                let constraints =
                    lookup_constraints(neo4j, &mut constraints_cache, *db_id, *table_id).await?;

                let mut existing = if let Some(data) = storage.get(&row_key).await? {
                    deserialize_columns(&data)
                } else {
                    Vec::new()
                };
                let old_pk_key = build_primary_key_mapping_key_from_columns(
                    *db_id,
                    *table_id,
                    &schema_cols,
                    &constraints,
                    &existing,
                )?;
                let old_unique_keys = build_unique_constraint_keys_from_columns(
                    *db_id,
                    *table_id,
                    &schema_cols,
                    &constraints,
                    &existing,
                )?;

                // Delete old index entries
                let indexed = lookup_indexed_cols(neo4j, &mut idx_cache, *db_id, *table_id).await?;
                if !indexed.is_empty() {
                    let old_keys = build_index_keys_for_row(
                        *db_id, *table_id, row_id_raw, &existing, &indexed,
                    );
                    if !old_keys.is_empty() {
                        storage.del(&old_keys).await?;
                    }
                }

                for (col_id, val) in columns {
                    if let Some(entry) = existing.iter_mut().find(|(id, _)| id == col_id) {
                        entry.1 = val.clone();
                    } else {
                        existing.push((*col_id, val.clone()));
                    }
                }
                let row_value = serialize_columns(&existing);
                let mut pairs = vec![(row_key, row_value)];

                // Write new index entries
                if !indexed.is_empty() {
                    pairs.extend(build_index_entries_for_row(
                        *db_id, *table_id, row_id_raw, &existing, &indexed,
                    ));
                }

                let mut constraint_keys_to_delete = old_unique_keys;
                constraint_keys_to_delete.push(old_pk_key);
                if !constraint_keys_to_delete.is_empty() {
                    storage.del(&constraint_keys_to_delete).await?;
                }

                storage.mset(&pairs).await?;
                let new_pk_key = build_primary_key_mapping_key_from_columns(
                    *db_id,
                    *table_id,
                    &schema_cols,
                    &constraints,
                    &existing,
                )?;
                let mut constraint_pairs = vec![(new_pk_key, row_id.as_bytes().to_vec())];
                constraint_pairs.extend(build_unique_constraint_pairs_from_columns(
                    *db_id,
                    *table_id,
                    &schema_cols,
                    &constraints,
                    &existing,
                    row_id,
                )?);
                storage.mset(&constraint_pairs).await?;
                info!(entry_id = entry.header.entry_id, "replayed UPDATE");
            }
            WalPayload::V1(WalPayloadV1::Delete {
                db_id,
                table_id,
                row_id,
                ..
            }) => {
                let row_id_raw = keys::decode_base62(row_id)?;
                let row_key = build_row_key(*db_id, *table_id, row_id_raw)?;
                let schema_cols =
                    lookup_schema_cols(neo4j, &mut schema_cache, *db_id, *table_id).await?;
                let constraints =
                    lookup_constraints(neo4j, &mut constraints_cache, *db_id, *table_id).await?;

                // Delete index entries before deleting row
                let indexed = lookup_indexed_cols(neo4j, &mut idx_cache, *db_id, *table_id).await?;
                if !indexed.is_empty() {
                    if let Some(data) = storage.get(&row_key).await? {
                        let existing = deserialize_columns(&data);
                        let old_keys = build_index_keys_for_row(
                            *db_id, *table_id, row_id_raw, &existing, &indexed,
                        );
                        if !old_keys.is_empty() {
                            storage.del(&old_keys).await?;
                        }
                    }
                }

                if let Some(data) = storage.get(&row_key).await? {
                    let existing = deserialize_columns(&data);
                    let mut constraint_keys_to_delete = build_unique_constraint_keys_from_columns(
                        *db_id,
                        *table_id,
                        &schema_cols,
                        &constraints,
                        &existing,
                    )?;
                    constraint_keys_to_delete.push(build_primary_key_mapping_key_from_columns(
                        *db_id,
                        *table_id,
                        &schema_cols,
                        &constraints,
                        &existing,
                    )?);
                    if !constraint_keys_to_delete.is_empty() {
                        storage.del(&constraint_keys_to_delete).await?;
                    }
                }

                storage.del(&[row_key]).await?;
                info!(entry_id = entry.header.entry_id, "replayed DELETE");
            }
        }
        wal_file.update_status(entry.file_offset, WalStatus::Committed)?;
    }
    Ok(())
}

/// Resolve indexed columns for a table during WAL replay, caching results.
async fn lookup_indexed_cols(
    neo4j: &Neo4jConnection,
    cache: &mut std::collections::HashMap<(u32, u32), Vec<ColumnInfo>>,
    db_id: u32,
    table_id: u32,
) -> Result<Vec<ColumnInfo>> {
    if let Some(cached) = cache.get(&(db_id, table_id)) {
        return Ok(cached.clone());
    }

    // Find the database and table names from IDs to call get_indexed_columns
    // For replay, we need to look up via Neo4j by IDs
    let mut result = neo4j
        .graph()
        .execute(
            neo4rs::query(
                "MATCH (db:Database)-[:HAS_TABLE]->(t:Table {id: $table_id}) \
             WHERE db.id = $db_id \
             RETURN db.name AS db_name, t.name AS table_name",
            )
            .param("db_id", db_id as i64)
            .param("table_id", table_id as i64),
        )
        .await?;

    let indexed = if let Some(row) = result.next().await? {
        let db_name: String = row.get("db_name")?;
        let table_name: String = row.get("table_name")?;
        neo4j.get_indexed_columns(&db_name, &table_name).await?
    } else {
        Vec::new()
    };

    cache.insert((db_id, table_id), indexed.clone());
    Ok(indexed)
}

async fn lookup_schema_cols(
    neo4j: &Neo4jConnection,
    cache: &mut HashMap<(u32, u32), Vec<ColumnInfo>>,
    db_id: u32,
    table_id: u32,
) -> Result<Vec<ColumnInfo>> {
    if let Some(cached) = cache.get(&(db_id, table_id)) {
        return Ok(cached.clone());
    }
    let mut result = neo4j
        .graph()
        .execute(
            neo4rs::query(
                "MATCH (db:Database)-[:HAS_TABLE]->(t:Table {id: $table_id}) \
                 WHERE db.id = $db_id \
                 RETURN db.name AS db_name, t.name AS table_name",
            )
            .param("db_id", db_id as i64)
            .param("table_id", table_id as i64),
        )
        .await?;
    let cols = if let Some(row) = result.next().await? {
        let db_name: String = row.get("db_name")?;
        let table_name: String = row.get("table_name")?;
        neo4j.describe_table(&db_name, &table_name).await?
    } else {
        Vec::new()
    };
    cache.insert((db_id, table_id), cols.clone());
    Ok(cols)
}

async fn lookup_constraints(
    neo4j: &Neo4jConnection,
    cache: &mut HashMap<(u32, u32), TableConstraintsInfo>,
    db_id: u32,
    table_id: u32,
) -> Result<TableConstraintsInfo> {
    if let Some(cached) = cache.get(&(db_id, table_id)) {
        return Ok(cached.clone());
    }
    let mut result = neo4j
        .graph()
        .execute(
            neo4rs::query(
                "MATCH (db:Database)-[:HAS_TABLE]->(t:Table {id: $table_id}) \
                 WHERE db.id = $db_id \
                 RETURN db.name AS db_name, t.name AS table_name",
            )
            .param("db_id", db_id as i64)
            .param("table_id", table_id as i64),
        )
        .await?;
    let constraints = if let Some(row) = result.next().await? {
        let db_name: String = row.get("db_name")?;
        let table_name: String = row.get("table_name")?;
        neo4j.get_table_constraints(&db_name, &table_name).await?
    } else {
        TableConstraintsInfo::default()
    };
    cache.insert((db_id, table_id), constraints.clone());
    Ok(constraints)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_columns() -> Vec<ColumnInfo> {
        vec![
            ColumnInfo {
                id: 1,
                name: "id".to_string(),
                data_type: "BIGINT".to_string(),
                nullable: false,
                is_pk: true,
                table_id: 1,
                indexed: false,
                index_name: None,
            },
            ColumnInfo {
                id: 2,
                name: "name".to_string(),
                data_type: "VARCHAR(255)".to_string(),
                nullable: false,
                is_pk: false,
                table_id: 1,
                indexed: false,
                index_name: None,
            },
            ColumnInfo {
                id: 3,
                name: "email".to_string(),
                data_type: "VARCHAR(255)".to_string(),
                nullable: true,
                is_pk: false,
                table_id: 1,
                indexed: false,
                index_name: None,
            },
        ]
    }

    #[test]
    fn resolve_columns_by_name() {
        let cols = test_columns();
        let names = vec!["id".to_string(), "name".to_string()];
        let vals = vec![b"1".to_vec(), b"Alice".to_vec()];

        let resolved = resolve_columns(&cols, &names, &vals).unwrap();
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].0.name, "id");
        assert_eq!(resolved[1].0.name, "name");
    }

    #[test]
    fn resolve_columns_all_implicit() {
        let cols = test_columns();
        let names: Vec<String> = vec![];
        let vals = vec![b"1".to_vec(), b"Alice".to_vec(), b"a@test.com".to_vec()];

        let resolved = resolve_columns(&cols, &names, &vals).unwrap();
        assert_eq!(resolved.len(), 3);
    }

    #[test]
    fn resolve_columns_wrong_count() {
        let cols = test_columns();
        let names: Vec<String> = vec![];
        let vals = vec![b"1".to_vec()];

        assert!(resolve_columns(&cols, &names, &vals).is_err());
    }

    #[test]
    fn resolve_columns_unknown_column() {
        let cols = test_columns();
        let names = vec!["nonexistent".to_string()];
        let vals = vec![b"val".to_vec()];

        assert!(resolve_columns(&cols, &names, &vals).is_err());
    }

    #[test]
    fn validate_not_null_columns_passes_with_all_required() {
        let cols = test_columns();
        let resolved = vec![(cols[0].id, b"1".to_vec()), (cols[1].id, b"Alice".to_vec())];

        assert!(validate_not_null_columns(&cols, &resolved).is_ok());
    }

    #[test]
    fn validate_not_null_columns_fails_missing_required() {
        let cols = test_columns();
        let resolved = vec![(cols[0].id, b"1".to_vec())];

        let err = validate_not_null_columns(&cols, &resolved).unwrap_err();
        assert!(err.to_string().contains("name"));
    }

    #[test]
    fn validate_not_null_columns_allows_missing_nullable() {
        let cols = test_columns();
        let resolved = vec![(cols[0].id, b"1".to_vec()), (cols[1].id, b"Alice".to_vec())];

        assert!(validate_not_null_columns(&cols, &resolved).is_ok());
    }

    #[test]
    fn serialize_deserialize_columns_roundtrip() {
        let columns = vec![
            (1u32, b"value1".to_vec()),
            (2u32, b"value2".to_vec()),
            (3u32, b"value3".to_vec()),
        ];

        let serialized = serialize_columns(&columns);
        let deserialized = deserialize_columns(&serialized);
        assert_eq!(deserialized.len(), 3);
        assert_eq!(deserialized[0], (1, b"value1".to_vec()));
        assert_eq!(deserialized[1], (2, b"value2".to_vec()));
        assert_eq!(deserialized[2], (3, b"value3".to_vec()));
    }

    #[test]
    fn deserialize_empty_returns_empty() {
        assert!(deserialize_columns(&[]).is_empty());
    }
}
