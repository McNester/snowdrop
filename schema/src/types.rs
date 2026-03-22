use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnType {
    TinyInt,
    SmallInt,
    Int,
    BigInt,
    Float,
    Double,
    Decimal { precision: u8, scale: u8 },
    Numeric { precision: u8, scale: u8 },
    Char(u32),
    Varchar(u32),
    Text,
    Date,
    DateTime,
    Timestamp,
    Boolean,
}

#[derive(Debug, Error)]
#[error("unsupported type: {0}. Supported types: TINYINT, SMALLINT, INT, BIGINT, FLOAT, DOUBLE, DECIMAL, NUMERIC, CHAR, VARCHAR, TEXT, DATE, DATETIME, TIMESTAMP, BOOLEAN")]
pub struct UnsupportedType(pub String);

impl ColumnType {
    pub fn as_str(&self) -> String {
        match self {
            Self::TinyInt => "TINYINT".to_string(),
            Self::SmallInt => "SMALLINT".to_string(),
            Self::Int => "INT".to_string(),
            Self::BigInt => "BIGINT".to_string(),
            Self::Float => "FLOAT".to_string(),
            Self::Double => "DOUBLE".to_string(),
            Self::Decimal { precision, scale } => format!("DECIMAL({precision},{scale})"),
            Self::Numeric { precision, scale } => format!("NUMERIC({precision},{scale})"),
            Self::Char(n) => format!("CHAR({n})"),
            Self::Varchar(n) => format!("VARCHAR({n})"),
            Self::Text => "TEXT".to_string(),
            Self::Date => "DATE".to_string(),
            Self::DateTime => "DATETIME".to_string(),
            Self::Timestamp => "TIMESTAMP".to_string(),
            Self::Boolean => "BOOLEAN".to_string(),
        }
    }

    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            Self::TinyInt
                | Self::SmallInt
                | Self::Int
                | Self::BigInt
                | Self::Float
                | Self::Double
                | Self::Decimal { .. }
                | Self::Numeric { .. }
        )
    }

    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            Self::TinyInt | Self::SmallInt | Self::Int | Self::BigInt
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_type_as_str() {
        assert_eq!(ColumnType::BigInt.as_str(), "BIGINT");
        assert_eq!(ColumnType::Varchar(255).as_str(), "VARCHAR(255)");
        assert_eq!(
            ColumnType::Decimal {
                precision: 10,
                scale: 2
            }
            .as_str(),
            "DECIMAL(10,2)"
        );
    }

    #[test]
    fn numeric_classification() {
        assert!(ColumnType::Int.is_numeric());
        assert!(ColumnType::Float.is_numeric());
        assert!(!ColumnType::Text.is_numeric());
        assert!(!ColumnType::Boolean.is_numeric());

        assert!(ColumnType::BigInt.is_integer());
        assert!(!ColumnType::Float.is_integer());
    }
}
