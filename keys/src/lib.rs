use thiserror::Error;

#[derive(Debug, Error)]
pub enum KeyError {
    #[error("value {0} exceeds maximum for {1}-char base62 encoding")]
    Overflow(u64, usize),
    #[error("invalid base62 string: {0}")]
    InvalidBase62(String),
}

const BASE62_CHARS: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Encodes a u64 into a zero-padded base62 string of exactly `width` characters.
pub fn encode_base62(value: u64, width: usize) -> Result<String, KeyError> {
    let mut result = vec![0u8; width];
    let mut val = value;

    for i in (0..width).rev() {
        result[i] = BASE62_CHARS[(val % 62) as usize];
        val /= 62;
    }

    if val > 0 {
        return Err(KeyError::Overflow(value, width));
    }

    Ok(String::from_utf8(result).unwrap())
}

/// Decodes a base62 string back to u64.
pub fn decode_base62(s: &str) -> Result<u64, KeyError> {
    let mut result: u64 = 0;

    for &b in s.as_bytes() {
        let digit = match b {
            b'0'..=b'9' => (b - b'0') as u64,
            b'A'..=b'Z' => (b - b'A' + 10) as u64,
            b'a'..=b'z' => (b - b'a' + 36) as u64,
            _ => return Err(KeyError::InvalidBase62(s.to_string())),
        };
        result = result
            .checked_mul(62)
            .and_then(|r| r.checked_add(digit))
            .ok_or_else(|| KeyError::InvalidBase62(s.to_string()))?;
    }

    Ok(result)
}

// Width constants matching the key format spec
pub const DB_ID_WIDTH: usize = 3; // 238,328 databases
pub const TABLE_ID_WIDTH: usize = 3; // 238,328 tables
pub const ROW_ID_WIDTH: usize = 11; // ~3.5e19 rows
pub const COL_ID_WIDTH: usize = 3; // 238,328 logical column IDs

/// Encodes a db_id (u32) to 3-char base62.
pub fn encode_db_id(id: u32) -> Result<String, KeyError> {
    encode_base62(id as u64, DB_ID_WIDTH)
}

/// Encodes a table_id (u32) to 3-char base62.
pub fn encode_table_id(id: u32) -> Result<String, KeyError> {
    encode_base62(id as u64, TABLE_ID_WIDTH)
}

/// Encodes a row_id (u64) to 11-char base62.
pub fn encode_row_id(id: u64) -> Result<String, KeyError> {
    encode_base62(id, ROW_ID_WIDTH)
}

/// Encodes a col_id (u32) to 2-char base62.
pub fn encode_col_id(id: u32) -> Result<String, KeyError> {
    encode_base62(id as u64, COL_ID_WIDTH)
}

/// Constructs a row-level key: {db_id}:{table_id}:{row_id}
/// Total length: 19 chars (3+1+3+1+11 = 19)
/// Used for row-oriented storage where all columns are in one value.
pub fn build_row_key(db_id: u32, table_id: u32, row_id: u64) -> Result<String, KeyError> {
    Ok(format!(
        "{}:{}:{}",
        encode_db_id(db_id)?,
        encode_table_id(table_id)?,
        encode_row_id(row_id)?
    ))
}

/// Constructs a full Kvrocks key: {db_id}:{table_id}:{row_id}:{col_id}
/// Total length: 23 chars (3+1+3+1+11+1+3 = 23)
pub fn build_key(db_id: u32, table_id: u32, row_id: u64, col_id: u32) -> Result<String, KeyError> {
    Ok(format!(
        "{}:{}:{}:{}",
        encode_db_id(db_id)?,
        encode_table_id(table_id)?,
        encode_row_id(row_id)?,
        encode_col_id(col_id)?
    ))
}

/// Scan pattern for all keys in a table: {db_id}:{table_id}:*
pub fn table_scan_pattern(db_id: u32, table_id: u32) -> Result<String, KeyError> {
    Ok(format!(
        "{}:{}:*",
        encode_db_id(db_id)?,
        encode_table_id(table_id)?
    ))
}

