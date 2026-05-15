use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use sqlparser::ast::{BinaryOperator, DataType};
use std::cmp::Ordering;
use std::fmt;

pub const EMPTY_TEXT_SENTINEL: &str = "\u{0000}";

/// Runtime representation of a column value with type information.
#[derive(Debug, Clone, PartialEq)]
pub enum TypedValue {
    Int(i64),
    Float(f64),
    Decimal(Decimal),
    Date(NaiveDate),
    Timestamp(NaiveDateTime),
    Time(NaiveTime),
    Text(String),
    Boolean(bool),
    Null,
}

impl TypedValue {
    /// Deserialize raw Kvrocks bytes into a typed value based on the column's data type string.
    pub fn from_bytes(bytes: &[u8], data_type: &str) -> Self {
        if bytes.is_empty() {
            return TypedValue::Null;
        }

        let s = String::from_utf8_lossy(bytes);
        let dt_upper = data_type.to_uppercase();

        match dt_upper.as_str() {
            "TINYINT" | "SMALLINT" | "INT" | "INTEGER" | "BIGINT" => s
                .parse::<i64>()
                .map(TypedValue::Int)
                .unwrap_or(TypedValue::Text(s.into_owned())),
            "FLOAT" | "DOUBLE" | "REAL" => s
                .parse::<f64>()
                .map(TypedValue::Float)
                .unwrap_or(TypedValue::Text(s.into_owned())),
            "BOOLEAN" | "BOOL" => match s.as_ref() {
                "true" | "1" | "t" => TypedValue::Boolean(true),
                "false" | "0" | "f" => TypedValue::Boolean(false),
                _ => TypedValue::Text(s.into_owned()),
            },
            // DECIMAL/NUMERIC are preserved as exact decimals in runtime operations.
            dt if dt.starts_with("DECIMAL") || dt.starts_with("NUMERIC") => s
                .parse::<Decimal>()
                .map(TypedValue::Decimal)
                .unwrap_or(TypedValue::Text(s.into_owned())),
            "DATE" => parse_date_text(s.as_ref())
                .map(TypedValue::Date)
                .unwrap_or(TypedValue::Text(s.into_owned())),
            "DATETIME" | "TIMESTAMP" => parse_timestamp_text(s.as_ref())
                .map(TypedValue::Timestamp)
                .unwrap_or(TypedValue::Text(s.into_owned())),
            "TIME" => parse_time_text(s.as_ref())
                .map(TypedValue::Time)
                .unwrap_or(TypedValue::Text(s.into_owned())),
            // VARCHAR, CHAR, TEXT fall back to text storage.
            _ => {
                if s.as_ref() == EMPTY_TEXT_SENTINEL {
                    TypedValue::Text(String::new())
                } else {
                    TypedValue::Text(s.into_owned())
                }
            }
        }
    }

    /// Returns true if this value is NULL.
    pub fn is_null(&self) -> bool {
        matches!(self, TypedValue::Null)
    }

