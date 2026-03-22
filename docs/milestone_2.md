# Milestone 2 -- DDL: Schema and Table Management

**Status:** Complete

## Deliverables

### SQL Statement Routing

- `sqlparser-rs` parses SQL to AST, statement dispatch routes to schema handlers
- Supports: CREATE/DROP DATABASE, CREATE/DROP TABLE, ALTER TABLE ADD COLUMN, SHOW DATABASES, SHOW TABLES, DESCRIBE, USE, INFORMATION_SCHEMA queries
- Unsupported types return clear errors (e.g. "type BYTEA is not supported in this version")
- Missing PRIMARY KEY returns "table must have exactly one PRIMARY KEY column"

### Schema Operations (Neo4j)

- `init_schema()`: creates uniqueness constraints and counter nodes on startup
- Counter-based ID generation: `(:Meta {key: 'db_counter'})` pattern for db_id, table_id, col_id
- CREATE DATABASE: Database node with uniqueness constraint
- CREATE TABLE: validates types, requires exactly one PK, creates Table + Column nodes with HAS_TABLE/HAS_COLUMN relationships
- DROP TABLE: cascades to Column nodes and IndexEntry nodes
- DROP DATABASE: cascades to all Tables and Columns
- ALTER TABLE ADD COLUMN: new Column node with fresh col_id
- DESCRIBE: returns Field, Type, Null, Key columns ordered by col_id

### Type System

- `ColumnType::from_sql_type()` parses sqlparser AST DataType to internal representation
- All Phase 1 types supported: TINYINT, SMALLINT, INT, BIGINT, FLOAT, DOUBLE, DECIMAL, NUMERIC, CHAR, VARCHAR, TEXT, DATE, DATETIME, TIMESTAMP, BOOLEAN
- Excluded types rejected with descriptive error

### INFORMATION_SCHEMA

- `INFORMATION_SCHEMA.tables`: returns (table_catalog, table_schema, table_name)
- `INFORMATION_SCHEMA.columns`: returns (table_catalog, table_schema, table_name, column_name, data_type, is_nullable)

### pgwire Integration

- Uses `DataRowEncoder` for proper pgwire row encoding
- `QueryResponse` with streamed rows via `futures::stream::iter`
- Error responses use pgwire's `ErrorInfo` with SQLSTATE codes

## Test Results

```
33 unit tests passing (keys: 11, wal: 13, schema/types: 9)
```

## Acceptance Criteria

| # | Criterion | Result |
|---|---|---|
| 1 | CREATE DATABASE succeeds, visible in SHOW DATABASES | PASS |
| 2 | CREATE TABLE with columns succeeds | PASS |
| 3 | DESCRIBE returns correct columns, types, nullable, PK | PASS |
| 4 | ALTER TABLE ADD COLUMN succeeds, DESCRIBE reflects it | PASS |
| 5 | DROP TABLE succeeds, SHOW TABLES empty | PASS |
| 6 | DROP DATABASE succeeds | PASS |
| 7 | CREATE TABLE with BYTEA returns unsupported type error | PASS |
| 8 | CREATE TABLE without PRIMARY KEY returns error | PASS |
| 9 | Restart orchestrator, DESCRIBE returns same results | PASS |

## How to Test

```bash
cargo test

docker compose up --build -d

psql -h localhost -U test -d test
> CREATE DATABASE mydb;
> CREATE TABLE mydb.users (id BIGINT PRIMARY KEY, name VARCHAR(255) NOT NULL, email VARCHAR(255));
> DESCRIBE mydb.users;
> ALTER TABLE mydb.users ADD COLUMN age INT;
> DROP TABLE mydb.users;
> DROP DATABASE mydb;

docker compose down
```
