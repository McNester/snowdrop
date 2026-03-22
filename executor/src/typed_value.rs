use std::cmp::Ordering;
use std::fmt;

/// Runtime representation of a column value with type information.
#[derive(Debug, Clone, PartialEq)]
pub enum TypedValue {
    Int(i64),
    Float(f64),
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
            "TINYINT" | "SMALLINT" | "INT" | "INTEGER" | "BIGINT" => {
                s.parse::<i64>().map(TypedValue::Int).unwrap_or(TypedValue::Text(s.into_owned()))
            }
            "FLOAT" | "DOUBLE" | "REAL" => {
                s.parse::<f64>().map(TypedValue::Float).unwrap_or(TypedValue::Text(s.into_owned()))
            }
            "BOOLEAN" | "BOOL" => match s.as_ref() {
                "true" | "1" | "t" => TypedValue::Boolean(true),
                "false" | "0" | "f" => TypedValue::Boolean(false),
                _ => TypedValue::Text(s.into_owned()),
            },
            // DECIMAL/NUMERIC stored as text, compared as float for numeric operations
            dt if dt.starts_with("DECIMAL") || dt.starts_with("NUMERIC") => {
                s.parse::<f64>().map(TypedValue::Float).unwrap_or(TypedValue::Text(s.into_owned()))
            }
            // VARCHAR, CHAR, TEXT, DATE, DATETIME, TIMESTAMP all stored as text
            _ => TypedValue::Text(s.into_owned()),
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
            TypedValue::Text(s) => s.parse::<f64>().ok(),
            _ => None,
        }
    }

    /// Compare two values. NULL is always "less than" non-NULL for ORDER BY purposes.
    pub fn cmp_order(&self, other: &TypedValue) -> Ordering {
        match (self, other) {
            (TypedValue::Null, TypedValue::Null) => Ordering::Equal,
            (TypedValue::Null, _) => Ordering::Less,
            (_, TypedValue::Null) => Ordering::Greater,
            (TypedValue::Int(a), TypedValue::Int(b)) => a.cmp(b),
            (TypedValue::Float(a), TypedValue::Float(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
            (TypedValue::Int(a), TypedValue::Float(b)) => {
                (*a as f64).partial_cmp(b).unwrap_or(Ordering::Equal)
            }
            (TypedValue::Float(a), TypedValue::Int(b)) => {
                a.partial_cmp(&(*b as f64)).unwrap_or(Ordering::Equal)
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
            (TypedValue::Int(a), TypedValue::Float(b)) => (*a as f64) == *b,
            (TypedValue::Float(a), TypedValue::Int(b)) => *a == (*b as f64),
            (TypedValue::Text(a), TypedValue::Text(b)) => a == b,
            (TypedValue::Boolean(a), TypedValue::Boolean(b)) => a == b,
            // Allow string-to-number comparison for WHERE col = '42'
            (TypedValue::Int(a), TypedValue::Text(b)) => b.parse::<i64>().map_or(false, |v| *a == v),
            (TypedValue::Text(a), TypedValue::Int(b)) => a.parse::<i64>().map_or(false, |v| v == *b),
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

    /// LIKE pattern matching. Supports prefix% and %suffix only in phase 1.
    pub fn sql_like(&self, pattern: &str) -> bool {
        let val = match self {
            TypedValue::Text(s) => s.as_str(),
            _ => return false,
        };

        if pattern.starts_with('%') && pattern.ends_with('%') && pattern.len() > 2 {
            let inner = &pattern[1..pattern.len() - 1];
            val.contains(inner)
        } else if pattern.starts_with('%') {
            val.ends_with(&pattern[1..])
        } else if pattern.ends_with('%') {
            val.starts_with(&pattern[..pattern.len() - 1])
        } else {
            val == pattern
        }
    }
}

impl fmt::Display for TypedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypedValue::Int(i) => write!(f, "{i}"),
            TypedValue::Float(v) => write!(f, "{v}"),
            TypedValue::Text(s) => write!(f, "{s}"),
            TypedValue::Boolean(b) => write!(f, "{b}"),
            TypedValue::Null => write!(f, "NULL"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_bytes_integer_types() {
        assert_eq!(TypedValue::from_bytes(b"42", "BIGINT"), TypedValue::Int(42));
        assert_eq!(TypedValue::from_bytes(b"-1", "INT"), TypedValue::Int(-1));
        assert_eq!(TypedValue::from_bytes(b"0", "SMALLINT"), TypedValue::Int(0));
    }

    #[test]
    fn from_bytes_float_types() {
        assert_eq!(TypedValue::from_bytes(b"3.14", "FLOAT"), TypedValue::Float(3.14));
        assert_eq!(TypedValue::from_bytes(b"2.0", "DOUBLE"), TypedValue::Float(2.0));
    }

    #[test]
    fn from_bytes_decimal() {
        assert_eq!(TypedValue::from_bytes(b"99.95", "DECIMAL(10,2)"), TypedValue::Float(99.95));
    }

    #[test]
    fn from_bytes_text_types() {
        assert_eq!(
            TypedValue::from_bytes(b"hello", "VARCHAR(255)"),
            TypedValue::Text("hello".to_string())
        );
        assert_eq!(
            TypedValue::from_bytes(b"2024-01-15", "DATE"),
            TypedValue::Text("2024-01-15".to_string())
        );
    }

    #[test]
    fn from_bytes_boolean() {
        assert_eq!(TypedValue::from_bytes(b"true", "BOOLEAN"), TypedValue::Boolean(true));
        assert_eq!(TypedValue::from_bytes(b"0", "BOOLEAN"), TypedValue::Boolean(false));
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
    fn cmp_order_nulls_sort_first() {
        assert_eq!(TypedValue::Null.cmp_order(&TypedValue::Int(1)), Ordering::Less);
        assert_eq!(TypedValue::Int(1).cmp_order(&TypedValue::Null), Ordering::Greater);
    }

    #[test]
    fn cmp_order_integers() {
        assert_eq!(TypedValue::Int(1).cmp_order(&TypedValue::Int(2)), Ordering::Less);
        assert_eq!(TypedValue::Int(5).cmp_order(&TypedValue::Int(5)), Ordering::Equal);
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
}
