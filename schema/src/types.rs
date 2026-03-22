use sqlparser::ast::DataType;
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
#[error("type {0} is not supported in this version")]
pub struct UnsupportedType(pub String);

const EXCLUDED_TYPES: &[&str] = &["BLOB", "BINARY", "VARBINARY", "JSON", "ENUM", "SET"];

impl ColumnType {
    /// Parses a sqlparser DataType AST node into our internal ColumnType.
    pub fn from_sql_type(dt: &DataType) -> Result<Self, UnsupportedType> {
        match dt {
            DataType::TinyInt(_) => Ok(Self::TinyInt),
            DataType::SmallInt(_) => Ok(Self::SmallInt),
            DataType::Int(_) | DataType::Integer(_) => Ok(Self::Int),
            DataType::BigInt(_) => Ok(Self::BigInt),
            DataType::Float(_) => Ok(Self::Float),
            DataType::Double | DataType::DoublePrecision => Ok(Self::Double),
            DataType::Decimal(info) | DataType::Numeric(info) => {
                let (p, s) = extract_precision_scale(info);
                if matches!(dt, DataType::Decimal(_)) {
                    Ok(Self::Decimal {
                        precision: p,
                        scale: s,
                    })
                } else {
                    Ok(Self::Numeric {
                        precision: p,
                        scale: s,
                    })
                }
            }
            DataType::Char(len) | DataType::Character(len) => {
                let n = extract_char_length(len);
                Ok(Self::Char(n))
            }
            DataType::Varchar(len) | DataType::CharVarying(len) | DataType::CharacterVarying(len) => {
                let n = extract_char_length(len);
                Ok(Self::Varchar(n))
            }
            DataType::Text => Ok(Self::Text),
            DataType::Date => Ok(Self::Date),
            DataType::Datetime(_) => Ok(Self::DateTime),
            DataType::Timestamp(_, _) => Ok(Self::Timestamp),
            DataType::Boolean | DataType::Bool => Ok(Self::Boolean),
            other => {
                let type_str = other.to_string().to_uppercase();
                for excluded in EXCLUDED_TYPES {
                    if type_str.contains(excluded) {
                        return Err(UnsupportedType(format!(
                            "{excluded} is not supported in this version"
                        )));
                    }
                }
                Err(UnsupportedType(type_str))
            }
        }
    }

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

fn extract_precision_scale(info: &sqlparser::ast::ExactNumberInfo) -> (u8, u8) {
    match info {
        sqlparser::ast::ExactNumberInfo::None => (10, 0),
        sqlparser::ast::ExactNumberInfo::Precision(p) => (*p as u8, 0),
        sqlparser::ast::ExactNumberInfo::PrecisionAndScale(p, s) => (*p as u8, *s as u8),
    }
}

fn extract_char_length(len: &Option<sqlparser::ast::CharacterLength>) -> u32 {
    match len {
        Some(sqlparser::ast::CharacterLength::IntegerLength { length, .. }) => *length as u32,
        Some(sqlparser::ast::CharacterLength::Max) => u32::MAX,
        None => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::dialect::PostgreSqlDialect;
    use sqlparser::parser::Parser;

    fn parse_type(sql: &str) -> DataType {
        let stmts = Parser::parse_sql(
            &PostgreSqlDialect {},
            &format!("CREATE TABLE t (c {sql})"),
        )
        .unwrap();
        match &stmts[0] {
            sqlparser::ast::Statement::CreateTable(ct) => {
                ct.columns[0].data_type.clone()
            }
            _ => panic!("expected CREATE TABLE"),
        }
    }

    #[test]
    fn parse_integer_types() {
        assert_eq!(ColumnType::from_sql_type(&parse_type("TINYINT")).unwrap(), ColumnType::TinyInt);
        assert_eq!(ColumnType::from_sql_type(&parse_type("SMALLINT")).unwrap(), ColumnType::SmallInt);
        assert_eq!(ColumnType::from_sql_type(&parse_type("INT")).unwrap(), ColumnType::Int);
        assert_eq!(ColumnType::from_sql_type(&parse_type("INTEGER")).unwrap(), ColumnType::Int);
        assert_eq!(ColumnType::from_sql_type(&parse_type("BIGINT")).unwrap(), ColumnType::BigInt);
    }

    #[test]
    fn parse_float_types() {
        assert_eq!(ColumnType::from_sql_type(&parse_type("FLOAT")).unwrap(), ColumnType::Float);
        assert_eq!(ColumnType::from_sql_type(&parse_type("DOUBLE PRECISION")).unwrap(), ColumnType::Double);
    }

    #[test]
    fn parse_decimal_types() {
        assert_eq!(
            ColumnType::from_sql_type(&parse_type("DECIMAL(10,2)")).unwrap(),
            ColumnType::Decimal { precision: 10, scale: 2 }
        );
        assert_eq!(
            ColumnType::from_sql_type(&parse_type("NUMERIC(5,3)")).unwrap(),
            ColumnType::Numeric { precision: 5, scale: 3 }
        );
    }

    #[test]
    fn parse_string_types() {
        assert_eq!(ColumnType::from_sql_type(&parse_type("CHAR(10)")).unwrap(), ColumnType::Char(10));
        assert_eq!(ColumnType::from_sql_type(&parse_type("VARCHAR(255)")).unwrap(), ColumnType::Varchar(255));
        assert_eq!(ColumnType::from_sql_type(&parse_type("TEXT")).unwrap(), ColumnType::Text);
    }

    #[test]
    fn parse_date_types() {
        assert_eq!(ColumnType::from_sql_type(&parse_type("DATE")).unwrap(), ColumnType::Date);
        assert_eq!(ColumnType::from_sql_type(&parse_type("TIMESTAMP")).unwrap(), ColumnType::Timestamp);
    }

    #[test]
    fn parse_boolean() {
        assert_eq!(ColumnType::from_sql_type(&parse_type("BOOLEAN")).unwrap(), ColumnType::Boolean);
    }

    #[test]
    fn reject_blob() {
        let result = ColumnType::from_sql_type(&parse_type("BYTEA"));
        assert!(result.is_err());
    }

    #[test]
    fn column_type_as_str() {
        assert_eq!(ColumnType::BigInt.as_str(), "BIGINT");
        assert_eq!(ColumnType::Varchar(255).as_str(), "VARCHAR(255)");
        assert_eq!(
            ColumnType::Decimal { precision: 10, scale: 2 }.as_str(),
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
