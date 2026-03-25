//! ClickHouse SQL query generator.
//!
//! Converts an `AnalyzedQuery` AST into a ClickHouse-compatible SQL string.
//! Delegates standard expressions to `sql_shared::display_common_expr` and
//! handles ClickHouse-specific syntax: `toTimezone` for `AT TIME ZONE`,
//! `position(haystack, needle)` for `POSITION`, and rejection of
//! `SIMILAR TO`, `TRY_CAST`, `SafeCast`, and `OVERLAY`.

use sqlparser::ast::{Expr, CastKind};
use anyhow::{anyhow, Result};
use crate::sqp_parser::AnalyzedQuery;
use crate::sql_shared::{format_value, display_common_expr};

/// Generates a ClickHouse SQL query string from an analyzed query AST.
pub fn create_clickhouse_query(ast: &AnalyzedQuery) -> Result<String> {
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
        query.push_str(&format!(" WHERE {}", ch_display_expr(selection)?));
    }

    // ORDER BY clause
    if let Some(order_by) = &ast.order_by {
        query.push_str(" ORDER BY ");
        let order_clauses = order_by.iter()
            .map(|order_by_expr| {
                let mut clause = ch_display_expr(&order_by_expr.expr)?;
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
            query.push_str(&format!(" LIMIT {}", ch_display_expr(limit)?));
        }
        if let Some(offset) = &limit_clause.offset {
            query.push_str(&format!(" OFFSET {}", ch_display_expr(offset)?));
        }
    }

    Ok(query)
}

/// Renders a single SQL expression into ClickHouse-specific syntax.
///
/// Handles ClickHouse overrides first (SIMILAR TO, POSITION, OVERLAY, casts),
/// then falls through to `display_common_expr` for standard SQL expressions,
/// and finally handles remaining ClickHouse-specific variants.
fn ch_display_expr(expr: &Expr) -> Result<String> {
    // ClickHouse overrides that must be checked BEFORE the shared handler
    match expr {
        // ClickHouse does not support SIMILAR TO
        Expr::SimilarTo { .. } => {
            return Err(anyhow!("SIMILAR TO is not supported in ClickHouse"));
        }

        // ClickHouse: position(haystack, needle) instead of POSITION(needle IN haystack)
        Expr::Position { expr, r#in } => {
            let needle_str = ch_display_expr(expr)?;
            let haystack_str = ch_display_expr(r#in)?;
            return Ok(format!("position({}, {})", haystack_str, needle_str));
        }

        // OVERLAY is not natively supported in ClickHouse; synthesize with concat+substring
        Expr::Overlay { expr, overlay_what, overlay_from, overlay_for } => {
            let expr_str = ch_display_expr(expr)?;
            let what_str = ch_display_expr(overlay_what)?;
            let from_str = ch_display_expr(overlay_from)?;

            return if let Some(for_expr) = overlay_for {
                let for_str = ch_display_expr(for_expr)?;
                Ok(format!(
                    "concat(substring({expr}, 1, {from} - 1), {what}, substring({expr}, {from} + {for_len}))",
                    expr = expr_str, from = from_str, what = what_str, for_len = for_str
                ))
            } else {
                Ok(format!(
                    "concat(substring({expr}, 1, {from} - 1), {what}, substring({expr}, {from} + length({what})))",
                    expr = expr_str, from = from_str, what = what_str
                ))
            };
        }

        // DoubleColon cast → CAST(expr AS type) in ClickHouse
        Expr::Cast { kind: CastKind::DoubleColon, expr, data_type, .. } => {
            let expr_str = ch_display_expr(expr)?;
            return Ok(format!("CAST({} AS {})", expr_str, data_type));
        }

        _ => {}
    }

    // Try the shared handler for standard expressions
    if let Some(result) = display_common_expr(expr, &ch_display_expr)? {
        return Ok(result);
    }

    // ClickHouse-specific expressions not handled by shared code
    match expr {
        Expr::TypedString(ts) => {
            Ok(format!("CAST({} AS {})", format_value(&ts.value.value)?, ts.data_type))
        }

        Expr::Extract { field, syntax: _, expr } => {
            let expr_str = ch_display_expr(expr)?;
            Ok(format!("EXTRACT({} FROM {})", field, expr_str))
        }

        Expr::AtTimeZone { timestamp, time_zone } => {
            let timestamp_str = ch_display_expr(timestamp)?;
            let timezone_str = ch_display_expr(time_zone)?;
            Ok(format!("toTimezone({}, {})", timestamp_str, timezone_str))
        }

        // ClickHouse does not support TRY_CAST or SafeCast
        Expr::Cast { kind, .. } => {
            match kind {
                CastKind::TryCast => Err(anyhow!("TRY_CAST is not supported in ClickHouse")),
                CastKind::SafeCast => Err(anyhow!("Safe cast is not supported in ClickHouse")),
                _ => Err(anyhow!("ClickHouse: Unexpected cast kind: {:?}", kind)),
            }
        }

        _ => Err(anyhow!("ClickHouse: Unsupported expression type: {:?}", expr)),
    }
}
