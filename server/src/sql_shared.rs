use sqlparser::ast::{Expr, Value, CastKind, TrimWhereField};
use anyhow::{anyhow, Result};

/// Format a SQL value literal into its string representation.
///
/// Shared across all database backends.
pub fn format_value(value: &Value) -> Result<String> {
    match value {
        Value::Boolean(b) => Ok(b.to_string().to_uppercase()),
        Value::DollarQuotedString(s) => Ok(format!("${}$", s)),
        Value::DoubleQuotedString(s) => Ok(format!("\"{}\"", s.replace("\"", "\"\""))),
        Value::Null => Ok("NULL".to_string()),
        Value::Number(n, _) => Ok(n.clone()),
        Value::Placeholder(p) => Ok(format!("${}", p)),
        Value::SingleQuotedString(s) => Ok(format!("'{}'", s.replace("'", "''"))),
        Value::TripleDoubleQuotedString(s) => Ok(format!("\"\"\"{}\"\"\"", s.replace("\"", "\"\""))),
        Value::TripleSingleQuotedString(s) => Ok(format!("'''{}'''", s.replace("'", "''"))),
        _ => Err(anyhow!("Unsupported value type: {:?}", value)),
    }
}

/// Handle standard SQL expressions shared across all database backends.
///
/// Returns `Ok(Some(string))` for expressions that are standard SQL and can be
/// handled identically across databases. Returns `Ok(None)` for dialect-specific
/// expressions (Extract, AtTimeZone, TypedString, TryCast/SafeCast), letting each
/// database backend handle those in its own `display_expr` function.
///
/// The `display_expr` parameter is a callback to the database-specific expression
/// renderer, used for recursive sub-expression rendering.
pub fn display_common_expr<F>(expr: &Expr, display_expr: &F) -> Result<Option<String>>
where
    F: Fn(&Expr) -> Result<String>,
{
    match expr {
        // Simple expressions
        Expr::Identifier(ident) => Ok(Some(ident.value.clone())),
        Expr::Value(value_with_span) => Ok(Some(format_value(&value_with_span.value)?)),

        // Boolean predicates
        Expr::IsFalse(expr) => Ok(Some(format!("{} IS FALSE", display_expr(expr)?))),
        Expr::IsNotFalse(expr) => Ok(Some(format!("{} IS NOT FALSE", display_expr(expr)?))),
        Expr::IsTrue(expr) => Ok(Some(format!("{} IS TRUE", display_expr(expr)?))),
        Expr::IsNotTrue(expr) => Ok(Some(format!("{} IS NOT TRUE", display_expr(expr)?))),
        Expr::IsNull(expr) => Ok(Some(format!("{} IS NULL", display_expr(expr)?))),
        Expr::IsNotNull(expr) => Ok(Some(format!("{} IS NOT NULL", display_expr(expr)?))),

        // IN expressions
        Expr::InList { expr, list, negated } => {
            let expr_str = display_expr(expr)?;
            let list_str = list.iter()
                .map(|e| display_expr(e))
                .collect::<Result<Vec<String>>>()?
                .join(", ");
            if *negated {
                Ok(Some(format!("({} NOT IN ({})", expr_str, list_str)))
            } else {
                Ok(Some(format!("({} IN ({})", expr_str, list_str)))
            }
        }

        // BETWEEN expressions
        Expr::Between { expr, negated, low, high } => {
            let expr_str = display_expr(expr)?;
            let low_str = display_expr(low)?;
            let high_str = display_expr(high)?;
            if *negated {
                Ok(Some(format!("{} NOT BETWEEN {} AND {}", expr_str, low_str, high_str)))
            } else {
                Ok(Some(format!("{} BETWEEN {} AND {}", expr_str, low_str, high_str)))
            }
        }

        // Binary operations
        Expr::BinaryOp { left, op, right } => {
            let left_str = display_expr(left)?;
            let right_str = display_expr(right)?;
            Ok(Some(format!("{} {} {}", left_str, op, right_str)))
        }

        // LIKE expressions
        Expr::Like { negated, any, expr, pattern, escape_char } => {
            let expr_str = display_expr(expr)?;
            let pattern_str = display_expr(pattern)?;
            let mut like_expr = if *negated { "NOT LIKE" } else { "LIKE" };
            if *any {
                like_expr = if *negated { "NOT ILIKE" } else { "ILIKE" };
            }
            let mut result = format!("({} {} {})", expr_str, like_expr, pattern_str);
            if let Some(escape) = escape_char {
                result.push_str(&format!(" ESCAPE '{}'", escape));
            }
            Ok(Some(result))
        }

        Expr::ILike { negated, any: _, expr, pattern, escape_char } => {
            let expr_str = display_expr(expr)?;
            let pattern_str = display_expr(pattern)?;
            let like_expr = if *negated { "NOT ILIKE" } else { "ILIKE" };
            let mut result = format!("({} {} {})", expr_str, like_expr, pattern_str);
            if let Some(escape) = escape_char {
                result.push_str(&format!(" ESCAPE '{}'", escape));
            }
            Ok(Some(result))
        }

        Expr::SimilarTo { negated, expr, pattern, escape_char } => {
            let expr_str = display_expr(expr)?;
            let pattern_str = display_expr(pattern)?;
            let like_expr = if *negated { "NOT SIMILAR TO" } else { "SIMILAR TO" };
            let mut result = format!("({} {} {})", expr_str, like_expr, pattern_str);
            if let Some(escape) = escape_char {
                result.push_str(&format!(" ESCAPE '{}'", escape));
            }
            Ok(Some(result))
        }

        // Unary operations
        Expr::UnaryOp { op, expr } => {
            let expr_str = display_expr(expr)?;
            Ok(Some(format!("({}{})", op, expr_str)))
        }

        // CAST expressions — only standard Cast and DoubleColon; TryCast and SafeCast are dialect-specific
        Expr::Cast { kind, expr, data_type, format: _, array: _ } => {
            match kind {
                CastKind::Cast => {
                    let expr_str = display_expr(expr)?;
                    Ok(Some(format!("CAST({} AS {})", expr_str, data_type)))
                }
                CastKind::DoubleColon => {
                    let expr_str = display_expr(expr)?;
                    Ok(Some(format!("{}::{}", expr_str, data_type)))
                }
                // TryCast and SafeCast are dialect-specific
                CastKind::TryCast | CastKind::SafeCast => Ok(None),
            }
        }

        // Math functions
        Expr::Ceil { expr, field: _ } => {
            let expr_str = display_expr(expr)?;
            Ok(Some(format!("CEIL({})", expr_str)))
        }

        Expr::Floor { expr, field: _ } => {
            let expr_str = display_expr(expr)?;
            Ok(Some(format!("FLOOR({})", expr_str)))
        }

        // String functions
        Expr::Position { expr, r#in } => {
            let expr_str = display_expr(expr)?;
            let in_str = display_expr(r#in)?;
            Ok(Some(format!("POSITION({} IN {})", expr_str, in_str)))
        }

        Expr::Substring { expr, substring_from, substring_for, special: _, shorthand } => {
            let expr_str = display_expr(expr)?;
            if *shorthand {
                if let Some(from) = substring_from {
                    let from_str = display_expr(from)?;
                    if let Some(for_expr) = substring_for {
                        let for_str = display_expr(for_expr)?;
                        Ok(Some(format!("SUBSTRING({} FROM {} FOR {})", expr_str, from_str, for_str)))
                    } else {
                        Ok(Some(format!("SUBSTRING({} FROM {})", expr_str, from_str)))
                    }
                } else {
                    Ok(Some(format!("SUBSTRING({})", expr_str)))
                }
            } else {
                let mut args = vec![expr_str];
                if let Some(from) = substring_from {
                    args.push(display_expr(from)?);
                }
                if let Some(for_expr) = substring_for {
                    args.push(display_expr(for_expr)?);
                }
                Ok(Some(format!("SUBSTRING({})", args.join(", "))))
            }
        }

        Expr::Trim { expr, trim_where, trim_what, trim_characters } => {
            let expr_str = display_expr(expr)?;
            let mut trim_expr = "TRIM".to_string();

            if let Some(where_field) = trim_where {
                match where_field {
                    TrimWhereField::Both => trim_expr = "TRIM".to_string(),
                    TrimWhereField::Leading => trim_expr = "LTRIM".to_string(),
                    TrimWhereField::Trailing => trim_expr = "RTRIM".to_string(),
                }
            }

            if let Some(what) = trim_what {
                let what_str = display_expr(what)?;
                Ok(Some(format!("{}({} FROM {})", trim_expr, what_str, expr_str)))
            } else if let Some(chars) = trim_characters {
                let chars_str = chars.iter()
                    .map(|c| display_expr(c))
                    .collect::<Result<Vec<String>>>()?
                    .join(", ");
                Ok(Some(format!("{}({}, {})", trim_expr, chars_str, expr_str)))
            } else {
                Ok(Some(format!("{}({})", trim_expr, expr_str)))
            }
        }

        Expr::Overlay { expr, overlay_what, overlay_from, overlay_for } => {
            let expr_str = display_expr(expr)?;
            let what_str = display_expr(overlay_what)?;
            let from_str = display_expr(overlay_from)?;

            if let Some(for_expr) = overlay_for {
                let for_str = display_expr(for_expr)?;
                Ok(Some(format!("OVERLAY({} PLACING {} FROM {} FOR {})", expr_str, what_str, from_str, for_str)))
            } else {
                Ok(Some(format!("OVERLAY({} PLACING {} FROM {})", expr_str, what_str, from_str)))
            }
        }

        // Nested expressions
        Expr::Nested(expr) => {
            let expr_str = display_expr(expr)?;
            Ok(Some(format!("({})", expr_str)))
        }

        // Tuple expressions
        Expr::Tuple(exprs) => {
            let exprs_str = exprs.iter()
                .map(|e| display_expr(e))
                .collect::<Result<Vec<String>>>()?
                .join(", ");
            Ok(Some(format!("({})", exprs_str)))
        }

        // Array expressions
        Expr::Array(array) => {
            let array_parts = array.elem.iter()
                .map(|e| display_expr(e))
                .collect::<Result<Vec<String>>>()?
                .join(", ");
            Ok(Some(format!("[{}]", array_parts)))
        }

        // Interval expressions
        Expr::Interval(interval) => {
            let value_str = display_expr(&interval.value)?;
            if let Some(leading_field) = &interval.leading_field {
                Ok(Some(format!("INTERVAL ({}) {}", value_str, leading_field)))
            } else {
                Ok(Some(format!("INTERVAL {}", value_str)))
            }
        }

        // Dialect-specific expressions — return None so each backend handles them
        Expr::Extract { .. } | Expr::AtTimeZone { .. } | Expr::TypedString(_) => Ok(None),

        // Unknown expressions — also return None for extensibility
        _ => Ok(None),
    }
}