    /// Attempt numeric coercion for arithmetic operations.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            TypedValue::Int(i) => Some(*i as f64),
            TypedValue::Float(f) => Some(*f),
            TypedValue::Decimal(d) => d.to_f64(),
            TypedValue::Text(s) => s.parse::<f64>().ok(),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            TypedValue::Int(i) => Some(*i),
            TypedValue::Float(f) if f.is_finite() => Some(*f as i64),
            TypedValue::Decimal(d) => d.to_i64(),
            TypedValue::Text(s) => s.parse::<i64>().ok(),
            _ => None,
        }
    }

    pub fn as_decimal(&self) -> Option<Decimal> {
        match self {
            TypedValue::Int(i) => Some(Decimal::from(*i)),
            TypedValue::Float(f) => Decimal::from_f64(*f),
            TypedValue::Decimal(d) => Some(*d),
            TypedValue::Text(s) => s.parse::<Decimal>().ok(),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            TypedValue::Boolean(b) => Some(*b),
            TypedValue::Text(s) => parse_bool_text(s),
            _ => None,
        }
    }

    pub fn as_date(&self) -> Option<NaiveDate> {
        match self {
            TypedValue::Date(date) => Some(*date),
            TypedValue::Timestamp(ts) => Some(ts.date()),
            TypedValue::Text(text) => parse_date_text(text),
            _ => None,
        }
    }

    pub fn as_timestamp(&self) -> Option<NaiveDateTime> {
        match self {
            TypedValue::Date(date) => date.and_hms_opt(0, 0, 0),
            TypedValue::Timestamp(ts) => Some(*ts),
            TypedValue::Text(text) => parse_timestamp_text(text)
                .or_else(|| parse_date_text(text).and_then(|date| date.and_hms_opt(0, 0, 0))),
            _ => None,
        }
    }

    pub fn as_time(&self) -> Option<NaiveTime> {
        match self {
            TypedValue::Time(time) => Some(*time),
            TypedValue::Timestamp(ts) => Some(ts.time()),
            TypedValue::Text(text) => parse_time_text(text),
            _ => None,
        }
    }

    pub fn cast_to_data_type(&self, data_type: &DataType) -> Result<Self, String> {
        if self.is_null() {
            return Ok(TypedValue::Null);
        }

        match data_type {
            DataType::Char(_)
            | DataType::Character(_)
            | DataType::CharacterVarying(_)
            | DataType::CharVarying(_)
            | DataType::Varchar(_)
            | DataType::Nvarchar(_)
            | DataType::Text
            | DataType::TinyText
            | DataType::MediumText
            | DataType::LongText
            | DataType::CharacterLargeObject(_)
            | DataType::CharLargeObject(_)
            | DataType::Clob(_)
            | DataType::String(_) => Ok(TypedValue::Text(self.to_string())),
            DataType::TinyInt(_)
            | DataType::Int2(_)
            | DataType::SmallInt(_)
            | DataType::Int(_)
            | DataType::Int4(_)
            | DataType::Integer(_)
            | DataType::Int8(_)
            | DataType::BigInt(_)
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::Int128
            | DataType::Int256 => self
                .as_i64()
                .map(TypedValue::Int)
                .ok_or_else(|| format!("cannot cast '{}' to {}", self, data_type)),
            DataType::Float(_)
            | DataType::Real
            | DataType::Float4
            | DataType::Float8
            | DataType::Double
            | DataType::DoublePrecision => self
                .as_f64()
                .map(TypedValue::Float)
                .ok_or_else(|| format!("cannot cast '{}' to {}", self, data_type)),
            DataType::Numeric(_)
            | DataType::Decimal(_)
            | DataType::BigNumeric(_)
            | DataType::BigDecimal(_)
            | DataType::Dec(_) => self
                .as_decimal()
                .map(TypedValue::Decimal)
                .ok_or_else(|| format!("cannot cast '{}' to {}", self, data_type)),
            DataType::Bool | DataType::Boolean => self
                .as_bool()
                .map(TypedValue::Boolean)
                .ok_or_else(|| format!("cannot cast '{}' to {}", self, data_type)),
            DataType::Date => self
                .as_date()
                .map(TypedValue::Date)
                .ok_or_else(|| format!("cannot cast '{}' to {}", self, data_type)),
            DataType::Datetime(_) | DataType::Timestamp(_, _) => self
                .as_timestamp()
                .map(TypedValue::Timestamp)
                .ok_or_else(|| format!("cannot cast '{}' to {}", self, data_type)),
            DataType::Time(_, _) => self
                .as_time()
                .map(TypedValue::Time)
                .ok_or_else(|| format!("cannot cast '{}' to {}", self, data_type)),
            // PostgreSQL pseudo-types (regclass, regtype, oid, etc.) — return as text.
            // DBeaver and other clients use these in catalog queries.
            DataType::Custom(_, _) => Ok(TypedValue::Text(self.to_string())),
            _ => Err(format!("CAST to {} is not supported", data_type)),
        }
    }

    pub fn supports_cast_type(data_type: &DataType) -> bool {
        matches!(
            data_type,
            DataType::Char(_)
                | DataType::Character(_)
                | DataType::CharacterVarying(_)
                | DataType::CharVarying(_)
                | DataType::Varchar(_)
                | DataType::Nvarchar(_)
                | DataType::Text
                | DataType::TinyText
                | DataType::MediumText
                | DataType::LongText
                | DataType::CharacterLargeObject(_)
                | DataType::CharLargeObject(_)
                | DataType::Clob(_)
                | DataType::TinyInt(_)
                | DataType::Int2(_)
                | DataType::SmallInt(_)
                | DataType::Int(_)
                | DataType::Int4(_)
                | DataType::Integer(_)
                | DataType::Int8(_)
                | DataType::BigInt(_)
                | DataType::Int16
                | DataType::Int32
                | DataType::Int64
                | DataType::Int128
                | DataType::Int256
                | DataType::Float(_)
                | DataType::Real
                | DataType::Float4
                | DataType::Float8
                | DataType::Double
                | DataType::DoublePrecision
                | DataType::Numeric(_)
                | DataType::Decimal(_)
                | DataType::BigNumeric(_)
                | DataType::BigDecimal(_)
                | DataType::Dec(_)
                | DataType::Bool
                | DataType::Boolean
                | DataType::Date
                | DataType::Datetime(_)
                | DataType::Timestamp(_, _)
                | DataType::Time(_, _)
                | DataType::String(_)
                | DataType::Custom(_, _)
        )
    }

    /// Compare two values. NULL is always "less than" non-NULL for ORDER BY purposes.
    pub fn cmp_order(&self, other: &TypedValue) -> Ordering {
        match (self, other) {
            (TypedValue::Null, TypedValue::Null) => Ordering::Equal,
            (TypedValue::Null, _) => Ordering::Less,
            (_, TypedValue::Null) => Ordering::Greater,
            (TypedValue::Int(a), TypedValue::Int(b)) => a.cmp(b),
            (TypedValue::Float(a), TypedValue::Float(b)) => {
                a.partial_cmp(b).unwrap_or(Ordering::Equal)
            }
            (TypedValue::Decimal(a), TypedValue::Decimal(b)) => a.cmp(b),
            (TypedValue::Date(a), TypedValue::Date(b)) => a.cmp(b),
            (TypedValue::Timestamp(a), TypedValue::Timestamp(b)) => a.cmp(b),
            (TypedValue::Time(a), TypedValue::Time(b)) => a.cmp(b),
            (TypedValue::Int(a), TypedValue::Float(b)) => {
                (*a as f64).partial_cmp(b).unwrap_or(Ordering::Equal)
            }
            (TypedValue::Float(a), TypedValue::Int(b)) => {
                a.partial_cmp(&(*b as f64)).unwrap_or(Ordering::Equal)
            }
            (TypedValue::Decimal(a), TypedValue::Int(b)) => a.cmp(&Decimal::from(*b)),
            (TypedValue::Int(a), TypedValue::Decimal(b)) => Decimal::from(*a).cmp(b),
            (TypedValue::Decimal(a), TypedValue::Float(b)) => Decimal::from_f64(*b)
                .map(|rhs| a.cmp(&rhs))
                .unwrap_or(Ordering::Equal),
            (TypedValue::Float(a), TypedValue::Decimal(b)) => Decimal::from_f64(*a)
                .map(|lhs| lhs.cmp(b))
                .unwrap_or(Ordering::Equal),
            (TypedValue::Decimal(a), TypedValue::Text(b)) => b
                .parse::<Decimal>()
                .map(|rhs| a.cmp(&rhs))
                .unwrap_or_else(|_| format!("{self}").cmp(&format!("{other}"))),
            (TypedValue::Text(a), TypedValue::Decimal(b)) => a
                .parse::<Decimal>()
                .map(|lhs| lhs.cmp(b))
                .unwrap_or_else(|_| format!("{self}").cmp(&format!("{other}"))),
            (TypedValue::Date(_), TypedValue::Text(_))
            | (TypedValue::Text(_), TypedValue::Date(_))
            | (TypedValue::Date(_), TypedValue::Timestamp(_))
            | (TypedValue::Timestamp(_), TypedValue::Date(_))
            | (TypedValue::Timestamp(_), TypedValue::Text(_))
            | (TypedValue::Text(_), TypedValue::Timestamp(_)) => {
                match (self.as_timestamp(), other.as_timestamp()) {
                    (Some(lhs), Some(rhs)) => lhs.cmp(&rhs),
                    _ => format!("{self}").cmp(&format!("{other}")),
                }
            }
            (TypedValue::Time(_), TypedValue::Text(_))
            | (TypedValue::Text(_), TypedValue::Time(_)) => {
                match (self.as_time(), other.as_time()) {
                    (Some(lhs), Some(rhs)) => lhs.cmp(&rhs),
                    _ => format!("{self}").cmp(&format!("{other}")),
                }
            }
            (TypedValue::Text(a), TypedValue::Text(b)) => a.cmp(b),
            (TypedValue::Boolean(a), TypedValue::Boolean(b)) => a.cmp(b),
            // Cross-type: fall back to string representation
            _ => format!("{self}").cmp(&format!("{other}")),
        }
    }

    /// Equality check for WHERE predicates. NULL = NULL is false (SQL semantics).
    pub fn sql_eq(&self, other: &TypedValue) -> bool {
        if self.is_null() || other.is_null() {
            return false;
        }
        match (self, other) {
            (TypedValue::Int(a), TypedValue::Int(b)) => a == b,
            (TypedValue::Float(a), TypedValue::Float(b)) => a == b,
            (TypedValue::Decimal(a), TypedValue::Decimal(b)) => a == b,
            (TypedValue::Int(a), TypedValue::Float(b)) => (*a as f64) == *b,
            (TypedValue::Float(a), TypedValue::Int(b)) => *a == (*b as f64),
            (TypedValue::Decimal(a), TypedValue::Int(b)) => *a == Decimal::from(*b),
            (TypedValue::Int(a), TypedValue::Decimal(b)) => Decimal::from(*a) == *b,
            (TypedValue::Date(a), TypedValue::Date(b)) => a == b,
            (TypedValue::Timestamp(a), TypedValue::Timestamp(b)) => a == b,
            (TypedValue::Time(a), TypedValue::Time(b)) => a == b,
            (TypedValue::Decimal(a), TypedValue::Float(b)) => {
                Decimal::from_f64(*b).map(|rhs| *a == rhs).unwrap_or(false)
            }
            (TypedValue::Float(a), TypedValue::Decimal(b)) => {
                Decimal::from_f64(*a).map(|lhs| lhs == *b).unwrap_or(false)
            }
            (TypedValue::Decimal(a), TypedValue::Text(b)) => {
                b.parse::<Decimal>().map(|rhs| *a == rhs).unwrap_or(false)
            }
            (TypedValue::Text(a), TypedValue::Decimal(b)) => {
                a.parse::<Decimal>().map(|lhs| lhs == *b).unwrap_or(false)
            }
            (TypedValue::Text(a), TypedValue::Text(b)) => a == b,
            (TypedValue::Boolean(a), TypedValue::Boolean(b)) => a == b,
            // Allow string-to-number comparison for WHERE col = '42'
            (TypedValue::Int(a), TypedValue::Text(b)) => {
                b.parse::<i64>().map_or(false, |v| *a == v)
            }
            (TypedValue::Text(a), TypedValue::Int(b)) => {
                a.parse::<i64>().map_or(false, |v| v == *b)
            }
            (TypedValue::Date(_), TypedValue::Text(_))
            | (TypedValue::Text(_), TypedValue::Date(_))
            | (TypedValue::Date(_), TypedValue::Timestamp(_))
            | (TypedValue::Timestamp(_), TypedValue::Date(_))
            | (TypedValue::Timestamp(_), TypedValue::Text(_))
            | (TypedValue::Text(_), TypedValue::Timestamp(_)) => {
                matches!((self.as_timestamp(), other.as_timestamp()), (Some(lhs), Some(rhs)) if lhs == rhs)
            }
            (TypedValue::Time(_), TypedValue::Text(_))
            | (TypedValue::Text(_), TypedValue::Time(_)) => {
                matches!((self.as_time(), other.as_time()), (Some(lhs), Some(rhs)) if lhs == rhs)
            }
            _ => false,
        }
    }

    /// Less-than for WHERE predicates. NULL comparisons return false.
    pub fn sql_lt(&self, other: &TypedValue) -> bool {
        if self.is_null() || other.is_null() {
            return false;
        }
        self.cmp_order(other) == Ordering::Less
    }

    /// Greater-than for WHERE predicates.
    pub fn sql_gt(&self, other: &TypedValue) -> bool {
        if self.is_null() || other.is_null() {
            return false;
        }
        self.cmp_order(other) == Ordering::Greater
    }

    /// SQL LIKE pattern matching with `%` / `_` wildcards.
    pub fn sql_like(&self, pattern: &str) -> bool {
        self.sql_like_with_options(pattern, None, false)
    }

    pub fn sql_ilike(&self, pattern: &str) -> bool {
        self.sql_like_with_options(pattern, None, true)
    }

    pub fn sql_like_with_options(
        &self,
        pattern: &str,
        escape_char: Option<&str>,
        case_insensitive: bool,
    ) -> bool {
        let val = match self {
            TypedValue::Text(s) => s.as_str(),
            _ => return false,
        };

        sql_pattern_matches(val, pattern, escape_char, case_insensitive)
    }

    pub fn negate_numeric(&self) -> TypedValue {
        match self {
            TypedValue::Int(i) => TypedValue::Int(-i),
            TypedValue::Float(f) => TypedValue::Float(-f),
            TypedValue::Decimal(d) => TypedValue::Decimal(-*d),
            _ => TypedValue::Null,
        }
    }

    pub fn apply_arithmetic(&self, op: &BinaryOperator, other: &TypedValue) -> TypedValue {
        if self.is_null() || other.is_null() {
            return TypedValue::Null;
        }

        if matches!(self, TypedValue::Decimal(_)) || matches!(other, TypedValue::Decimal(_)) {
            let Some(left_num) = self.as_decimal() else {
                return TypedValue::Null;
            };
            let Some(right_num) = other.as_decimal() else {
                return TypedValue::Null;
            };
            let result = match op {
                BinaryOperator::Plus => Some(left_num + right_num),
                BinaryOperator::Minus => Some(left_num - right_num),
                BinaryOperator::Multiply => Some(left_num * right_num),
                BinaryOperator::Divide => {
                    if right_num == Decimal::ZERO {
                        None
                    } else {
                        Some(left_num / right_num)
                    }
                }
                BinaryOperator::Modulo => {
                    if right_num == Decimal::ZERO {
                        None
                    } else {
                        Some(left_num % right_num)
                    }
                }
                _ => None,
            };
            return result.map(TypedValue::Decimal).unwrap_or(TypedValue::Null);
        }

        let Some(left_num) = self.as_f64() else {
            return TypedValue::Null;
        };
        let Some(right_num) = other.as_f64() else {
            return TypedValue::Null;
        };
        let result = match op {
            BinaryOperator::Plus => left_num + right_num,
            BinaryOperator::Minus => left_num - right_num,
            BinaryOperator::Multiply => left_num * right_num,
            BinaryOperator::Divide => {
                if right_num == 0.0 {
                    return TypedValue::Null;
                }
                left_num / right_num
            }
            BinaryOperator::Modulo => {
                if right_num == 0.0 {
                    return TypedValue::Null;
                }
                left_num % right_num
            }
            _ => return TypedValue::Null,
        };
        if matches!(self, TypedValue::Int(_))
            && matches!(other, TypedValue::Int(_))
            && result == (result as i64) as f64
            && !matches!(op, BinaryOperator::Divide)
        {
            TypedValue::Int(result as i64)
        } else {
            TypedValue::Float(result)
        }
    }
}

