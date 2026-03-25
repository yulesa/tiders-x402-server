//! PostgreSQL SQL query generator.
//!
//! Converts an `AnalyzedQuery` AST into a PostgreSQL-compatible SQL string.
//! Delegates standard expressions to `sql_shared::display_common_expr` and
//! handles Postgres-specific syntax: standard `EXTRACT`, `AT TIME ZONE`,
//! typed-string casts (`'value'::type`), and rejection of `TRY_CAST`/`SafeCast`.

use sqlparser::ast::{Expr, CastKind};
use anyhow::{anyhow, Result};
use crate::sqp_parser::AnalyzedQuery;
use crate::sql_shared::{format_value, display_common_expr};

/// Generates a PostgreSQL SQL query string from an analyzed query AST.
pub fn create_postgresql_query(ast: &AnalyzedQuery) -> Result<String> {
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
        query.push_str(&format!(" WHERE {}", pg_display_expr(selection)?));
    }

    // ORDER BY clause
    if let Some(order_by) = &ast.order_by {
        query.push_str(" ORDER BY ");
        let order_clauses = order_by.iter()
            .map(|order_by_expr| {
                let mut clause = pg_display_expr(&order_by_expr.expr)?;
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
            query.push_str(&format!(" LIMIT {}", pg_display_expr(limit)?));
        }
        if let Some(offset) = &limit_clause.offset {
            query.push_str(&format!(" OFFSET {}", pg_display_expr(offset)?));
        }
    }

    Ok(query)
}

/// Renders a single SQL expression into PostgreSQL-specific syntax.
///
/// Falls through to `display_common_expr` for standard SQL expressions and
/// handles Postgres-specific variants (Extract, AtTimeZone, TypedString, Cast).
fn pg_display_expr(expr: &Expr) -> Result<String> {
    // Try the shared handler first
    if let Some(result) = display_common_expr(expr, &pg_display_expr)? {
        return Ok(result);
    }

    // Postgres-specific expressions
    match expr {
        Expr::TypedString(ts) => {
            Ok(format!("{}::{}", format_value(&ts.value.value)?, ts.data_type))
        }

        Expr::Extract { field, syntax: _, expr } => {
            let expr_str = pg_display_expr(expr)?;
            Ok(format!("EXTRACT({} FROM {})", field, expr_str))
        }

        Expr::AtTimeZone { timestamp, time_zone } => {
            let timestamp_str = pg_display_expr(timestamp)?;
            let timezone_str = pg_display_expr(time_zone)?;
            Ok(format!("{} AT TIME ZONE {}", timestamp_str, timezone_str))
        }

        // Postgres does not support TRY_CAST or SafeCast
        Expr::Cast { kind, expr: _, data_type: _, format: _, array: _ } => {
            match kind {
                CastKind::TryCast => Err(anyhow!("TRY_CAST is not supported in PostgreSQL")),
                CastKind::SafeCast => Err(anyhow!("Safe cast is not supported in PostgreSQL")),
                _ => Err(anyhow!("PostgreSQL: Unexpected cast kind: {:?}", kind)),
            }
        }

        _ => Err(anyhow!("PostgreSQL: Unsupported expression type: {:?}", expr)),
    }
}
