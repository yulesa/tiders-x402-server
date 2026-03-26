//! PostgreSQL SQL query generator.
//!
//! Converts an `AnalyzedQuery` AST into a PostgreSQL-compatible SQL string.
//! Delegates standard expressions to `sql_shared::display_common_expr` and
//! handles Postgres-specific syntax: standard `EXTRACT`, `AT TIME ZONE`,
//! typed-string casts (`'value'::type`), and rejection of `TRY_CAST`/`SafeCast`.

use crate::sql_shared::{create_query, display_common_expr, format_value};
use crate::sqp_parser::AnalyzedQuery;
use anyhow::{Result, anyhow};
use sqlparser::ast::{CastKind, Expr};

/// Generates a PostgreSQL SQL query string from an analyzed query AST.
pub fn create_postgresql_query(ast: &AnalyzedQuery) -> Result<String> {
    create_query(ast, &pg_display_expr)
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
        Expr::TypedString(ts) => Ok(format!(
            "{}::{}",
            format_value(&ts.value.value)?,
            ts.data_type
        )),

        Expr::Extract {
            field,
            syntax: _,
            expr,
        } => {
            let expr_str = pg_display_expr(expr)?;
            Ok(format!("EXTRACT({} FROM {})", field, expr_str))
        }

        Expr::AtTimeZone {
            timestamp,
            time_zone,
        } => {
            let timestamp_str = pg_display_expr(timestamp)?;
            let timezone_str = pg_display_expr(time_zone)?;
            Ok(format!("{} AT TIME ZONE {}", timestamp_str, timezone_str))
        }

        // Postgres does not support TRY_CAST or SafeCast
        Expr::Cast {
            kind,
            expr: _,
            data_type: _,
            format: _,
            array: _,
        } => match kind {
            CastKind::TryCast => Err(anyhow!("TRY_CAST is not supported in PostgreSQL")),
            CastKind::SafeCast => Err(anyhow!("Safe cast is not supported in PostgreSQL")),
            _ => Err(anyhow!("PostgreSQL: Unexpected cast kind: {:?}", kind)),
        },

        _ => Err(anyhow!(
            "PostgreSQL: Unsupported expression type: {:?}",
            expr
        )),
    }
}