fn sql_pattern_matches(
    value: &str,
    pattern: &str,
    escape_char: Option<&str>,
    case_insensitive: bool,
) -> bool {
    let value_chars: Vec<char> = if case_insensitive {
        value.to_lowercase().chars().collect()
    } else {
        value.chars().collect()
    };
    let pattern_chars: Vec<char> = if case_insensitive {
        pattern.to_lowercase().chars().collect()
    } else {
        pattern.chars().collect()
    };
    let escape = escape_char.and_then(|s| s.chars().next()).unwrap_or('\\');

    let mut tokens = Vec::new();
    let mut idx = 0usize;
    while idx < pattern_chars.len() {
        let ch = pattern_chars[idx];
        if ch == escape && idx + 1 < pattern_chars.len() {
            idx += 1;
            tokens.push(PatternToken::Literal(pattern_chars[idx]));
        } else if ch == '%' {
            tokens.push(PatternToken::AnySequence);
        } else if ch == '_' {
            tokens.push(PatternToken::AnySingle);
        } else {
            tokens.push(PatternToken::Literal(ch));
        }
        idx += 1;
    }

    let mut dp = vec![vec![false; value_chars.len() + 1]; tokens.len() + 1];
    dp[0][0] = true;

    for token_idx in 1..=tokens.len() {
        if matches!(tokens[token_idx - 1], PatternToken::AnySequence) {
            dp[token_idx][0] = dp[token_idx - 1][0];
        }
    }

    for token_idx in 1..=tokens.len() {
        for value_idx in 1..=value_chars.len() {
            dp[token_idx][value_idx] = match tokens[token_idx - 1] {
                PatternToken::AnySequence => {
                    dp[token_idx - 1][value_idx] || dp[token_idx][value_idx - 1]
                }
                PatternToken::AnySingle => dp[token_idx - 1][value_idx - 1],
                PatternToken::Literal(expected) => {
                    dp[token_idx - 1][value_idx - 1] && value_chars[value_idx - 1] == expected
                }
            };
        }
    }

    dp[tokens.len()][value_chars.len()]
}

