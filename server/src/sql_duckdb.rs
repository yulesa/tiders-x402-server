use sqlparser::ast::{Expr, CastKind};
use anyhow::{anyhow, Result};
use crate::sqp_parser::AnalyzedQuery;
use crate::sql_shared::{format_value, display_common_expr};

pub fn create_duckdb_query(ast: &AnalyzedQuery) -> Result<String> {
    let mut query = String::new();

    // SELECT clause
    query.push_str("SELECT ");

    if ast.body.wildcard {
        query.push_str("*");
    } else {
        let projections: Vec<String> = ast.body.projection.iter()
            .map(|item| {
                if let Some(alias) = &item.alias {
                    format!("{} AS {}", item.ident, alias)
                } else {
                    item.ident.clone()
                }
            })
            .collect();
        query.push_str(&projections.join(", "));
    }

    // FROM clause
    query.push_str(&format!(" FROM {}", ast.body.from));

    // WHERE clause
    if let Some(selection) = &ast.body.selection {
        query.push_str(&format!(" WHERE {}", duckdb_display_expr(selection)?));
    }

    // ORDER BY clause
    if let Some(order_by) = &ast.order_by {
        query.push_str(" ORDER BY ");
        let order_clauses = order_by.iter()
            .map(|order_by_expr| {
                let mut clause = duckdb_display_expr(&order_by_expr.expr)?;
                if !order_by_expr.asc {
                    clause.push_str(" DESC");
                }
                if let Some(nulls_first) = order_by_expr.nulls_first {
                    if nulls_first {
                        clause.push_str(" NULLS FIRST");
                    } else {
                        clause.push_str(" NULLS LAST");
                    }
                }
                Ok(clause)
            })
            .collect::<Result<Vec<String>>>()?;
        query.push_str(&order_clauses.join(", "));
    }

    // LIMIT clause
    if let Some(limit_clause) = &ast.limit_clause {
        if let Some(limit) = &limit_clause.limit {
            query.push_str(&format!(" LIMIT {}", duckdb_display_expr(limit)?));
        }
        if let Some(offset) = &limit_clause.offset {
            query.push_str(&format!(" OFFSET {}", duckdb_display_expr(offset)?));
        }
    }

    Ok(query)
}

fn duckdb_display_expr(expr: &Expr) -> Result<String> {
    // Try the shared handler first
    if let Some(result) = display_common_expr(expr, &duckdb_display_expr)? {
        return Ok(result);
    }

    // DuckDB-specific expressions
    match expr {
        Expr::TypedString(ts) => {
            Ok(format!("{}::{}", format_value(&ts.value.value)?, ts.data_type))
        }

        Expr::Extract { field, syntax: _, expr } => {
            let expr_str = duckdb_display_expr(expr)?;
            Ok(format!("date_part('{}', {})", field, expr_str))
        }

        Expr::AtTimeZone { timestamp, time_zone } => {
            let timestamp_str = duckdb_display_expr(timestamp)?;
            let timezone_str = duckdb_display_expr(time_zone)?;
            Ok(format!("{} AT TIME ZONE {}", timestamp_str, timezone_str))
        }

        // DuckDB supports TRY_CAST but not SafeCast
        Expr::Cast { kind, expr, data_type, format: _, array: _ } => {
            let expr_str = duckdb_display_expr(expr)?;
            let data_type_str = format!("{}", data_type);
            match kind {
                CastKind::TryCast => Ok(format!("TRY_CAST({} AS {})", expr_str, data_type_str)),
                CastKind::SafeCast => Err(anyhow!("Safe cast is not supported in DuckDB")),
                _ => Err(anyhow!("DuckDB: Unexpected cast kind: {:?}", kind)),
            }
        }

        _ => Err(anyhow!("DuckDB: Unsupported expression type: {:?}", expr)),
    }
}

#[cfg(test)]
mod tests {
    use sqlparser::dialect::DuckDbDialect;

    use super::*;
    use crate::sqp_parser::analyze_query;
    use sqlparser::parser::Parser;

    #[test]
    #[ignore]

