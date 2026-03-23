use crate::dml::deserialize_columns;
use crate::row_format::RowView;
use crate::storage::StorageBackend;
use anyhow::Result;
use keys::{build_index_key, index_point_prefix, index_prefix, parse_index_key, table_scan_prefix};
use schema::ColumnInfo;
use tracing::debug;

// ── Index Value Encoding ──
// Produces lexicographically sortable strings so RocksDB prefix iteration
// yields results in natural order.

/// Encode a raw column value for secondary index storage.
/// The encoded string sorts lexicographically in the same order as the
/// natural type ordering.
pub fn encode_index_value(raw: &[u8], data_type: &str) -> String {
    if raw.is_empty() {
        return "~NULL".to_string();
    }

    let s = String::from_utf8_lossy(raw);
    let dt = data_type.to_uppercase();

    match dt.as_str() {
        "TINYINT" | "SMALLINT" | "INT" | "INTEGER" | "BIGINT" => {
            if let Ok(v) = s.parse::<i64>() {
                encode_i64(v)
            } else {
                s.into_owned()
            }
        }
        "FLOAT" | "DOUBLE" | "REAL" => {
            if let Ok(v) = s.parse::<f64>() {
                encode_f64(v)
            } else {
                s.into_owned()
            }
        }
        dt if dt.starts_with("DECIMAL") || dt.starts_with("NUMERIC") => {
            if let Ok(v) = s.parse::<f64>() {
                encode_f64(v)
            } else {
                s.into_owned()
            }
        }
        "BOOLEAN" | "BOOL" => match s.as_ref() {
            "true" | "1" | "t" => "1".to_string(),
            _ => "0".to_string(),
        },
        // DATE, DATETIME, TIMESTAMP, VARCHAR, CHAR, TEXT -- pass through
        _ => s.into_owned(),
    }
}

/// Offset encoding for signed integers.
/// Maps i64::MIN -> "00000000000000000000", 0 -> "09223372036854775808", i64::MAX -> "18446744073709551615"
fn encode_i64(v: i64) -> String {
    let unsigned = (v as u64).wrapping_add(u64::MAX / 2 + 1);
    format!("{:020}", unsigned)
}

/// IEEE 754 sortable encoding for floats.
/// Positive floats: flip sign bit (so they sort after negatives).
/// Negative floats: flip all bits (so more-negative sorts first).
fn encode_f64(v: f64) -> String {
    let bits = v.to_bits();
    let sortable = if bits & (1u64 << 63) != 0 {
        // Negative: flip all bits
        !bits
    } else {
        // Positive (or zero): flip sign bit only
        bits | (1u64 << 63)
    };
    format!("{:016x}", sortable)
}

/// Decode an offset-encoded i64 back to its original value.
pub fn decode_i64(encoded: &str) -> Option<i64> {
    encoded.parse::<u64>().ok().map(|u| u.wrapping_sub(u64::MAX / 2 + 1) as i64)
}

// ── Index Scan Operations ──

/// Point lookup: find all row_ids where column equals the given encoded value.
pub async fn index_point_lookup(
    storage: &dyn StorageBackend,
    db_id: u32,
    table_id: u32,
    col_id: u32,
    value_encoded: &str,
) -> Result<Vec<u64>> {
    let prefix = index_point_prefix(db_id, table_id, col_id, value_encoded)?;
    let entries = storage.scan_prefix(&prefix).await?;

    let mut row_ids = Vec::with_capacity(entries.len());
    for (key, _) in &entries {
        let (_, _, _, _, row_id) = parse_index_key(key)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        row_ids.push(row_id);
    }
    Ok(row_ids)
}

/// Range scan: find all row_ids where the encoded value falls within [low, high].
/// Either bound can be None for an open-ended range.
pub async fn index_range_scan(
    storage: &dyn StorageBackend,
    db_id: u32,
    table_id: u32,
    col_id: u32,
    low: Option<&str>,
    high: Option<&str>,
    low_inclusive: bool,
    high_inclusive: bool,
) -> Result<Vec<u64>> {
    let prefix = index_prefix(db_id, table_id, col_id)?;
    let entries = storage.scan_prefix(&prefix).await?;

    let mut row_ids = Vec::new();
    for (key, _) in &entries {
        let (_, _, _, val, row_id) = parse_index_key(key)
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let above_low = match low {
            None => true,
            Some(lo) => {
                if low_inclusive { val.as_str() >= lo } else { val.as_str() > lo }
            }
        };
        let below_high = match high {
            None => true,
            Some(hi) => {
                if high_inclusive { val.as_str() <= hi } else { val.as_str() < hi }
            }
        };

        if above_low && below_high {
            row_ids.push(row_id);
        }
    }
    Ok(row_ids)
}