#[derive(Debug, Clone, Copy)]
enum PatternToken {
    Literal(char),
    AnySequence,
    AnySingle,
}

fn parse_bool_text(input: &str) -> Option<bool> {
    match input.trim().to_ascii_lowercase().as_str() {
        "true" | "t" | "1" => Some(true),
        "false" | "f" | "0" => Some(false),
        _ => None,
    }
}

impl fmt::Display for TypedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypedValue::Int(i) => write!(f, "{i}"),
            TypedValue::Float(v) => write!(f, "{v}"),
            TypedValue::Decimal(v) => write!(f, "{v}"),
            TypedValue::Date(date) => write!(f, "{}", date.format("%Y-%m-%d")),
            TypedValue::Timestamp(ts) => write!(f, "{}", ts.format("%Y-%m-%d %H:%M:%S%.f")),
            TypedValue::Time(time) => write!(f, "{}", time.format("%H:%M:%S%.f")),
            TypedValue::Text(s) => write!(f, "{s}"),
            TypedValue::Boolean(b) => write!(f, "{b}"),
            TypedValue::Null => write!(f, "NULL"),
        }
    }
}

fn parse_date_text(input: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(input.trim(), "%Y-%m-%d").ok()
}

fn parse_timestamp_text(input: &str) -> Option<NaiveDateTime> {
    let input = input.trim();
    const FORMATS: &[&str] = &[
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
    ];
    FORMATS
        .iter()
        .find_map(|fmt| NaiveDateTime::parse_from_str(input, fmt).ok())
}