    pub fn run_tests() -> Result<()> {
        println!("=== Testing SQL Parser and DuckDB Reader ===\n");

        // Test 1: Simple SELECT with wildcard
        test_query(
            "SELECT * FROM users",
            "Test 1: Simple SELECT with wildcard"
        )?;

        // Test 2: SELECT quoted style
        test_query(
            "SELECT id, name as 'Name', email AS \"Email\" FROM users WHERE name = 'John' AND email = \"john@example.com\"",
            "Test 2: SELECT with specific columns"
        )?;

        // Test 3: SELECT with all Value variants
        test_query(
            "SELECT *
                FROM users
                WHERE 1 = TRUE
                AND $$dollar$$ = 'dollar'
                AND \"double_quoted\" = 'double_quoted'
                AND NULL = NULL
                AND $1 = ?1
                AND \"\"\"triple_double\"\"\" = 'triple_double'
                AND '''triple_single_quoted''' = 'triple_single_quoted'",
            "Test 3: SELECT with all Value variants"
        )?;

        // Test 4a: TypedString
        test_query(
            "SELECT * FROM user WHERE
                created_at = DATE '2025-01-01'
                AND c1 = CHAR 'A'
                AND c2 = VARCHAR 'abc'
                AND c3 = NVARCHAR 'abc'
                AND c4 = UUID '123e4567-e89b-12d3-a456-426614174000'
                AND c5 = CLOB 'clobtext'
                AND c6 = BINARY '010101'
                AND c7 = VARBINARY '010101'
                AND c8 = BLOB 'blobdata'
                AND c9 = DECIMAL '123.45'
                AND c10 = FLOAT '1.23'",
                "Test 4a: TypedString with all DataType variants"
            )?;

            // Test 4b: TypedString
            test_query(
                "SELECT * FROM user WHERE
                c13 = INT '3'
                AND c14 = BIGINT '4'
                AND c15 = REAL '5.6'
                AND c16 = DOUBLE '7.8'
                AND c17 = BOOLEAN 'TRUE'
                AND c18 = TIME '12:34:56'
                AND c19 = TIMESTAMP '2025-01-01 12:34:56'
                AND c20 = INTERVAL 1 day",
                "Test 4b: TypedString with all DataType variants"
            )?;

        // Test 5: boolean expressions
        test_query(
            "SELECT * FROM user WHERE
                active IS TRUE
                OR active IS NOT TRUE
                OR active IS FALSE
                OR active IS NOT FALSE
                OR active IS NULL
                OR active IS NOT NULL",
            "Test 5: boolean expressions"
        )?;

        // Test 6: IN expressions
        test_query(
            "SELECT * FROM users WHERE age IN (18, 21, 25)",
            "Test 6: IN expression"
        )?;
        test_query(
            "SELECT * FROM users WHERE age NOT IN (18, 21, 25)",
            "Test 6b: NOT IN expression"
        )?;

        // Test 7: BETWEEN expressions
        test_query(
            "SELECT * FROM users WHERE age BETWEEN 18 AND 25 AND age NOT BETWEEN 18 AND 25",
            "Test 7: BETWEEN expression"
        )?;

        // Test 8: Binary operations
        test_query(
            "SELECT * FROM users WHERE age != 25",
            "Test 8: BinaryOp equals (!=)"
        )?;

        // Test 9: LIKE expressions
        test_query(
            "SELECT * FROM users WHERE name LIKE 'John%' AND name NOT LIKE 'Joe%'",
            "Test 9: LIKE expression"
        )?;

        // Test 10: ILIKE expressions
        test_query(
            "SELECT * FROM users WHERE name ILIKE 'john%' AND name NOT ILIKE 'joe%'",
            "Test 10: ILIKE expression"
        )?;

        // Test 11: SIMILAR TO expressions
        test_query(
            "SELECT * FROM users WHERE name SIMILAR TO 'J%' AND name NOT SIMILAR TO 'J%'",
            "Test 11: SIMILAR TO expression"
        )?;

        // Test 12: CAST and CONVERT expressions
        test_query(
            "SELECT * FROM users WHERE CAST(age AS VARCHAR) = '25'",
            "Test 12: CAST expression"
        )?;
        test_query(
            "SELECT * FROM users WHERE TRY_CAST(age AS VARCHAR) = '25'",
            "Test 12b: TRY_CAST expression"
        )?;
        test_query(
            "SELECT * FROM users WHERE age::VARCHAR = '25'",
            "Test 12c: DoubleColon CAST expression"
        )?;

        // Test 13: Time functions
        test_query(
            "SELECT * FROM users WHERE created_at AT TIME ZONE 'UTC' = '2025-01-01 00:00:00+00'",
            "Test 13: AT TIME ZONE expression"
        )?;
        test_query(
            "SELECT * FROM users WHERE EXTRACT(YEAR FROM created_at) = 2025",
            "Test 13b: EXTRACT expression"
        )?;

        // Test 14: String functions
        test_query(
            "SELECT * FROM users WHERE POSITION('a' IN name) = 1",
            "Test 14: POSITION expression"
        )?;
        test_query(
            "SELECT * FROM users WHERE SUBSTRING(name FROM 1 FOR 3) = 'Joh'",
            "Test 14b: SUBSTRING expression"
        )?;
        test_query(
            "SELECT * FROM users WHERE TRIM(name) = 'John'",
            "Test 14c: TRIM expression"
        )?;
        test_query(
            "SELECT * FROM users WHERE OVERLAY(name PLACING 'X' FROM 2 FOR 1) = 'JXhn'",
            "Test 14f: OVERLAY expression"
        )?;

        // Test 15: Tuple expressions
        test_query(
            "SELECT * FROM users WHERE (age, status) = (25, 'active')",
            "Test 15: Tuple expression"
        )?;

        // Test 16: Array expressions
        test_query(
            "SELECT * FROM users WHERE ARRAY[1, 2, 3] = ARRAY[1, 2, 3]",
            "Test 16: Array expression"
        )?;

        // Test 17: Interval expressions
        test_query(
            "SELECT * FROM users WHERE \"interval\"= INTERVAL 1 day",
            "Test 17: Interval expression"
        )?;

        println!("=== All tests completed successfully! ===");
        Ok(())
    }

