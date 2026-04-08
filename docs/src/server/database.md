# Database

The database layer is split into a shared trait and per-backend implementations. This lets the rest of the server (query handler, root handler) work with any supported database without knowing which one is running.

## Trait

Define in `database.rs`.

The `Database` trait (`server/src/database.rs`) defines the async interface that every backend must implement:

- **`execute_query`** — runs a SQL query and returns results as Arrow `RecordBatch`es.
- **`execute_row_count_query`** — runs a `COUNT(*)` query and returns the count as a single number. Used during the estimation step to determine the price before payment.
- **`get_table_schema`** — returns the Arrow schema of a table. Used by the root handler to advertise available tables.
- **`create_sql_query`** — converts an `AnalyzedQuery` AST (from the SQL parser) into a backend-specific SQL string. Each backend delegates to its own SQL generator (see [SQL Generators](./sql-generators.md)).

This file also contains `serialize_batches_to_arrow_ipc`, a backend-agnostic helper that converts Arrow record batches into the Arrow IPC streaming format — the binary format sent back to clients in successful responses.

## DuckDB

The DuckDB backend (`server/src/database_duckdb.rs`) wraps a `duckdb::Connection` behind `Arc<Mutex<…>>`. Since the DuckDB crate is synchronous, all operations use `tokio::task::spawn_blocking` to avoid blocking the async runtime.

Construction:
- **`DuckDbDatabase::new`** — from a pre-configured `Connection`.
- **`DuckDbDatabase::from_path`** — opens a database file at the given path.

Schema introspection uses `SELECT * FROM table LIMIT 0` to get the Arrow schema directly from DuckDB's native Arrow support.

## PostgreSQL

The PostgreSQL backend (`server/src/database_postgresql.rs`) uses `deadpool-postgres` for async connection pooling and `tokio-postgres` for query execution. Since Postgres does not return Arrow natively, this backend converts `tokio-postgres` rows into Arrow `RecordBatch`es column-by-column.

Construction:
- **`PostgresqlDatabase::from_connection_string`** — parses a connection string, builds a pool (default 16 connections), and verifies connectivity.
- **`PostgresqlDatabase::from_params`** — accepts individual parameters (host, port, user, password, dbname) with full control over pool settings (timeouts, recycling method, max size).
- **`PostgresqlDatabase::from_pool`** — from a user-managed `deadpool_postgres::Pool`.

The file includes a `pg_type_to_arrow` mapping that covers booleans, integers, floats, strings, dates, timestamps, decimals (with precision/scale from the type modifier), UUIDs, intervals, arrays, and JSON. Custom `FromSql` wrappers (`PgDate`, `PgTimestamp`, `PgNumeric`, `PgUuid`, `PgTime`, `PgInterval`) handle binary decoding of types that `tokio-postgres` does not convert directly.

## ClickHouse

The ClickHouse backend (`server/src/database_clickhouse.rs`) uses the `clickhouse` crate, which is natively async. Queries request results in `FORMAT ArrowStream`, so the response arrives as Arrow IPC bytes that are decoded directly into `RecordBatch`es — no intermediate JSON step.

Construction:
- **`ClickHouseDatabase::from_params`** — accepts URL plus optional user, password, database, access token, compression mode (`none` or `lz4`), custom settings, and HTTP headers.
- **`ClickHouseDatabase::from_client`** — from a user-managed `clickhouse::Client`.

Schema introspection uses `DESCRIBE TABLE` and maps ClickHouse types to Arrow via `ch_type_to_arrow`, handling `Nullable(…)` and `LowCardinality(…)` wrappers, decimal variants, enums, and date/time types.

## Adding a New Backend

To support a new database, create a `database_<backend>.rs` file that implements the four methods on the `Database` trait: executing queries, counting rows, retrieving table schemas, and generating backend-specific SQL. You will also need a corresponding SQL generator — see [SQL Generators](./sql-generators.md) for that side.

Key design decisions to consider:

- **Async strategy** — if the database driver is synchronous, wrap calls in `spawn_blocking` (as DuckDB does). Natively async drivers (ClickHouse, PostgreSQL) can be used directly.
- **Arrow conversion** — some drivers return Arrow natively (DuckDB) or can be asked to (ClickHouse with `FORMAT ArrowStream`). Others require manual row-to-Arrow conversion (PostgreSQL).
- **Schema introspection** — choose between `SELECT * FROM table LIMIT 0` or a `DESCRIBE TABLE` equivalent, depending on what the backend supports.

## Why This Structure

Separating the trait from the implementations keeps each backend self-contained. Adding a new database means writing a new file that implements `Database` — the query handler, root handler, and the rest of the server do not need to change.