fn parse_time_text(input: &str) -> Option<NaiveTime> {
    let input = input.trim();
    const FORMATS: &[&str] = &["%H:%M:%S%.f", "%H:%M:%S"];
    FORMATS
        .iter()
        .find_map(|fmt| NaiveTime::parse_from_str(input, fmt).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::ast::DataType;

    #[test]
    fn from_bytes_integer_types() {
        assert_eq!(TypedValue::from_bytes(b"42", "BIGINT"), TypedValue::Int(42));
        assert_eq!(TypedValue::from_bytes(b"-1", "INT"), TypedValue::Int(-1));
        assert_eq!(TypedValue::from_bytes(b"0", "SMALLINT"), TypedValue::Int(0));
    }

    #[test]
    fn from_bytes_float_types() {
        assert_eq!(
            TypedValue::from_bytes(b"3.14", "FLOAT"),
            TypedValue::Float(3.14)
        );
        assert_eq!(
            TypedValue::from_bytes(b"2.0", "DOUBLE"),
            TypedValue::Float(2.0)
        );
    }

    #[test]
    fn from_bytes_decimal() {
        assert_eq!(
            TypedValue::from_bytes(b"99.95", "DECIMAL(10,2)"),
            TypedValue::Decimal("99.95".parse().unwrap())
        );
    }

    #[test]
    fn from_bytes_text_types() {
        assert_eq!(
            TypedValue::from_bytes(b"hello", "VARCHAR(255)"),
            TypedValue::Text("hello".to_string())
        );
        assert_eq!(
            TypedValue::from_bytes(EMPTY_TEXT_SENTINEL.as_bytes(), "TEXT"),
            TypedValue::Text(String::new())
        );
        assert_eq!(
            TypedValue::from_bytes(b"2024-01-15", "DATE"),
            TypedValue::Date(NaiveDate::from_ymd_opt(2024, 1, 15).unwrap())
        );
    }

    #[test]
    fn from_bytes_timestamp_and_time_types() {
        assert_eq!(
            TypedValue::from_bytes(b"2024-01-15 10:11:12", "TIMESTAMP"),
            TypedValue::Timestamp(
                NaiveDate::from_ymd_opt(2024, 1, 15)
                    .unwrap()
                    .and_hms_opt(10, 11, 12)
                    .unwrap()
            )
        );
        assert_eq!(
            TypedValue::from_bytes(b"10:11:12", "TIME"),
            TypedValue::Time(NaiveTime::from_hms_opt(10, 11, 12).unwrap())
        );
    }

    #[test]
    fn from_bytes_boolean() {
        assert_eq!(
            TypedValue::from_bytes(b"true", "BOOLEAN"),
            TypedValue::Boolean(true)
        );
        assert_eq!(
            TypedValue::from_bytes(b"0", "BOOLEAN"),
            TypedValue::Boolean(false)
        );
    }

    #[test]
    fn from_bytes_empty_is_null() {
        assert_eq!(TypedValue::from_bytes(b"", "BIGINT"), TypedValue::Null);
    }

    #[test]
    fn sql_eq_null_semantics() {
        assert!(!TypedValue::Null.sql_eq(&TypedValue::Null));
        assert!(!TypedValue::Null.sql_eq(&TypedValue::Int(1)));
    }

    #[test]
    fn sql_eq_cross_type_int_text() {
        assert!(TypedValue::Int(42).sql_eq(&TypedValue::Text("42".to_string())));
    }

    #[test]
    fn sql_eq_numeric_coercion() {
        assert!(TypedValue::Int(3).sql_eq(&TypedValue::Float(3.0)));
    }

    #[test]
    fn sql_eq_decimal_is_exact() {
        let left = TypedValue::Decimal("0.3".parse().unwrap());
        let right = TypedValue::Decimal("0.30".parse().unwrap());
        assert!(left.sql_eq(&right));
    }

    #[test]
    fn cmp_order_decimal_is_numeric() {
        let left = TypedValue::Decimal("2.10".parse().unwrap());
        let right = TypedValue::Decimal("10.01".parse().unwrap());
        assert_eq!(left.cmp_order(&right), Ordering::Less);
    }

    #[test]
    fn decimal_arithmetic_stays_exact() {
        let left = TypedValue::Decimal("0.10".parse().unwrap());
        let right = TypedValue::Decimal("0.20".parse().unwrap());
        assert_eq!(
            left.apply_arithmetic(&BinaryOperator::Plus, &right),
            TypedValue::Decimal("0.30".parse().unwrap())
        );
    }

    #[test]
    fn cmp_order_nulls_sort_first() {
        assert_eq!(
            TypedValue::Null.cmp_order(&TypedValue::Int(1)),
            Ordering::Less
        );
        assert_eq!(
            TypedValue::Int(1).cmp_order(&TypedValue::Null),
            Ordering::Greater
        );
    }

    #[test]
    fn cmp_order_integers() {
        assert_eq!(
            TypedValue::Int(1).cmp_order(&TypedValue::Int(2)),
            Ordering::Less
        );
        assert_eq!(
            TypedValue::Int(5).cmp_order(&TypedValue::Int(5)),
            Ordering::Equal
        );
    }

    #[test]
    fn cmp_order_text() {
        let a = TypedValue::Text("alice".to_string());
        let b = TypedValue::Text("bob".to_string());
        assert_eq!(a.cmp_order(&b), Ordering::Less);
    }

    #[test]
    fn like_prefix() {
        let v = TypedValue::Text("alice".to_string());
        assert!(v.sql_like("ali%"));
        assert!(!v.sql_like("bob%"));
    }

    #[test]
    fn like_suffix() {
        let v = TypedValue::Text("alice".to_string());
        assert!(v.sql_like("%ice"));
        assert!(!v.sql_like("%bob"));
    }

    #[test]
    fn like_contains() {
        let v = TypedValue::Text("alice".to_string());
        assert!(v.sql_like("%lic%"));
        assert!(!v.sql_like("%xyz%"));
    }

    #[test]
    fn like_exact() {
        let v = TypedValue::Text("alice".to_string());
        assert!(v.sql_like("alice"));
        assert!(!v.sql_like("bob"));
    }

    #[test]
    fn like_supports_single_character_wildcard() {
        let v = TypedValue::Text("alice".to_string());
        assert!(v.sql_like("a_i_e"));
        assert!(!v.sql_like("a__z_"));
    }

    #[test]
    fn like_supports_escaped_wildcards() {
        let v = TypedValue::Text("100% real".to_string());
        assert!(v.sql_like("100\\% real"));
        assert!(!v.sql_like("100% real_"));
    }

    #[test]
    fn ilike_is_case_insensitive() {
        let v = TypedValue::Text("Alice".to_string());
        assert!(v.sql_ilike("aLi%"));
        assert!(!v.sql_ilike("bob%"));
    }

    #[test]
    fn sql_lt_gt() {
        assert!(TypedValue::Int(1).sql_lt(&TypedValue::Int(2)));
        assert!(!TypedValue::Int(2).sql_lt(&TypedValue::Int(1)));
        assert!(TypedValue::Int(2).sql_gt(&TypedValue::Int(1)));
        assert!(!TypedValue::Null.sql_lt(&TypedValue::Int(1)));
    }

    #[test]
    fn display() {
        assert_eq!(format!("{}", TypedValue::Int(42)), "42");
        assert_eq!(format!("{}", TypedValue::Text("hi".into())), "hi");
        assert_eq!(format!("{}", TypedValue::Null), "NULL");
    }

    #[test]
    fn cast_to_supported_types() {
        assert_eq!(
            TypedValue::Text("42".to_string())
                .cast_to_data_type(&DataType::BigInt(None))
                .unwrap(),
            TypedValue::Int(42)
        );
        assert_eq!(
            TypedValue::Text("true".to_string())
                .cast_to_data_type(&DataType::Boolean)
                .unwrap(),
            TypedValue::Boolean(true)
        );
        assert_eq!(
            TypedValue::Int(7)
                .cast_to_data_type(&DataType::Varchar(None))
                .unwrap(),
            TypedValue::Text("7".to_string())
        );
        assert_eq!(
            TypedValue::Text("2024-01-15".to_string())
                .cast_to_data_type(&DataType::Date)
                .unwrap(),
            TypedValue::Date(NaiveDate::from_ymd_opt(2024, 1, 15).unwrap())
        );
    }

    #[test]
    fn cast_to_unsupported_type_errors() {
        assert!(TypedValue::supports_cast_type(&DataType::BigInt(None)));
        assert!(!TypedValue::supports_cast_type(&DataType::Uuid));
        assert!(TypedValue::Text("alice".to_string())
            .cast_to_data_type(&DataType::Uuid)
            .is_err());
    }
}