    fn test_query(sql: &str, test_name: &str) -> Result<()> {
        println!("{}", test_name);
        println!("SQL: {}", sql);

        // Parse the SQL
        let analyzed_query = analyze_query(sql)?;
        let duckdb_sql = create_duckdb_query(&analyzed_query)?;
        println!("DuckDB SQL: {}", duckdb_sql);
        let dialect = DuckDbDialect;
        match Parser::parse_sql(&dialect, sql) {
            Ok(_) => {
                println!("✓ Test passed\n");
            }
            Err(e) => {
                println!("Error parsing SQL: {}", e);
                println!("SQL: {}", sql);
                println!("DuckDB SQL: {}", duckdb_sql);
                println!("Parsed SQL: {:#?}", analyzed_query);
            }
        }

        Ok(())
    }

    #[test]
    #[ignore]
    // Test error cases
    pub fn run_error_tests() -> Result<()> {
        println!("=== Testing Error Cases ===\n");

        // Test unsupported features
        let error_cases = vec![
            ("SELECT * FROM users JOIN orders ON users.id = orders.user_id", "JOIN not supported"),
            ("SELECT * FROM users GROUP BY status", "GROUP BY not supported"),
            ("SELECT * FROM users HAVING COUNT(*) > 10", "HAVING not supported"),
            ("SELECT * FROM users WHERE EXISTS (SELECT 1 FROM orders WHERE orders.user_id = users.id)", "EXISTS not supported"),
            ("SELECT * FROM users WHERE age IN (SELECT age FROM seniors)", "IN subquery not supported"),
            ("SELECT COUNT(*) FROM users", "Aggregate functions not supported"),
            ("SELECT * FROM users WHERE age > 18 UNION SELECT * FROM admins", "UNION not supported"),
        ];

        for (sql, description) in error_cases {
            println!("Testing: {}", description);
            println!("SQL: {}", sql);

            match analyze_query(sql) {
                Ok(_) => {
                    println!("❌ Expected error but got success\n");
                    return Err(anyhow!("Expected error but got success"));
                }
                Err(e) => {
                    println!("✓ Expected error: {}\n", e);
                }
            }
        }

        println!("=== Error tests completed! ===");
        Ok(())
    }
}
