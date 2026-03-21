# SQL Parser

The SQL parser (`server/src/sqp_parser.rs`) implements a restricted SQL dialect called "Simplified SQL" that prevents expensive or dangerous operations. It uses the `sqlparser` crate with the ANSI dialect.

## Supported SQL Features

### SELECT Clause
- Wildcard: `SELECT *`
- Named columns: `SELECT col1, col2`
- Aliases: `SELECT col1 AS alias`
- No expressions, functions, or computed columns

### FROM Clause
- Single table only: `FROM my_table`
- No table aliases
- No JOINs

### WHERE Clause
- Comparison operators: `=`, `!=`, `<`, `>`, `<=`, `>=`
- Boolean: `IS TRUE`, `IS FALSE`, `IS NULL`, `IS NOT NULL`
- Range: `BETWEEN ... AND ...`, `NOT BETWEEN`
- Set membership: `IN (...)`, `NOT IN (...)`
- Pattern matching: `LIKE`, `ILIKE`, `SIMILAR TO`
- Logical operators: `AND`, `OR`, `NOT`
- Nested expressions with parentheses
- Type casting: `CAST(... AS ...)`, `TRY_CAST`, `::`
- String functions: `SUBSTRING`, `TRIM`, `OVERLAY`, `POSITION`
- Math functions: `CEIL`, `FLOOR`
- Time functions: `EXTRACT`, `AT TIME ZONE`
- Literals: strings, numbers, booleans, NULL, arrays, intervals

### ORDER BY
- Column references
- `ASC` / `DESC`
- `NULLS FIRST` / `NULLS LAST`

### LIMIT / OFFSET
- `LIMIT n`
- `OFFSET n`

## Explicitly Unsupported

The parser rejects queries containing:

| Feature | Reason |
|---------|--------|
| JOINs | Prevents cross-table operations |
| GROUP BY / HAVING | No aggregation |
| Subqueries | No nested queries |
| CTEs (`WITH`) | No common table expressions |
| UNION / INTERSECT / EXCEPT | No set operations |
| Window functions | No analytical functions |
| Aggregate functions (`COUNT`, `SUM`, etc.) | No aggregation in SELECT |
| DISTINCT | No deduplication |
| Qualified wildcards (`alias.*`) | No table-qualified wildcards |
| `LIMIT BY` | MySQL-specific syntax |
| `ORDER BY ALL` | Not supported |

## Analyzed Query Structure

The parser produces an `AnalyzedQuery` struct:

```rust
pub struct AnalyzedQuery {
    pub body: AnalyzedSelect,          // SELECT/FROM/WHERE
    pub limit_clause: Option<AnalyzedLimitClause>,
    pub order_by: Option<Vec<AnalyzedOrderByExpr>>,
}

pub struct AnalyzedSelect {
    pub projection: Vec<AnalyzedSelectItem>,  // Column list
    pub wildcard: bool,                        // Was * used?
    pub from: String,                          // Table name
    pub selection: Option<Expr>,               // WHERE clause AST
}
```

The WHERE clause retains the `sqlparser` `Expr` AST nodes, which are validated to contain only supported expression types.

## Row Count Estimation

The parser provides `create_estimate_rows_query` which wraps any valid query in a `SELECT COUNT(*)`:

```rust
pub fn create_estimate_rows_query(duckdb_sql: &str) -> String {
    format!("SELECT COUNT(*) as num_rows FROM ({})", duckdb_sql)
}
```

This is used in Phase 1 to estimate pricing without executing the full query.
