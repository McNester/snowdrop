use crate::kvrocks::KvrocksConnection;
use anyhow::{bail, Context, Result};
use keys::{build_key, encode_row_id};
use schema::{ColumnInfo, Neo4jConnection};
use tracing::{debug, info};
use wal::{WalFile, WalPayload, WalPayloadV1, WalStatus};

pub struct WriteEngine<'a> {
    kvrocks: &'a KvrocksConnection,
    neo4j: &'a Neo4jConnection,
    wal: &'a mut WalFile,
}

impl<'a> WriteEngine<'a> {
    pub fn new(
        kvrocks: &'a KvrocksConnection,
        neo4j: &'a Neo4jConnection,
        wal: &'a mut WalFile,
    ) -> Self {
        Self { kvrocks, neo4j, wal }
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
        let db = self.neo4j.get_database(db_name).await?
            .context(format!("database '{}' does not exist", db_name))?;
        let table = self.neo4j.get_table(db_name, table_name).await?
            .context(format!("table '{}' does not exist in database '{}'", table_name, db_name))?;
        let schema_cols = self.neo4j.describe_table(db_name, table_name).await?;

        // Resolve which columns are being inserted
        let resolved = resolve_columns(&schema_cols, column_names, values)?;

        // Validate NOT NULL constraints
        validate_not_null(&schema_cols, &resolved)?;

        // Check PK uniqueness
        let pk_col = schema_cols.iter().find(|c| c.is_pk)
            .context("table has no primary key")?;

        if let Some((_, pk_value)) = resolved.iter().find(|(col, _)| col.id == pk_col.id) {
            let pk_str = String::from_utf8_lossy(pk_value);
            self.check_pk_unique(db.id, table.id, &schema_cols, &pk_str).await?;
        }

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

        // Step 2: Neo4j transaction (IndexEntry nodes for indexed columns)
        // For M3, indexes are not implemented yet (M7), so we skip IndexEntry creation.
        // But we store the PK mapping: pk:{db_id}:{table_id}:{pk_value} -> row_id
        if let Some((_, pk_value)) = resolved.iter().find(|(col, _)| col.id == pk_col.id) {
            let pk_str = String::from_utf8_lossy(pk_value);
            let pk_key = format!("pk:{}:{}:{}", db.id, table.id, pk_str);
            self.kvrocks.mset(&[(pk_key, row_id.as_bytes().to_vec())]).await?;
        }

        // Step 3: WAL status -> Neo4jCommitted
        self.wal.update_status(file_offset, WalStatus::Neo4jCommitted)?;

        // Step 4: Kvrocks MSET
        let kv_pairs = build_kv_pairs(db.id, table.id, row_id_raw, &wal_columns)?;
        self.kvrocks.mset(&kv_pairs).await?;

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
        let db = self.neo4j.get_database(db_name).await?
            .context(format!("database '{}' does not exist", db_name))?;
        let table = self.neo4j.get_table(db_name, table_name).await?
            .context(format!("table '{}' does not exist", table_name))?;
        let schema_cols = self.neo4j.describe_table(db_name, table_name).await?;

        // Find matching row_ids via WHERE clause
        let row_ids = self.find_rows_by_predicate(db.id, table.id, &schema_cols, where_col, where_val).await?;

        if row_ids.is_empty() {
            return Ok(0);
        }

        let mut updated = 0u64;

        for (row_id_str, row_id_raw) in &row_ids {
            // Resolve SET columns
            let mut wal_columns: Vec<(u32, Vec<u8>)> = Vec::new();
            for (col_name, val) in set_pairs {
                let col = schema_cols.iter().find(|c| c.name == *col_name)
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

            // WAL -> Neo4jCommitted (no index updates in M3)
            self.wal.update_status(file_offset, WalStatus::Neo4jCommitted)?;

            // Kvrocks MSET for changed columns only
            let kv_pairs = build_kv_pairs(db.id, table.id, *row_id_raw, &wal_columns)?;
            self.kvrocks.mset(&kv_pairs).await?;

            // If PK column is being updated, update the pk: mapping
            let pk_col = schema_cols.iter().find(|c| c.is_pk);
            if let Some(pk_col) = pk_col {
                if let Some((_, new_pk_val)) = wal_columns.iter().find(|(id, _)| *id == pk_col.id) {
                    // Remove old pk mapping
                    let old_pk_key = format!("pk:{}:{}:{}", db.id, table.id, where_val);
                    self.kvrocks.del(&[old_pk_key]).await?;
                    // Add new pk mapping
                    let new_pk_str = String::from_utf8_lossy(new_pk_val);
                    let new_pk_key = format!("pk:{}:{}:{}", db.id, table.id, new_pk_str);
                    self.kvrocks.mset(&[(new_pk_key, row_id_str.as_bytes().to_vec())]).await?;
                }
            }

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
        let db = self.neo4j.get_database(db_name).await?
            .context(format!("database '{}' does not exist", db_name))?;
        let table = self.neo4j.get_table(db_name, table_name).await?
            .context(format!("table '{}' does not exist", table_name))?;
        let schema_cols = self.neo4j.describe_table(db_name, table_name).await?;

        let row_ids = self.find_rows_by_predicate(db.id, table.id, &schema_cols, where_col, where_val).await?;

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

            // Neo4j: delete IndexEntry nodes for this row (future: M7)
            // For now, just delete RowMeta if exists
            self.neo4j.graph()
                .run(neo4rs::query(
                    "MATCH (m:RowMeta {row_id: $row_id}) DELETE m"
                ).param("row_id", row_id_str.as_str()))
                .await
                .ok(); // Ignore if not found

            self.wal.update_status(file_offset, WalStatus::Neo4jCommitted)?;

            // Kvrocks DEL for all columns
            let keys: Vec<String> = col_ids
                .iter()
                .filter_map(|col_id| build_key(db.id, table.id, *row_id_raw, *col_id).ok())
                .collect();
            self.kvrocks.del(&keys).await?;

            // Also delete the pk: mapping
            if schema_cols.iter().any(|c| c.is_pk) {
                let pk_key = format!("pk:{}:{}:{}", db.id, table.id, where_val);
                self.kvrocks.del(&[pk_key]).await?;
            }

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
        let col = schema_cols.iter().find(|c| c.name == where_col)
            .context(format!("column '{}' not found in WHERE clause", where_col))?;

        if col.is_pk {
            // PK lookup via pk: key
            let pk_key = format!("pk:{}:{}:{}", db_id, table_id, where_val);
            if let Some(row_id_bytes) = self.kvrocks.get(&pk_key).await? {
                let row_id_str = String::from_utf8(row_id_bytes)
                    .context("invalid row_id encoding")?;
                let row_id_raw = keys::decode_base62(&row_id_str)?;
                return Ok(vec![(row_id_str, row_id_raw)]);
            }
            return Ok(Vec::new());
        }

        // Non-PK: scan all rows and filter
        let pattern = keys::table_scan_pattern(db_id, table_id)?;
        let all_keys = self.kvrocks.scan_keys(&pattern).await?;

        // Group keys by row_id
        let mut row_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        for key in &all_keys {
            let parts: Vec<&str> = key.split(':').collect();
            if parts.len() == 4 {
                row_map.entry(parts[2].to_string()).or_default().push(key.clone());
            }
        }

        // For each row, check if the where_col value matches
        let col_id_encoded = keys::encode_col_id(col.id)?;
        let mut matches = Vec::new();

        for (row_id_str, keys) in &row_map {
            for key in keys {
                if key.ends_with(&format!(":{}", col_id_encoded)) {
                    if let Some(val_bytes) = self.kvrocks.get(key).await? {
                        let val_str = String::from_utf8_lossy(&val_bytes);
                        if val_str == where_val {
                            let row_id_raw = keys::decode_base62(row_id_str)?;
                            matches.push((row_id_str.clone(), row_id_raw));
                        }
                    }
                }
            }
        }

        Ok(matches)
    }

    async fn check_pk_unique(
        &self,
        db_id: u32,
        table_id: u32,
        _schema_cols: &[ColumnInfo],
        pk_value: &str,
    ) -> Result<()> {
        let pk_key = format!("pk:{}:{}:{}", db_id, table_id, pk_value);
        if self.kvrocks.exists(&pk_key).await? {
            bail!("duplicate primary key value: {}", pk_value);
        }
        Ok(())
    }
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

/// Validate NOT NULL constraints.
fn validate_not_null(schema_cols: &[ColumnInfo], resolved: &[(ColumnInfo, Vec<u8>)]) -> Result<()> {
    // Check that all NOT NULL columns have a value provided
    for col in schema_cols {
        if !col.nullable && !col.is_pk {
            let has_value = resolved.iter().any(|(c, v)| c.id == col.id && !v.is_empty());
            if !has_value {
                bail!("column '{}' cannot be NULL", col.name);
            }
        }
    }
    Ok(())
}

/// Build Kvrocks key-value pairs from WAL column data.
fn build_kv_pairs(
    db_id: u32,
    table_id: u32,
    row_id: u64,
    columns: &[(u32, Vec<u8>)],
) -> Result<Vec<(String, Vec<u8>)>> {
    let mut pairs = Vec::new();
    for (col_id, value) in columns {
        let key = build_key(db_id, table_id, row_id, *col_id)?;
        pairs.push((key, value.clone()));
    }
    Ok(pairs)
}

/// Replay WAL entries to Kvrocks (called on startup for Neo4jCommitted entries).
/// Restores row data and PK mappings. Neo4j is used to resolve PK columns.
pub async fn replay_wal_entries(
    kvrocks: &KvrocksConnection,
    neo4j: &Neo4jConnection,
    entries: &[wal::WalEntry],
    wal_file: &WalFile,
) -> Result<()> {
    // Cache schema lookups to avoid repeated Neo4j queries
    let mut pk_cache: std::collections::HashMap<(u32, u32), Option<u32>> = std::collections::HashMap::new();

    for entry in entries {
        match &entry.payload {
            WalPayload::V1(WalPayloadV1::Insert { db_id, table_id, row_id, columns }) => {
                let row_id_raw = keys::decode_base62(row_id)?;
                let kv_pairs = build_kv_pairs(*db_id, *table_id, row_id_raw, columns)?;
                kvrocks.mset(&kv_pairs).await?;

                // Restore PK mapping
                let pk_col_id = lookup_pk_col_id(neo4j, &mut pk_cache, *db_id, *table_id).await?;
                if let Some(pk_col_id) = pk_col_id {
                    if let Some((_, pk_value)) = columns.iter().find(|(id, _)| *id == pk_col_id) {
                        let pk_str = String::from_utf8_lossy(pk_value);
                        let pk_key = format!("pk:{}:{}:{}", db_id, table_id, pk_str);
                        kvrocks.mset(&[(pk_key, row_id.as_bytes().to_vec())]).await?;
                    }
                }

                info!(entry_id = entry.header.entry_id, "replayed INSERT to Kvrocks");
            }
            WalPayload::V1(WalPayloadV1::Update { db_id, table_id, row_id, columns }) => {
                let row_id_raw = keys::decode_base62(row_id)?;
                let kv_pairs = build_kv_pairs(*db_id, *table_id, row_id_raw, columns)?;
                kvrocks.mset(&kv_pairs).await?;
                info!(entry_id = entry.header.entry_id, "replayed UPDATE to Kvrocks");
            }
            WalPayload::V1(WalPayloadV1::Delete { db_id, table_id, row_id, col_ids }) => {
                let row_id_raw = keys::decode_base62(row_id)?;
                let keys: Vec<String> = col_ids
                    .iter()
                    .filter_map(|col_id| build_key(*db_id, *table_id, row_id_raw, *col_id).ok())
                    .collect();
                kvrocks.del(&keys).await?;
                info!(entry_id = entry.header.entry_id, "replayed DELETE to Kvrocks");
            }
        }
        wal_file.update_status(entry.file_offset, WalStatus::Committed)?;
    }
    Ok(())
}

/// Resolve the PK column ID for a table, caching results.
async fn lookup_pk_col_id(
    neo4j: &Neo4jConnection,
    cache: &mut std::collections::HashMap<(u32, u32), Option<u32>>,
    db_id: u32,
    table_id: u32,
) -> Result<Option<u32>> {
    if let Some(cached) = cache.get(&(db_id, table_id)) {
        return Ok(*cached);
    }

    // Query Neo4j for PK column of this table
    let pk_col_id = neo4j.get_pk_col_id_by_table_id(db_id, table_id).await?;
    cache.insert((db_id, table_id), pk_col_id);
    Ok(pk_col_id)
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
            },
            ColumnInfo {
                id: 2,
                name: "name".to_string(),
                data_type: "VARCHAR(255)".to_string(),
                nullable: false,
                is_pk: false,
                table_id: 1,
            },
            ColumnInfo {
                id: 3,
                name: "email".to_string(),
                data_type: "VARCHAR(255)".to_string(),
                nullable: true,
                is_pk: false,
                table_id: 1,
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
    fn validate_not_null_passes_with_all_required() {
        let cols = test_columns();
        let resolved = vec![
            (cols[0].clone(), b"1".to_vec()),
            (cols[1].clone(), b"Alice".to_vec()),
        ];

        assert!(validate_not_null(&cols, &resolved).is_ok());
    }

    #[test]
    fn validate_not_null_fails_missing_required() {
        let cols = test_columns();
        let resolved = vec![
            (cols[0].clone(), b"1".to_vec()),
        ];

        let err = validate_not_null(&cols, &resolved).unwrap_err();
        assert!(err.to_string().contains("name"));
    }

    #[test]
    fn validate_not_null_allows_missing_nullable() {
        let cols = test_columns();
        let resolved = vec![
            (cols[0].clone(), b"1".to_vec()),
            (cols[1].clone(), b"Alice".to_vec()),
        ];

        assert!(validate_not_null(&cols, &resolved).is_ok());
    }

    #[test]
    fn build_kv_pairs_produces_correct_keys() {
        let columns = vec![
            (1u32, b"value1".to_vec()),
            (2u32, b"value2".to_vec()),
        ];

        let pairs = build_kv_pairs(1, 1, 12345, &columns).unwrap();
        assert_eq!(pairs.len(), 2);
        for (key, _) in &pairs {
            assert_eq!(key.len(), 21);
            assert_eq!(key.chars().filter(|c| *c == ':').count(), 3);
        }
    }

}
