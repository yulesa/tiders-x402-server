# SQL Generators

The SQL generator layer converts an `AnalyzedQuery` (from the [SQL Parser](./sql-parser.md)) into a database-specific SQL string. It is split into a shared module that handles standard SQL and per-backend modules that handle dialect differences.

## Shared

The shared module (`server/src/sql_shared.rs`) contains logic that is identical across all backends:

- **`create_query`** — assembles the final SQL string from the `AnalyzedQuery` AST: `SELECT`, `FROM`, `WHERE`, `ORDER BY`, `LIMIT`, and `OFFSET` clauses. It accepts a `display_expr` callback so each backend can plug in its own expression renderer.
- **`display_common_expr`** — renders standard SQL expressions that work the same everywhere: identifiers, literals, boolean predicates (`IS TRUE`, `IS NULL`, etc.), `IN`, `BETWEEN`, binary operators, `LIKE`/`ILIKE`/`SIMILAR TO`, `CAST`, `::`, math functions (`CEIL`, `FLOOR`), string functions (`POSITION`, `SUBSTRING`, `TRIM`, `OVERLAY`), nested/tuple/array/interval expressions. Returns `None` for dialect-specific expressions (`EXTRACT`, `AT TIME ZONE`, `TypedString`, `TRY_CAST`, `SafeCast`), letting each backend handle those.
- **`format_value`** — formats SQL value literals (strings, numbers, booleans, NULL, placeholders) with proper escaping.

## DuckDB

The DuckDB generator (`server/src/sql_duckdb.rs`) calls `display_common_expr` first and only handles what falls through:

- **`EXTRACT`** → `date_part('field', expr)` (DuckDB's preferred syntax).
- **`AT TIME ZONE`** → standard `expr AT TIME ZONE 'tz'`.
- **`TypedString`** → `'value'::type` (double-colon cast).
- **`TRY_CAST`** → supported natively.
- **`SafeCast`** → rejected (not available in DuckDB).

This file also contains the test suite that exercises all expression types through the parse → analyze → generate pipeline.

## PostgreSQL 

The PostgreSQL generator (`server/src/sql_postgresql.rs`) follows the same pattern — shared handler first, then Postgres-specific overrides:

- **`EXTRACT`** → standard `EXTRACT(field FROM expr)`.
- **`AT TIME ZONE`** → standard `expr AT TIME ZONE 'tz'`.
- **`TypedString`** → `'value'::type` (same as DuckDB).
- **`TRY_CAST`** and **`SafeCast`** → both rejected (not available in PostgreSQL).

## ClickHouse

The ClickHouse generator (`server/src/sql_clickhouse.rs`) has the most overrides because ClickHouse's SQL dialect diverges further from standard SQL. Some overrides are checked *before* calling the shared handler to intercept expressions that would otherwise be handled differently:

- **`SIMILAR TO`** → rejected (not supported).
- **`POSITION`** → rewritten to `position(haystack, needle)` (ClickHouse uses reversed argument order).
- **`OVERLAY`** → synthesized as `concat(substring(…), replacement, substring(…))` since ClickHouse lacks native `OVERLAY`.
- **`::`** (double-colon cast) → rewritten as `CAST(expr AS type)`.
- **`EXTRACT`** → standard `EXTRACT(field FROM expr)`.
- **`AT TIME ZONE`** → `toTimezone(expr, 'tz')`.
- **`TypedString`** → `CAST('value' AS type)`.
- **`TRY_CAST`** and **`SafeCast`** → both rejected.

## Adding a New Backend

To support a new database dialect, create a new `sql_<backend>.rs` file that:

1. Calls `create_query` with a backend-specific `display_expr` callback.
2. In that callback, tries `display_common_expr` first.
3. Handles any dialect-specific expressions that returned `None` from the shared handler, or overrides expressions before calling it when the backend needs different behavior.