/// Backfill index entries for all existing rows of a table.
/// Called after CREATE INDEX to populate the index for pre-existing data.
/// Returns the number of entries created.
pub async fn backfill_index(
    storage: &dyn StorageBackend,
    db_id: u32,
    table_id: u32,
    col_id: u32,
    data_type: &str,
    _schema_cols: &[ColumnInfo],
) -> Result<usize> {
    let prefix = table_scan_prefix(db_id, table_id)?;
    let all_kv = storage.scan_prefix(&prefix).await?;

    let mut pairs = Vec::new();
    let mut count = 0;

    for (key, row_data) in &all_kv {
        if key.starts_with("pk:") || key.starts_with("idx:") {
            continue;
        }

        // Extract row_id from key (third segment)
        let parts: Vec<&str> = key.split(':').collect();
        if parts.len() != 3 {
            continue;
        }
        let row_id = keys::decode_base62(parts[2])
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        // Extract column value
        let col_bytes = if let Some(view) = RowView::new(row_data) {
            view.find_col(col_id).map(|b| b.to_vec())
        } else {
            let cols = deserialize_columns(row_data);
            cols.into_iter().find(|(id, _)| *id == col_id).map(|(_, v)| v)
        };

        if let Some(raw) = col_bytes {
            let encoded = encode_index_value(&raw, data_type);
            let idx_key = build_index_key(db_id, table_id, col_id, &encoded, row_id)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            pairs.push((idx_key, Vec::new()));
            count += 1;
        }
    }

    if !pairs.is_empty() {
        storage.mset(&pairs).await?;
    }

    debug!(db_id, table_id, col_id, count, "index backfill complete");
    Ok(count)
}

/// Remove all index entries for a column. Called on DROP INDEX.
/// Returns the number of entries deleted.
pub async fn drop_index_entries(
    storage: &dyn StorageBackend,
    db_id: u32,
    table_id: u32,
    col_id: u32,
) -> Result<usize> {
    let prefix = index_prefix(db_id, table_id, col_id)?;
    let entries = storage.scan_prefix(&prefix).await?;

    let keys: Vec<String> = entries.into_iter().map(|(k, _)| k).collect();
    let count = keys.len();

    if !keys.is_empty() {
        storage.del(&keys).await?;
    }

    debug!(db_id, table_id, col_id, count, "index entries dropped");
    Ok(count)
}

/// Build index entries for a single row's indexed columns.
/// Returns (key, empty_value) pairs ready for mset.
pub fn build_index_entries_for_row(
    db_id: u32,
    table_id: u32,
    row_id: u64,
    columns: &[(u32, Vec<u8>)],
    indexed_cols: &[ColumnInfo],
) -> Vec<(String, Vec<u8>)> {
    let mut pairs = Vec::new();
    for col in indexed_cols {
        if let Some((_, raw)) = columns.iter().find(|(id, _)| *id == col.id) {
            let encoded = encode_index_value(raw, &col.data_type);
            if let Ok(key) = build_index_key(db_id, table_id, col.id, &encoded, row_id) {
                pairs.push((key, Vec::new()));
            }
        }
    }
    pairs
}