/// Prefix for all keys in a table (no wildcard): {db_id}:{table_id}:
/// Used for RocksDB prefix iteration.
pub fn table_scan_prefix(db_id: u32, table_id: u32) -> Result<String, KeyError> {
    Ok(format!(
        "{}:{}:",
        encode_db_id(db_id)?,
        encode_table_id(table_id)?
    ))
}

/// Scan pattern for a specific column across all rows: {db_id}:{table_id}:*:{col_id}
pub fn column_scan_pattern(db_id: u32, table_id: u32, col_id: u32) -> Result<String, KeyError> {
    Ok(format!(
        "{}:{}:*:{}",
        encode_db_id(db_id)?,
        encode_table_id(table_id)?,
        encode_col_id(col_id)?
    ))
}

/// Scan pattern for all keys of a specific row: {db_id}:{table_id}:{row_id}:*
pub fn row_scan_pattern(db_id: u32, table_id: u32, row_id: u64) -> Result<String, KeyError> {
    Ok(format!(
        "{}:{}:{}:*",
        encode_db_id(db_id)?,
        encode_table_id(table_id)?,
        encode_row_id(row_id)?
    ))
}

/// Scan pattern for all keys in a database: {db_id}:*
pub fn db_scan_pattern(db_id: u32) -> Result<String, KeyError> {
    Ok(format!("{}:*", encode_db_id(db_id)?))
}

// ── Secondary Index Keys ──

/// Constructs a secondary index key: idx:{db}:{tbl}:{col}:{value_encoded}:{row}
/// Lexicographic ordering of value_encoded ensures range scans work via prefix iteration.
pub fn build_index_key(
    db_id: u32,
    table_id: u32,
    col_id: u32,
    value_encoded: &str,
    row_id: u64,
) -> Result<String, KeyError> {
    Ok(format!(
        "idx:{}:{}:{}:{}:{}",
        encode_db_id(db_id)?,
        encode_table_id(table_id)?,
        encode_col_id(col_id)?,
        value_encoded,
        encode_row_id(row_id)?
    ))
}

/// Prefix for all index entries of a column: idx:{db}:{tbl}:{col}:
pub fn index_prefix(db_id: u32, table_id: u32, col_id: u32) -> Result<String, KeyError> {
    Ok(format!(
        "idx:{}:{}:{}:",
        encode_db_id(db_id)?,
        encode_table_id(table_id)?,
        encode_col_id(col_id)?
    ))
}

/// Prefix for exact-match index lookup: idx:{db}:{tbl}:{col}:{value_encoded}:
pub fn index_point_prefix(
    db_id: u32,
    table_id: u32,
    col_id: u32,
    value_encoded: &str,
) -> Result<String, KeyError> {
    Ok(format!(
        "idx:{}:{}:{}:{}:",
        encode_db_id(db_id)?,
        encode_table_id(table_id)?,
        encode_col_id(col_id)?,
        value_encoded
    ))
}

