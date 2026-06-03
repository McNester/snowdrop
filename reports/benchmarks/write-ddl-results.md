# Snowdrop Write and DDL Benchmark

Generated at: `2026-05-23T09:47:28.769723981+00:00`

Benchmark database: `__snowdrop_write_ddl_bench`

| Measurement | Detail | Rows | Latency | Throughput |
|---|---|---:|---:|---:|
| `create_database` | CREATE DATABASE for an empty benchmark database | - | 95.23ms | - |
| `create_table` | CREATE TABLE with PK, defaults, numeric/text/boolean/date columns | - | 237.97ms | - |
| `alter_table_add_column` | ALTER TABLE ADD COLUMN with a DEFAULT | - | 186.65ms | - |
| `single_row_insert` | One application-style INSERT statement | 1 | 59.43ms | 16.8 rows/sec |
| `row_by_row_bulk_insert` | Current migration-style row-by-row insert loop | 100 | 6972.81ms | 14.3 rows/sec |
| `multi_values_insert` | One INSERT statement with many VALUES rows; this exercises cached insert planning | 100 | 147.02ms | 680.2 rows/sec |

## How To Use This

- Use this as the baseline before changing schema-cache, Neo4j batching, or migration bulk-load behavior.
- The first target is not absolute parity with PostgreSQL; it is measurable improvement against this Snowdrop baseline.