/// Build index keys to delete for a single row's indexed columns.
pub fn build_index_keys_for_row(
    db_id: u32,
    table_id: u32,
    row_id: u64,
    columns: &[(u32, Vec<u8>)],
    indexed_cols: &[ColumnInfo],
) -> Vec<String> {
    let mut keys = Vec::new();
    for col in indexed_cols {
        if let Some((_, raw)) = columns.iter().find(|(id, _)| *id == col.id) {
            let encoded = encode_index_value(raw, &col.data_type);
            if let Ok(key) = build_index_key(db_id, table_id, col.id, &encoded, row_id) {
                keys.push(key);
            }
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── encode_i64 ──

    #[test]
    fn i64_encoding_preserves_order() {
        let vals = [i64::MIN, -1000, -1, 0, 1, 1000, i64::MAX];
        let encoded: Vec<String> = vals.iter().map(|v| encode_i64(*v)).collect();
        for w in encoded.windows(2) {
            assert!(w[0] < w[1], "{} should be < {}", w[0], w[1]);
        }
    }

    #[test]
    fn i64_encoding_fixed_width() {
        assert_eq!(encode_i64(0).len(), 20);
        assert_eq!(encode_i64(i64::MIN).len(), 20);
        assert_eq!(encode_i64(i64::MAX).len(), 20);
    }

    #[test]
    fn i64_roundtrip() {
        for v in [i64::MIN, -1, 0, 1, 42, i64::MAX] {
            assert_eq!(decode_i64(&encode_i64(v)), Some(v));
        }
    }

    // ── encode_f64 ──

    #[test]
    fn f64_encoding_preserves_order() {
        let vals = [-1000.0, -1.0, -0.01, 0.0, 0.01, 1.0, 1000.0];
        let encoded: Vec<String> = vals.iter().map(|v| encode_f64(*v)).collect();
        for w in encoded.windows(2) {
            assert!(w[0] < w[1], "{} should be < {} (for consecutive values)", w[0], w[1]);
        }
    }

    #[test]
    fn f64_encoding_fixed_width() {
        assert_eq!(encode_f64(0.0).len(), 16);
        assert_eq!(encode_f64(-1.0).len(), 16);
    }

    // ── encode_index_value ──

    #[test]
    fn encode_integer_types() {
        let enc = encode_index_value(b"42", "BIGINT");
        assert_eq!(enc, encode_i64(42));
    }

    #[test]
    fn encode_date_passthrough() {
        assert_eq!(encode_index_value(b"2024-01-15", "DATE"), "2024-01-15");
    }

    #[test]
    fn encode_text_passthrough() {
        assert_eq!(encode_index_value(b"hello", "VARCHAR(255)"), "hello");
    }

    #[test]
    fn encode_boolean() {
        assert_eq!(encode_index_value(b"true", "BOOLEAN"), "1");
        assert_eq!(encode_index_value(b"false", "BOOLEAN"), "0");
    }

    #[test]
    fn encode_null_value() {
        assert_eq!(encode_index_value(b"", "BIGINT"), "~NULL");
    }

    #[test]
    fn encode_float_type() {
        let e1 = encode_index_value(b"-1.5", "DOUBLE");
        let e2 = encode_index_value(b"1.5", "DOUBLE");
        assert!(e1 < e2);
    }

    #[test]
    fn encode_decimal_type() {
        let e1 = encode_index_value(b"0.05", "DECIMAL(10,2)");
        let e2 = encode_index_value(b"0.07", "DECIMAL(10,2)");
        assert!(e1 < e2);
    }

    // ── build_index_entries_for_row ──

    #[test]
    fn build_entries_for_indexed_columns() {
        let indexed = vec![
            ColumnInfo {
                id: 3, name: "age".into(), data_type: "INT".into(),
                nullable: false, is_pk: false, table_id: 1,
                indexed: true, index_name: Some("idx_age".into()),
            },
        ];
        let columns = vec![(1u32, b"pk_val".to_vec()), (3, b"25".to_vec())];

        let entries = build_index_entries_for_row(1, 1, 100, &columns, &indexed);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].0.starts_with("idx:"));
        assert!(entries[0].1.is_empty());
    }

    #[test]
    fn build_entries_skips_missing_columns() {
        let indexed = vec![
            ColumnInfo {
                id: 5, name: "missing".into(), data_type: "INT".into(),
                nullable: true, is_pk: false, table_id: 1,
                indexed: true, index_name: Some("idx_missing".into()),
            },
        ];
        let columns = vec![(1u32, b"pk_val".to_vec())];

        let entries = build_index_entries_for_row(1, 1, 100, &columns, &indexed);
        assert!(entries.is_empty());
    }
}
