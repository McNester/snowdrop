/// In-memory columnar cache for SIMD-friendly full-table scans.
///
/// Row-oriented RocksDB remains the source of truth for durability and mutations.
/// On first scan of a table, columns are extracted into contiguous typed arrays.
/// Predicates are evaluated column-at-a-time using auto-vectorizable loops that
/// produce u64 bitmasks. Only matching rows are materialized into Row objects.
use crate::row_format::RowView;
use crate::typed_value::{TypedValue, EMPTY_TEXT_SENTINEL};
use schema::ColumnInfo;
use std::collections::HashMap;

// ── Typed column arrays ──

/// Columnar representation of a single column's data.
/// Fixed-width types are stored as contiguous native arrays for SIMD.
#[derive(Debug, Clone)]
pub enum TypedColumn {
    Int64(Vec<i64>),
    Float64(Vec<f64>),
    /// Days since Unix epoch (1970-01-01). Enables i32 SIMD comparisons on dates.
    DateDays(Vec<i32>),
    Text(Vec<String>),
    Boolean(Vec<bool>),
}

/// Null bitmap: bit N is set if row N is NULL for this column.
#[derive(Debug, Clone)]
pub struct NullBitmap {
    words: Vec<u64>,
}

impl NullBitmap {
    pub fn new(len: usize) -> Self {
        let num_words = (len + 63) / 64;
        Self {
            words: vec![0u64; num_words],
        }
    }

    #[inline]
    pub fn set_null(&mut self, index: usize) {
        self.words[index / 64] |= 1u64 << (index % 64);
    }

    #[inline]
    pub fn is_null(&self, index: usize) -> bool {
        (self.words[index / 64] >> (index % 64)) & 1 == 1
    }
}

/// All columns for a single table, ready for columnar scans.
pub struct TableColumns {
    pub row_count: usize,
    pub row_keys: Vec<String>,
    pub columns: HashMap<u32, (TypedColumn, NullBitmap)>,
}

// ── Column classification ──

#[derive(Debug, Clone, Copy)]
enum ColumnKind {
    Int64,
    Float64,
    DateDays,
    Text,
    Boolean,
}

fn classify_column(data_type: &str) -> ColumnKind {
    let dt = data_type.to_uppercase();
    match dt.as_str() {
        "TINYINT" | "SMALLINT" | "INT" | "INTEGER" | "BIGINT" => ColumnKind::Int64,
        "FLOAT" | "DOUBLE" | "REAL" => ColumnKind::Float64,
        "BOOLEAN" | "BOOL" => ColumnKind::Boolean,
        "DATE" => ColumnKind::DateDays,
        dt if dt.starts_with("DECIMAL") || dt.starts_with("NUMERIC") => ColumnKind::Float64,
        _ => ColumnKind::Text,
    }
}

// ── Date conversion ──

/// Convert "YYYY-MM-DD" to days since 1970-01-01 using Howard Hinnant's algorithm.
pub fn date_to_days(s: &str) -> Option<i32> {
    let bytes = s.as_bytes();
    if bytes.len() < 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let y: i32 = s[0..4].parse().ok()?;
    let m: u32 = s[5..7].parse().ok()?;
    let d: u32 = s[8..10].parse().ok()?;
    Some(days_from_civil(y, m, d))
}

/// Convert days since 1970-01-01 back to "YYYY-MM-DD".
pub fn days_to_date(days: i32) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

