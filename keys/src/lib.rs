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
pub const DB_ID_WIDTH: usize = 2;    // 3,844 databases
pub const TABLE_ID_WIDTH: usize = 3; // 238,328 tables
pub const ROW_ID_WIDTH: usize = 11;  // ~3.5e19 rows
pub const COL_ID_WIDTH: usize = 2;   // 3,844 columns per table

/// Encodes a db_id (u32) to 2-char base62.
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

/// Constructs a full Kvrocks key: {db_id}:{table_id}:{row_id}:{col_id}
/// Total length: 21 chars (2+1+3+1+11+1+2 = 21)
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

/// Scan pattern for a specific column across all rows: {db_id}:{table_id}:*:{col_id}
pub fn column_scan_pattern(
    db_id: u32,
    table_id: u32,
    col_id: u32,
) -> Result<String, KeyError> {
    Ok(format!(
        "{}:{}:*:{}",
        encode_db_id(db_id)?,
        encode_table_id(table_id)?,
        encode_col_id(col_id)?
    ))
}

/// Scan pattern for all keys of a specific row: {db_id}:{table_id}:{row_id}:*
pub fn row_scan_pattern(
    db_id: u32,
    table_id: u32,
    row_id: u64,
) -> Result<String, KeyError> {
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
        assert_eq!(key, "01:00F:00000000000:03");
        assert_eq!(key.len(), 21);
    }

    #[test]
    fn key_length_is_always_21() {
        let key = build_key(0, 0, 0, 0).unwrap();
        assert_eq!(key.len(), 21);

        let key = build_key(3843, 238327, u64::MAX, 3843).unwrap();
        assert_eq!(key.len(), 21);
    }

    #[test]
    fn table_scan_pattern_format() {
        let pat = table_scan_pattern(1, 15).unwrap();
        assert_eq!(pat, "01:00F:*");
    }

    #[test]
    fn column_scan_pattern_format() {
        let pat = column_scan_pattern(1, 15, 5).unwrap();
        assert_eq!(pat, "01:00F:*:05");
    }

    #[test]
    fn row_scan_pattern_format() {
        let pat = row_scan_pattern(1, 15, 0).unwrap();
        assert_eq!(pat, "01:00F:00000000000:*");
    }

    #[test]
    fn db_scan_pattern_format() {
        let pat = db_scan_pattern(1).unwrap();
        assert_eq!(pat, "01:*");
    }

    #[test]
    fn decode_invalid_chars() {
        assert!(decode_base62("!!").is_err());
        assert!(decode_base62("a+b").is_err());
    }
}
