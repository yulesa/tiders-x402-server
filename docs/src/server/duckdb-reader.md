# DuckDB Reader

The DuckDB reader (`server/src/duckdb_reader.rs`) converts an `AnalyzedQuery` (from the SQL parser) into a DuckDB-compatible SQL string and handles query execution.

## Query Generation

`create_duckdb_query` takes an `AnalyzedQuery` and produces a SQL string:

```rust
pub fn create_duckdb_query(ast: AnalyzedQuery) -> Result<String>
```

It reconstructs the query by appending:
1. `SELECT` -- wildcard or comma-separated column list (with aliases)
2. `FROM` -- table name
3. `WHERE` -- expression tree converted via `duckdb_display_expr`
4. `ORDER BY` -- with ASC/DESC and NULLS FIRST/LAST
5. `LIMIT` / `OFFSET`

## Expression Rendering

The `duckdb_display_expr` function converts `sqlparser::ast::Expr` nodes to DuckDB SQL strings. It handles:

- **Identifiers and values** -- column names, string/number/boolean literals
- **Boolean predicates** -- `IS TRUE`, `IS NULL`, etc.
- **Comparisons and logic** -- binary operators (`=`, `AND`, `OR`, etc.)
- **Pattern matching** -- `LIKE`, `ILIKE`, `SIMILAR TO` with optional `ESCAPE`
- **Range checks** -- `BETWEEN`, `IN (...)`
- **Type casting** -- `CAST`, `TRY_CAST`, `::` (double colon)
- **Functions** -- `CEIL`, `FLOOR`, `POSITION`, `SUBSTRING`, `TRIM`, `OVERLAY`, `EXTRACT`
- **Time zones** -- `AT TIME ZONE`
- **Complex types** -- nested expressions, tuples, arrays, intervals

### Value Formatting

String values are properly escaped for DuckDB:
- Single-quoted strings: `'` is doubled to `''`
- Double-quoted strings: `"` is doubled to `""`
- NULL, boolean, number, and placeholder values are passed through

## Schema Introspection

```rust
pub fn get_duckdb_table_schema(db: &Connection, table_name: &str) -> Result<Schema>
```

Retrieves the Arrow schema for a table by executing `SELECT * FROM table LIMIT 0`. This schema is stored in `TablePaymentOffers` and returned to clients in the root endpoint.

## Execution

Query execution happens in `database.rs`:

- `execute_query` -- Runs the SQL and collects Arrow `RecordBatch` results
- `execute_row_count_query` -- Runs a `COUNT(*)` query and returns the count as `usize`
- `serialize_batches_to_arrow_ipc` -- Converts record batches to Arrow IPC streaming format