fn days_from_civil(y: i32, m: u32, d: u32) -> i32 {
    let y = if m <= 2 { y as i64 - 1 } else { y as i64 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe as i64 * 365 + yoe as i64 / 4 - yoe as i64 / 100 + doy;
    (era * 146097 + doe - 719468) as i32
}

fn civil_from_days(days: i32) -> (i32, u32, u32) {
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

// ── Build from RocksDB data ──

impl TableColumns {
    /// Build columnar arrays from a RocksDB prefix scan result.
    pub fn build(all_kv: &[(String, Vec<u8>)], schema_cols: &[ColumnInfo]) -> Self {
        let capacity = all_kv.len();
        let mut row_keys = Vec::with_capacity(capacity);

        // Initialize column arrays
        let mut col_builders: HashMap<u32, ColumnBuilder> = HashMap::new();
        for sc in schema_cols {
            let kind = classify_column(&sc.data_type);
            col_builders.insert(sc.id, ColumnBuilder::new(kind, capacity));
        }

        // Extract columns from each row
        for (key, row_data) in all_kv {
            row_keys.push(key.clone());

            if let Some(view) = RowView::new(row_data) {
                for sc in schema_cols {
                    if let Some(builder) = col_builders.get_mut(&sc.id) {
                        match view.find_col(sc.id) {
                            Some(bytes) if !bytes.is_empty() => builder.push(bytes),
                            _ => builder.push_null(),
                        }
                    }
                }
            } else {
                // Legacy bincode fallback
                let cols: Vec<(u32, Vec<u8>)> = bincode::deserialize(row_data).unwrap_or_default();
                for sc in schema_cols {
                    if let Some(builder) = col_builders.get_mut(&sc.id) {
                        match cols.iter().find(|(id, _)| *id == sc.id) {
                            Some((_, bytes)) if !bytes.is_empty() => builder.push(bytes),
                            _ => builder.push_null(),
                        }
                    }
                }
            }
        }

        // Finalize builders into typed columns
        let columns = col_builders
            .into_iter()
            .map(|(id, builder)| (id, builder.finish()))
            .collect();

        TableColumns {
            row_count: row_keys.len(),
            row_keys,
            columns,
        }
    }

    /// Retrieve a typed value for a specific row and column (for building Row objects).
    pub fn get_typed_value(&self, row_idx: usize, col: &ColumnInfo) -> TypedValue {
        if let Some((typed_col, null_bm)) = self.columns.get(&col.id) {
            if null_bm.is_null(row_idx) {
                return TypedValue::Null;
            }
            match typed_col {
                TypedColumn::Int64(v) => TypedValue::Int(v[row_idx]),
                TypedColumn::Float64(v) => TypedValue::Float(v[row_idx]),
                TypedColumn::DateDays(v) => TypedValue::Text(days_to_date(v[row_idx])),
                TypedColumn::Text(v) => TypedValue::Text(v[row_idx].clone()),
                TypedColumn::Boolean(v) => TypedValue::Boolean(v[row_idx]),
            }
        } else {
            TypedValue::Null
        }
    }
}

// ── Column builder (accumulates values during construction) ──

struct ColumnBuilder {
    kind: ColumnKind,
    ints: Vec<i64>,
    floats: Vec<f64>,
    dates: Vec<i32>,
    texts: Vec<String>,
    bools: Vec<bool>,
    nulls: NullBitmap,
    count: usize,
}

impl ColumnBuilder {
    fn new(kind: ColumnKind, capacity: usize) -> Self {
        Self {
            kind,
            ints: if matches!(kind, ColumnKind::Int64) {
                Vec::with_capacity(capacity)
            } else {
                Vec::new()
            },
            floats: if matches!(kind, ColumnKind::Float64) {
                Vec::with_capacity(capacity)
            } else {
                Vec::new()
            },
            dates: if matches!(kind, ColumnKind::DateDays) {
                Vec::with_capacity(capacity)
            } else {
                Vec::new()
            },
            texts: if matches!(kind, ColumnKind::Text) {
                Vec::with_capacity(capacity)
            } else {
                Vec::new()
            },
            bools: if matches!(kind, ColumnKind::Boolean) {
                Vec::with_capacity(capacity)
            } else {
                Vec::new()
            },
            nulls: NullBitmap::new(capacity),
            count: 0,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        let s = std::str::from_utf8(bytes).unwrap_or("");
        match self.kind {
            ColumnKind::Int64 => {
                self.ints.push(s.parse::<i64>().unwrap_or(0));
            }
            ColumnKind::Float64 => {
                self.floats.push(s.parse::<f64>().unwrap_or(0.0));
            }
            ColumnKind::DateDays => {
                self.dates.push(date_to_days(s).unwrap_or(0));
            }
            ColumnKind::Text => {
                self.texts.push(if s == EMPTY_TEXT_SENTINEL {
                    String::new()
                } else {
                    s.to_string()
                });
            }
            ColumnKind::Boolean => {
                self.bools.push(matches!(s, "true" | "1" | "t"));
            }
        }
        self.count += 1;
    }

    fn push_null(&mut self) {
        self.nulls.set_null(self.count);
        match self.kind {
            ColumnKind::Int64 => self.ints.push(0),
            ColumnKind::Float64 => self.floats.push(0.0),
            ColumnKind::DateDays => self.dates.push(0),
            ColumnKind::Text => self.texts.push(String::new()),
            ColumnKind::Boolean => self.bools.push(false),
        }
        self.count += 1;
    }

    fn finish(self) -> (TypedColumn, NullBitmap) {
        let col = match self.kind {
            ColumnKind::Int64 => TypedColumn::Int64(self.ints),
            ColumnKind::Float64 => TypedColumn::Float64(self.floats),
            ColumnKind::DateDays => TypedColumn::DateDays(self.dates),
            ColumnKind::Text => TypedColumn::Text(self.texts),
            ColumnKind::Boolean => TypedColumn::Boolean(self.bools),
        };
        (col, self.nulls)
    }
}

// ── Bitmap operations ──

/// A bitmask over rows. Bit N set = row N qualifies.
#[derive(Debug, Clone)]
pub struct Bitmap {
    pub words: Vec<u64>,
    pub len: usize,
}

impl Bitmap {
    pub fn all_true(len: usize) -> Self {
        let num_words = (len + 63) / 64;
        let mut words = vec![u64::MAX; num_words];
        // Clear trailing bits in the last word
        let remainder = len % 64;
        if remainder > 0 && !words.is_empty() {
            *words.last_mut().unwrap() = (1u64 << remainder) - 1;
        }
        Self { words, len }
    }

    pub fn all_false(len: usize) -> Self {
        let num_words = (len + 63) / 64;
        Self {
            words: vec![0u64; num_words],
            len,
        }
    }

    #[inline]
    pub fn set(&mut self, index: usize) {
        self.words[index / 64] |= 1u64 << (index % 64);
    }

    #[inline]
    pub fn is_set(&self, index: usize) -> bool {
        (self.words[index / 64] >> (index % 64)) & 1 == 1
    }

    /// Bitwise AND with another bitmap (in place).
    pub fn and_with(&mut self, other: &Bitmap) {
        for (a, b) in self.words.iter_mut().zip(other.words.iter()) {
            *a &= *b;
        }
    }

    /// Count set bits.
    pub fn count_ones(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Iterate over set bit indices.
    pub fn iter_set_bits(&self) -> impl Iterator<Item = usize> + '_ {
        self.words.iter().enumerate().flat_map(|(word_idx, &word)| {
            let base = word_idx * 64;
            BitIter { word, base }
        })
    }
}

struct BitIter {
    word: u64,
    base: usize,
}

impl Iterator for BitIter {
    type Item = usize;

    #[inline]
    fn next(&mut self) -> Option<usize> {
        if self.word == 0 {
            return None;
        }
        let tz = self.word.trailing_zeros() as usize;
        self.word &= self.word - 1; // clear lowest set bit
        Some(self.base + tz)
    }
}

// ── SIMD-friendly filter functions ──
//
// These are tight scalar loops that LLVM auto-vectorizes to NEON (ARM) or AVX2 (x86).
// The key: contiguous arrays, simple comparisons, no branches in the hot path.

#[derive(Debug, Clone, Copy)]
pub enum CmpOp {
    Eq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

/// Filter i64 column against a threshold, producing a bitmap.
pub fn filter_i64(values: &[i64], op: CmpOp, threshold: i64) -> Bitmap {
    let len = values.len();
    let mut bm = Bitmap::all_false(len);
    for (i, &v) in values.iter().enumerate() {
        let pass = match op {
            CmpOp::Eq => v == threshold,
            CmpOp::Lt => v < threshold,
            CmpOp::LtEq => v <= threshold,
            CmpOp::Gt => v > threshold,
            CmpOp::GtEq => v >= threshold,
        };
        // Branchless: convert bool to u64 and OR into bitmap word
        bm.words[i / 64] |= (pass as u64) << (i % 64);
    }
    bm
}

/// Filter i32 column (dates as days-since-epoch).
pub fn filter_i32(values: &[i32], op: CmpOp, threshold: i32) -> Bitmap {
    let len = values.len();
    let mut bm = Bitmap::all_false(len);
    for (i, &v) in values.iter().enumerate() {
        let pass = match op {
            CmpOp::Eq => v == threshold,
            CmpOp::Lt => v < threshold,
            CmpOp::LtEq => v <= threshold,
            CmpOp::Gt => v > threshold,
            CmpOp::GtEq => v >= threshold,
        };
        bm.words[i / 64] |= (pass as u64) << (i % 64);
    }
    bm
}

/// Filter f64 column against a threshold.
pub fn filter_f64(values: &[f64], op: CmpOp, threshold: f64) -> Bitmap {
    let len = values.len();
    let mut bm = Bitmap::all_false(len);
    for (i, &v) in values.iter().enumerate() {
        let pass = match op {
            CmpOp::Eq => v == threshold,
            CmpOp::Lt => v < threshold,
            CmpOp::LtEq => v <= threshold,
            CmpOp::Gt => v > threshold,
            CmpOp::GtEq => v >= threshold,
        };
        bm.words[i / 64] |= (pass as u64) << (i % 64);
    }
    bm
}

/// Filter text column with string comparison.
pub fn filter_text(values: &[String], op: CmpOp, threshold: &str) -> Bitmap {
    let len = values.len();
    let mut bm = Bitmap::all_false(len);
    for (i, v) in values.iter().enumerate() {
        let pass = match op {
            CmpOp::Eq => v.as_str() == threshold,
            CmpOp::Lt => v.as_str() < threshold,
            CmpOp::LtEq => v.as_str() <= threshold,
            CmpOp::Gt => v.as_str() > threshold,
            CmpOp::GtEq => v.as_str() >= threshold,
        };
        bm.words[i / 64] |= (pass as u64) << (i % 64);
    }
    bm
}

/// Apply a typed filter on a TypedColumn, dispatching to the right SIMD function.
pub fn filter_column(
    col: &TypedColumn,
    null_bm: &NullBitmap,
    op: CmpOp,
    literal: &TypedValue,
) -> Bitmap {
    let bm = match (col, literal) {
        (TypedColumn::Int64(vals), TypedValue::Int(threshold)) => filter_i64(vals, op, *threshold),
        (TypedColumn::Int64(vals), TypedValue::Float(threshold)) => {
            // Promote to f64 comparison
            let f_vals: Vec<f64> = vals.iter().map(|&v| v as f64).collect();
            filter_f64(&f_vals, op, *threshold)
        }
        (TypedColumn::Float64(vals), TypedValue::Float(threshold)) => {
            filter_f64(vals, op, *threshold)
        }
        (TypedColumn::Float64(vals), TypedValue::Int(threshold)) => {
            filter_f64(vals, op, *threshold as f64)
        }
        (TypedColumn::DateDays(vals), TypedValue::Text(date_str)) => {
            if let Some(days) = date_to_days(date_str) {
                filter_i32(vals, op, days)
            } else {
                Bitmap::all_false(vals.len())
            }
        }
        (TypedColumn::Text(vals), TypedValue::Text(threshold)) => filter_text(vals, op, threshold),
        _ => {
            // Unsupported type combination — return empty (no matches)
            let len = match col {
                TypedColumn::Int64(v) => v.len(),
                TypedColumn::Float64(v) => v.len(),
                TypedColumn::DateDays(v) => v.len(),
                TypedColumn::Text(v) => v.len(),
                TypedColumn::Boolean(v) => v.len(),
            };
            Bitmap::all_false(len)
        }
    };

    // Mask out NULLs: NULL comparisons always return false
    let mut result = bm;
    for (rw, nw) in result.words.iter_mut().zip(null_bm.words.iter()) {
        *rw &= !nw; // clear bits where null bitmap is set
    }
    result
}

// ── Columnar aggregation (avoid building Row for aggregate-only queries) ──

/// Compute SUM over an f64 column for rows matching a bitmap.
pub fn sum_f64_masked(values: &[f64], bitmap: &Bitmap) -> f64 {
    let mut sum = 0.0f64;
    for idx in bitmap.iter_set_bits() {
        sum += values[idx];
    }
    sum
}

/// Compute SUM of (a * b) for rows matching a bitmap (e.g., SUM(price * discount)).
pub fn sum_product_f64_masked(a: &[f64], b: &[f64], bitmap: &Bitmap) -> f64 {
    let mut sum = 0.0f64;
    for idx in bitmap.iter_set_bits() {
        sum += a[idx] * b[idx];
    }
    sum
}

/// Count set bits in a bitmap.
pub fn count_masked(bitmap: &Bitmap) -> i64 {
    bitmap.count_ones() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_roundtrip() {
        for date in [
            "1970-01-01",
            "2024-06-15",
            "1994-01-01",
            "2000-02-29",
            "1900-01-01",
        ] {
            let days = date_to_days(date).unwrap();
            assert_eq!(days_to_date(days), date, "roundtrip failed for {}", date);
        }
    }

    #[test]
    fn date_ordering() {
        let d1 = date_to_days("1994-01-01").unwrap();
        let d2 = date_to_days("1995-01-01").unwrap();
        assert!(d1 < d2);
        assert_eq!(d2 - d1, 365);
    }

    #[test]
    fn epoch_is_zero() {
        assert_eq!(date_to_days("1970-01-01").unwrap(), 0);
    }

    #[test]
    fn bitmap_basic() {
        let mut bm = Bitmap::all_false(100);
        assert_eq!(bm.count_ones(), 0);
        bm.set(0);
        bm.set(50);
        bm.set(99);
        assert_eq!(bm.count_ones(), 3);
        assert!(bm.is_set(0));
        assert!(bm.is_set(50));
        assert!(!bm.is_set(1));
    }

    #[test]
    fn bitmap_all_true() {
        let bm = Bitmap::all_true(100);
        assert_eq!(bm.count_ones(), 100);
        assert!(bm.is_set(0));
        assert!(bm.is_set(99));
    }

    #[test]
    fn bitmap_and() {
        let mut a = Bitmap::all_true(100);
        let mut b = Bitmap::all_false(100);
        b.set(5);
        b.set(10);
        a.and_with(&b);
        assert_eq!(a.count_ones(), 2);
        assert!(a.is_set(5));
        assert!(a.is_set(10));
    }

    #[test]
    fn bitmap_iter_set_bits() {
        let mut bm = Bitmap::all_false(200);
        bm.set(0);
        bm.set(63);
        bm.set(64);
        bm.set(127);
        bm.set(199);
        let bits: Vec<usize> = bm.iter_set_bits().collect();
        assert_eq!(bits, vec![0, 63, 64, 127, 199]);
    }

    #[test]
    fn filter_i64_lt() {
        let values = vec![10, 20, 5, 30, 15];
        let bm = filter_i64(&values, CmpOp::Lt, 16);
        let indices: Vec<usize> = bm.iter_set_bits().collect();
        assert_eq!(indices, vec![0, 2, 4]); // 10, 5, 15
    }

    #[test]
    fn filter_i64_ge() {
        let values = vec![10, 20, 5, 30, 15];
        let bm = filter_i64(&values, CmpOp::GtEq, 15);
        let indices: Vec<usize> = bm.iter_set_bits().collect();
        assert_eq!(indices, vec![1, 3, 4]); // 20, 30, 15
    }

    #[test]
    fn filter_f64_between() {
        let values = vec![0.03, 0.05, 0.06, 0.07, 0.08, 0.04];
        let bm_ge = filter_f64(&values, CmpOp::GtEq, 0.05);
        let bm_le = filter_f64(&values, CmpOp::LtEq, 0.07);
        let mut bm = bm_ge;
        bm.and_with(&bm_le);
        let indices: Vec<usize> = bm.iter_set_bits().collect();
        assert_eq!(indices, vec![1, 2, 3]); // 0.05, 0.06, 0.07
    }

    #[test]
    fn filter_date_range() {
        let dates = vec![
            date_to_days("1993-12-31").unwrap(),
            date_to_days("1994-01-01").unwrap(),
            date_to_days("1994-06-15").unwrap(),
            date_to_days("1995-01-01").unwrap(),
            date_to_days("1995-01-02").unwrap(),
        ];
        let d1 = date_to_days("1994-01-01").unwrap();
        let d2 = date_to_days("1995-01-01").unwrap();
        let bm_ge = filter_i32(&dates, CmpOp::GtEq, d1);
        let bm_lt = filter_i32(&dates, CmpOp::Lt, d2);
        let mut bm = bm_ge;
        bm.and_with(&bm_lt);
        let indices: Vec<usize> = bm.iter_set_bits().collect();
        assert_eq!(indices, vec![1, 2]); // 1994-01-01 and 1994-06-15
    }

    #[test]
    fn filter_column_null_handling() {
        let values = vec![10, 20, 30];
        let mut null_bm = NullBitmap::new(3);
        null_bm.set_null(1); // row 1 is NULL
        let bm = filter_column(
            &TypedColumn::Int64(values),
            &null_bm,
            CmpOp::GtEq,
            &TypedValue::Int(10),
        );
        let indices: Vec<usize> = bm.iter_set_bits().collect();
        assert_eq!(indices, vec![0, 2]); // row 1 excluded due to NULL
    }

    #[test]
    fn sum_f64_masked_test() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let mut bm = Bitmap::all_false(5);
        bm.set(0);
        bm.set(2);
        bm.set(4);
        assert_eq!(sum_f64_masked(&values, &bm), 9.0);
    }

    #[test]
    fn sum_product_masked_test() {
        let a = vec![100.0, 200.0, 300.0];
        let b = vec![0.05, 0.06, 0.07];
        let mut bm = Bitmap::all_false(3);
        bm.set(0);
        bm.set(2);
        let result = sum_product_f64_masked(&a, &b, &bm);
        assert!((result - 26.0).abs() < 1e-10); // 100*0.05 + 300*0.07
    }
}