/// Parse a secondary index key back into its components.
/// Returns (db_id, table_id, col_id, value_encoded, row_id).
pub fn parse_index_key(key: &str) -> Result<(u32, u32, u32, String, u64), KeyError> {
    let parts: Vec<&str> = key.splitn(6, ':').collect();
    if parts.len() != 6 || parts[0] != "idx" {
        return Err(KeyError::InvalidBase62(format!("not an index key: {key}")));
    }
    let db_id = decode_base62(parts[1])? as u32;
    let table_id = decode_base62(parts[2])? as u32;
    let col_id = decode_base62(parts[3])? as u32;
    let value_encoded = parts[4].to_string();
    let row_id = decode_base62(parts[5])?;
    Ok((db_id, table_id, col_id, value_encoded, row_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base62_encode_zero() {
        assert_eq!(encode_base62(0, 2).unwrap(), "00");
        assert_eq!(encode_base62(0, 3).unwrap(), "000");
        assert_eq!(encode_base62(0, 11).unwrap(), "00000000000");
    }

    #[test]
    fn base62_encode_values() {
        assert_eq!(encode_base62(1, 2).unwrap(), "01");
        assert_eq!(encode_base62(61, 2).unwrap(), "0z");
        assert_eq!(encode_base62(62, 2).unwrap(), "10");
        assert_eq!(encode_base62(10, 2).unwrap(), "0A");
    }

    #[test]
    fn base62_roundtrip() {
        for val in [0, 1, 61, 62, 100, 3843, 238327, u64::MAX / 2] {
            let encoded = encode_base62(val, 11).unwrap();
            let decoded = decode_base62(&encoded).unwrap();
            assert_eq!(val, decoded, "roundtrip failed for {val}");
        }
    }

    #[test]
    fn base62_overflow() {
        // 62^2 = 3844, so 3844 cannot fit in 2 chars
        assert!(encode_base62(3844, 2).is_err());
    }

    #[test]
    fn build_key_format() {
        let key = build_key(1, 15, 0, 3).unwrap();
        assert_eq!(key, "001:00F:00000000000:003");
        assert_eq!(key.len(), 23);
    }

    #[test]
    fn key_length_is_always_23() {
        let key = build_key(0, 0, 0, 0).unwrap();
        assert_eq!(key.len(), 23);

        let key = build_key(238327, 238327, u64::MAX, 3843).unwrap();
        assert_eq!(key.len(), 23);
    }

    #[test]
    fn table_scan_pattern_format() {
        let pat = table_scan_pattern(1, 15).unwrap();
        assert_eq!(pat, "001:00F:*");
    }

    #[test]
    fn column_scan_pattern_format() {
        let pat = column_scan_pattern(1, 15, 5).unwrap();
        assert_eq!(pat, "001:00F:*:005");
    }

    #[test]
    fn row_scan_pattern_format() {
        let pat = row_scan_pattern(1, 15, 0).unwrap();
        assert_eq!(pat, "001:00F:00000000000:*");
    }

    #[test]
    fn db_scan_pattern_format() {
        let pat = db_scan_pattern(1).unwrap();
        assert_eq!(pat, "001:*");
    }

    #[test]
    fn decode_invalid_chars() {
        assert!(decode_base62("!!").is_err());
        assert!(decode_base62("a+b").is_err());
    }

    // ── Index key tests ──

    #[test]
    fn build_index_key_format() {
        let key = build_index_key(1, 15, 3, "09223372036854775808", 42).unwrap();
        assert!(key.starts_with("idx:001:00F:003:09223372036854775808:"));
    }

    #[test]
    fn index_key_roundtrip() {
        let key = build_index_key(1, 15, 3, "2024-01-15", 999).unwrap();
        let (db, tbl, col, val, row) = parse_index_key(&key).unwrap();
        assert_eq!(db, 1);
        assert_eq!(tbl, 15);
        assert_eq!(col, 3);
        assert_eq!(val, "2024-01-15");
        assert_eq!(row, 999);
    }

    #[test]
    fn index_prefix_format() {
        let p = index_prefix(1, 15, 3).unwrap();
        assert_eq!(p, "idx:001:00F:003:");
    }

    #[test]
    fn index_point_prefix_format() {
        let p = index_point_prefix(1, 15, 3, "hello").unwrap();
        assert_eq!(p, "idx:001:00F:003:hello:");
    }

    #[test]
    fn large_column_ids_fit_index_key_encoding() {
        let key = build_index_key(1, 15, 8248, "2024-01-15", 999).unwrap();
        let (db, tbl, col, val, row) = parse_index_key(&key).unwrap();
        assert_eq!(db, 1);
        assert_eq!(tbl, 15);
        assert_eq!(col, 8248);
        assert_eq!(val, "2024-01-15");
        assert_eq!(row, 999);
    }

    #[test]
    fn index_key_lexicographic_order() {
        // Smaller encoded values produce lexicographically smaller keys
        let k1 = build_index_key(1, 1, 1, "00000", 1).unwrap();
        let k2 = build_index_key(1, 1, 1, "00001", 1).unwrap();
        assert!(k1 < k2);
    }

    #[test]
    fn parse_index_key_rejects_non_index() {
        assert!(parse_index_key("001:00F:00000000000").is_err());
        assert!(parse_index_key("notidx:001:00F:03:val:00000000000").is_err());
    }
}
