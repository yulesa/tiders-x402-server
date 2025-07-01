use sqlparser::{ast::OrderByKind, dialect::AnsiDialect};
use sqlparser::parser::Parser;
use sqlparser::ast::{
    Expr, GroupByExpr, LimitClause, ObjectNamePart, SelectFlavor, SelectItem, SetExpr, Statement, TableFactor, TableWithJoins};
use anyhow::{anyhow, Result};

#[derive(Debug, Clone)]
pub struct AnalyzedQuery{
    pub body: AnalyzedSelect,
    pub limit_clause: Option<AnalyzedLimitClause>,
    pub order_by: Option<Vec<AnalyzedOrderByExpr>>,
}

impl AnalyzedQuery{
    fn new() -> Self {
        Self {
            body: AnalyzedSelect::new(),
            limit_clause: None,
            order_by: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnalyzedSelect{
    pub projection: Vec<AnalyzedSelectItem>, // Items in SELECT clause
    pub wildcard: bool, // Whether * was used in SELECT clause
    pub from: String, // Table name in FROM clause
    pub selection:Option<Expr> // Expression in WHERE clause
}

impl AnalyzedSelect{
    fn new() -> Self {
        Self {
            projection: Vec::new(),
            wildcard: false,
            from: String::new(),
            selection: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnalyzedSelectItem{
    pub ident: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AnalyzedOrderByExpr{
    pub expr: Expr,
    pub asc: bool,
    pub nulls_first: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct AnalyzedLimitClause{
    pub limit: Option<Expr>,
    pub offset: Option<Expr>
}

pub fn analyze_query(sql: &str) -> Result<AnalyzedQuery> {
    let mut analyzed_query = AnalyzedQuery::new();

    let dialect = AnsiDialect {};
    let ast:Vec<Statement> = Parser::parse_sql(&dialect, sql)?;
    
    // Ensure there only one statement, and it is a query statement
    if ast.len() != 1 {
        return Err(anyhow!("Simplified SQL only supports one statement"));
    }
    match &ast[0] {
        Statement::Query(query) => {
            // Check all unsupported fields in the query
            check_unsupported_field(&query.with)?;
            check_unsupported_field(&query.fetch)?;
            check_unsupported_field(&query.locks)?;
            check_unsupported_field(&query.for_clause)?;
            check_unsupported_field(&query.settings)?;
            check_unsupported_field(&query.format_clause)?;

            // Check Query body (SELECT/FROM/WHERE)
            match &query.body.as_ref() {
                SetExpr::Select(select) => {
                    // Check all unsupported fields in the select
                    check_unsupported_field(&select.distinct)?;
                    check_unsupported_field(&select.top)?;
                    check_unsupported_field(&select.top_before_distinct)?;
                    check_unsupported_field(&select.into)?;
                    check_unsupported_field(&select.lateral_views)?;
                    check_unsupported_field(&select.prewhere)?;
                    check_unsupported_field(&select.cluster_by)?;
                    check_unsupported_field(&select.distribute_by)?;
                    check_unsupported_field(&select.sort_by)?;
                    check_unsupported_field(&select.having)?;
                    check_unsupported_field(&select.named_window)?;
                    check_unsupported_field(&select.qualify)?;
                    check_unsupported_field(&select.window_before_qualify)?;
                    check_unsupported_field(&select.value_table_mode)?;
                    check_unsupported_field(&select.connect_by)?;
                    if !matches!(select.flavor, SelectFlavor::Standard) {
                        return Err(anyhow!("Simplified SQL only supports standard 'SELECT ... FROM' statements"));
                    }
                    if !matches!(select.group_by, GroupByExpr::Expressions(ref exprs, ref sets) if exprs.is_empty() && sets.is_empty()) {
                        return Err(anyhow!("Simplified SQL does not support the use of 'GroupByExpr'"));
                    }

                    // Projections (SELECT Items)
                    for item in &select.projection {
                        match item {
                            SelectItem::UnnamedExpr(expr) => {
                                match expr {
                                    Expr::Identifier(ident) => {
                                        // Field name
                                        analyzed_query.body.projection.push(AnalyzedSelectItem{
                                            ident: ident.value.clone(),
                                            alias: None,
                                        });
                                    }
                                    _ => {
                                        return Err(anyhow!("Simplified SQL does not support the use of expressions in Select Items: {}", expr));
                                    }
                                }
                            }
                            SelectItem::ExprWithAlias{expr, alias} => {
                                match expr {
                                    Expr::Identifier(ident) => {
                                        // Field name
                                        analyzed_query.body.projection.push(AnalyzedSelectItem{
                                            ident: ident.value.clone(),
                                            alias: Some(alias.value.clone()),
                                        });
                                    }
                                    _ => {
                                        return Err(anyhow!("Simplified SQL does not support the use of expressions in Select Items: {}", expr));
                                    }
                                }
                            }
                            SelectItem::Wildcard(wildcard_options) => {
                                check_unsupported_field(&wildcard_options.opt_ilike)?;
                                check_unsupported_field(&wildcard_options.opt_exclude)?;
                                check_unsupported_field(&wildcard_options.opt_except)?;
                                check_unsupported_field(&wildcard_options.opt_replace)?;
                                check_unsupported_field(&wildcard_options.opt_rename)?;
                                // Wildcard
                                analyzed_query.body.wildcard = true;
                            }
                            SelectItem::QualifiedWildcard(_, _) => {
                                return Err(anyhow!("Simplified SQL does not support the use of Qualified Wildcard (alias.*)"));
                            }
                        }
                    }

                    // FROM clause
                    if select.from.len() != 1 {
                        return Err(anyhow!("Simplified SQL only supports one table in the FROM clause"));
                    }
                    let TableWithJoins{ relation, joins } = select.from[0].clone();
                    check_unsupported_field(&joins)?;
                    match relation {
                        TableFactor::Table { name, alias, args, with_hints, version, with_ordinality,  partitions, json_path, sample, index_hints} => {
                            check_unsupported_field(&alias)?;
                            check_unsupported_field(&args)?;
                            check_unsupported_field(&with_hints)?;
                            check_unsupported_field(&version)?;
                            check_unsupported_field(&with_ordinality)?;
                            check_unsupported_field(&partitions)?;
                            check_unsupported_field(&json_path)?;
                            check_unsupported_field(&sample)?;
                            check_unsupported_field(&index_hints)?;
                            // Table name
                            if name.0.len() == 1 {
                                let ObjectNamePart::Identifier(ident) = &name.0[0];
                                analyzed_query.body.from = ident.value.clone();
                            } else {
                                return Err(anyhow!("Simplified SQL only supports simple table in the FROM clause. No support compound table name: {}", name));
                            }
                        }
                        _ => {
                            return Err(anyhow!("Simplified SQL only supports simple table in the FROM clause"));
                        }
                    }

                    // WHERE clause
                    let selection = check_unsupported_expr(select.selection.clone())?;
                    match selection {
                        Some(selection) => {
                            // Where clause
                            analyzed_query.body.selection = Some(selection.clone());
                        }
                        None => {
                            analyzed_query.body.selection = None;
                        }
                    }
                }
                _ => {
                    return Err(anyhow!("Simplified SQL only supports SELECT statements"));
                }
            }
            
            // Check Order by clause
            match &query.order_by {
                Some(order_by) => {
                    let kind = order_by.kind.clone();
                    match kind {
                        OrderByKind::All(_) => {
                            return Err(anyhow!("Simplified SQL does not support the use of 'ALL' in ORDER BY clause"));
                        }
                        OrderByKind::Expressions(exprs) => {
                            analyzed_query.order_by = Some(Vec::new());
                            for order_by_expr in exprs {
                                if let Some(_) = &order_by_expr.with_fill {
                                    return Err(anyhow!("Simplified SQL does not support the use of 'WITH FILL' in ORDER BY clause"));
                                }
                                check_unsupported_expr(Some(order_by_expr.expr.clone()))?;
                                analyzed_query.order_by.as_mut().unwrap().push(AnalyzedOrderByExpr{
                                    expr: order_by_expr.expr.clone(),
                                    asc: order_by_expr.options.asc.unwrap_or(true),
                                    nulls_first: order_by_expr.options.nulls_first,
                                });
                            }
                        }
                    }
                    if let Some(interpolate) = &order_by.interpolate {
                        if let Some(interpolate_exprs) = &interpolate.exprs {
                            for interpolate_expr in interpolate_exprs {
                                check_unsupported_expr(interpolate_expr.expr.clone())?;
                            }
                        }
                    }
                }
                None => {}
            }

            // Check Limit clause
            match &query.limit_clause {
                Some(limit_clause) => {
                    match limit_clause {
                        LimitClause::LimitOffset {limit, offset, limit_by} => {
                            check_unsupported_expr(limit.clone())?;
                            if let Some(offset) = offset {
                                check_unsupported_expr(Some(offset.value.clone()))?;
                            }
                            if !limit_by.is_empty() {
                                return Err(anyhow!("Simplified SQL does not support the use of 'LIMIT BY' in LIMIT clause"));
                            }
                            analyzed_query.limit_clause = Some(AnalyzedLimitClause{
                                limit: limit.clone(),
                                offset: offset.as_ref().map(|o| o.value.clone()),
                            });
                        }
                        // MySQL-specific syntax; the order of expressions is reversed. ANSI dialect does not support this.
                        LimitClause::OffsetCommaLimit{..} => {
                            return Err(anyhow!("Simplified SQL does not support MySQL-specific syntax: 'OFFSET, LIMIT'"));
                        }
                    };
                }
                None => {
                    analyzed_query.limit_clause = None;
                }
            }

        }
        _ => {
            return Err(anyhow!("Simplified SQL only supports SELECT statements"));
        }

        
    }
    
    Ok(analyzed_query)
}

// Trait for types that can be checked for being unsupported
trait UnsupportedCheck {
    fn is_supported(&self) -> bool;
    // Return the name of the field that is unsupported, for error message
    fn field_name() -> &'static str;
}

impl<T> UnsupportedCheck for Option<T> {
    fn is_supported(&self) -> bool {
        self.is_none()
    }
    fn field_name() -> &'static str {
        std::any::type_name::<T>()
            .split("::")
            .last()
            .unwrap_or("unknown")
    }
}

impl<T> UnsupportedCheck for Vec<T> {
    fn is_supported(&self) -> bool {
        self.is_empty()
    }
    fn field_name() -> &'static str {
        std::any::type_name::<T>()
            .split("::")
            .last()
            .unwrap_or("unknown")
    }
}

impl UnsupportedCheck for bool {
    fn is_supported(&self) -> bool {
        !*self
    }
    fn field_name() -> &'static str {
        "boolean"
    }
}

fn check_unsupported_field<T: UnsupportedCheck + std::fmt::Debug>(field: &T) -> Result<()> {
    if !field.is_supported() {
        return Err(anyhow!("Simplified SQL does not support the use of '{}'", T::field_name()));
    }
    Ok(())
}

// Only support expressions that don't take subqueries or deep down expressions
fn check_unsupported_expr(expr: Option<Expr>) -> Result<Option<Expr>> {
    match expr {
        Some(expr) => {
            match expr {
                // First check for explicitly unsupported expressions. Being restrictive here is better than being lenient.
                Expr::CompoundIdentifier(_)
                | Expr::CompoundFieldAccess{..}
                | Expr::JsonAccess {..}
                | Expr::IsUnknown(_)
                | Expr::IsNotUnknown(_)
                | Expr::IsDistinctFrom(_, _)
                | Expr::IsNotDistinctFrom(_, _)
                | Expr::IsNormalized{..}
                | Expr::InSubquery{..}
                | Expr::InUnnest{..}
                | Expr::RLike{..}
                | Expr::AnyOp{..}
                | Expr::AllOp{..}
                | Expr::Convert{..}
                | Expr::Collate{..}
                | Expr::Prefixed { .. }
                | Expr::Function{..}
                | Expr::Case{..}
                | Expr::Exists{..}
                | Expr::Subquery{..}
                | Expr::GroupingSets(_)
                | Expr::Cube(_)
                | Expr::Rollup(_)
                | Expr::Struct { .. }
                | Expr::Named{..}
                | Expr::Dictionary(_)
                | Expr::Map(_)
                | Expr::MatchAgainst{..}
                | Expr::Wildcard(_)
                | Expr::QualifiedWildcard(_, _)
                | Expr::OuterJoin(_)
                | Expr::Prior(_)
                | Expr::Lambda(_) => {
                    return Err(anyhow!("Simplified SQL does not support the use of {} expressions", expr));
                }
                
                // Simple expressions that don't need recursive checking
                Expr::Identifier(_)
                | Expr::Value(_)
                | Expr::TypedString { .. } => {
                    Ok(Some(expr))
                }
                
                // Then recursively check sub-expressions in supported variants
                Expr::IsFalse(expr) => {
                    let expr_clone = expr.clone();
                    check_unsupported_expr(Some(*expr_clone))?;
                    Ok(Some(Expr::IsFalse(expr)))
                }
                Expr::IsNotFalse(expr) => {
                    let expr_clone = expr.clone();
                    check_unsupported_expr(Some(*expr_clone))?;
                    Ok(Some(Expr::IsNotFalse(expr)))
                }
                Expr::IsTrue(expr) => {
                    let expr_clone = expr.clone();
                    check_unsupported_expr(Some(*expr_clone))?;
                    Ok(Some(Expr::IsTrue(expr)))
                }
                Expr::IsNotTrue(expr) => {
                    let expr_clone = expr.clone();
                    check_unsupported_expr(Some(*expr_clone))?;
                    Ok(Some(Expr::IsNotTrue(expr)))
                }
                Expr::IsNull(expr) => {
                    let expr_clone = expr.clone();
                    check_unsupported_expr(Some(*expr_clone))?;
                    Ok(Some(Expr::IsNull(expr)))
                }
                Expr::IsNotNull(expr) => {
                    let expr_clone = expr.clone();
                    check_unsupported_expr(Some(*expr_clone))?;
                    Ok(Some(Expr::IsNotNull(expr)))
                }
                Expr::InList { expr, list, negated } => {
                    let expr_clone = expr.clone();
                    check_unsupported_expr(Some(*expr_clone))?;
                    for e in &list {
                        check_unsupported_expr(Some(e.clone()))?;
                    }
                    Ok(Some(Expr::InList { expr, list, negated }))
                }
                Expr::Between { expr, negated, low, high } => {
                    let expr_clone = expr.clone();
                    let low_clone = low.clone();
                    let high_clone = high.clone();
                    check_unsupported_expr(Some(*expr_clone))?;
                    check_unsupported_expr(Some(*low_clone))?;
                    check_unsupported_expr(Some(*high_clone))?;
                    Ok(Some(Expr::Between { expr, negated, low, high }))
                }
                Expr::BinaryOp { left, op, right } => {
                    let left_clone = left.clone();
                    let right_clone = right.clone();
                    check_unsupported_expr(Some(*left_clone))?;
                    check_unsupported_expr(Some(*right_clone))?;
                    Ok(Some(Expr::BinaryOp { left, op, right }))
                }
                Expr::Like { negated, any, expr, pattern, escape_char } => {
                    let expr_clone = expr.clone();
                    let pattern_clone = pattern.clone();
                    check_unsupported_expr(Some(*expr_clone))?;
                    check_unsupported_expr(Some(*pattern_clone))?;
                    Ok(Some(Expr::Like { negated, any, expr, pattern, escape_char }))
                }
                Expr::ILike { negated, any, expr, pattern, escape_char } => {
                    let expr_clone = expr.clone();
                    let pattern_clone = pattern.clone();
                    check_unsupported_expr(Some(*expr_clone))?;
                    check_unsupported_expr(Some(*pattern_clone))?;
                    Ok(Some(Expr::ILike { negated, any, expr, pattern, escape_char }))
                }
                Expr::SimilarTo { negated, expr, pattern, escape_char } => {
                    let expr_clone = expr.clone();
                    let pattern_clone = pattern.clone();
                    check_unsupported_expr(Some(*expr_clone))?;
                    check_unsupported_expr(Some(*pattern_clone))?;
                    Ok(Some(Expr::SimilarTo { negated, expr, pattern, escape_char }))
                }
                Expr::UnaryOp { op, expr } => {
                    let expr_clone = expr.clone();
                    check_unsupported_expr(Some(*expr_clone))?;
                    Ok(Some(Expr::UnaryOp { op, expr }))
                }
                Expr::Cast { kind, expr, data_type, format } => {
                    let expr_clone = expr.clone();
                    check_unsupported_expr(Some(*expr_clone))?;
                    Ok(Some(Expr::Cast { kind, expr, data_type, format }))
                }
                Expr::AtTimeZone { timestamp, time_zone } => {
                    let timestamp_clone = timestamp.clone();
                    let time_zone_clone = time_zone.clone();
                    check_unsupported_expr(Some(*timestamp_clone))?;
                    check_unsupported_expr(Some(*time_zone_clone))?;
                    Ok(Some(Expr::AtTimeZone { timestamp, time_zone }))
                }
                Expr::Extract { field, syntax, expr } => {
                    let expr_clone = expr.clone();
                    check_unsupported_expr(Some(*expr_clone))?;
                    Ok(Some(Expr::Extract { field, syntax, expr }))
                }
                Expr::Ceil { expr, field } => {
                    let expr_clone = expr.clone();
                    check_unsupported_expr(Some(*expr_clone))?;
                    Ok(Some(Expr::Ceil { expr, field }))
                }
                Expr::Floor { expr, field } => {
                    let expr_clone = expr.clone();
                    check_unsupported_expr(Some(*expr_clone))?;
                    Ok(Some(Expr::Floor { expr, field }))
                }
                Expr::Position { expr, r#in } => {
                    let expr_clone = expr.clone();
                    let in_clone = r#in.clone();
                    check_unsupported_expr(Some(*expr_clone))?;
                    check_unsupported_expr(Some(*in_clone))?;
                    Ok(Some(Expr::Position { expr, r#in }))
                }
                Expr::Substring { expr, substring_from, substring_for, special, shorthand } => {
                    let expr_clone = expr.clone();
                    check_unsupported_expr(Some(*expr_clone))?;
                    if let Some(from) = &substring_from {
                        let from_clone = from.clone();
                        check_unsupported_expr(Some(*from_clone))?;
                    }
                    if let Some(for_expr) = &substring_for {
                        let for_expr_clone = for_expr.clone();
                        check_unsupported_expr(Some(*for_expr_clone))?;
                    }
                    Ok(Some(Expr::Substring { expr, substring_from, substring_for, special, shorthand }))
                }
                Expr::Trim { expr, trim_where, trim_what, trim_characters } => {
                    let expr_clone = expr.clone();
                    check_unsupported_expr(Some(*expr_clone))?;
                    if let Some(what) = &trim_what {
                        let what_clone = what.clone();
                        check_unsupported_expr(Some(*what_clone))?;
                    }
                    if let Some(chars) = &trim_characters {
                        for e in chars {
                            check_unsupported_expr(Some(e.clone()))?;
                        }
                    }
                    Ok(Some(Expr::Trim { expr, trim_where, trim_what, trim_characters }))
                }
                Expr::Overlay { expr, overlay_what, overlay_from, overlay_for } => {
                    let expr_clone = expr.clone();
                    let overlay_what_clone = overlay_what.clone();
                    let overlay_from_clone = overlay_from.clone();
                    check_unsupported_expr(Some(*expr_clone))?;
                    check_unsupported_expr(Some(*overlay_what_clone))?;
                    check_unsupported_expr(Some(*overlay_from_clone))?;
                    if let Some(for_expr) = &overlay_for {
                        let for_expr_clone = for_expr.clone();
                        check_unsupported_expr(Some(*for_expr_clone))?;
                    }
                    Ok(Some(Expr::Overlay { expr, overlay_what, overlay_from, overlay_for }))
                }
                Expr::Nested(expr) => {
                    let expr_clone = expr.clone();
                    check_unsupported_expr(Some(*expr_clone))?;
                    Ok(Some(Expr::Nested(expr)))
                }
                Expr::Tuple(exprs) => {
                    for e in &exprs {
                        check_unsupported_expr(Some(e.clone()))?;
                    }
                    Ok(Some(Expr::Tuple(exprs)))
                }
                Expr::Array(array) => {
                    for elem in &array.elem {
                        check_unsupported_expr(Some(elem.clone()))?;
                    }
                    Ok(Some(Expr::Array(array)))
                }
                Expr::Interval(interval) => {
                    let value = interval.value.clone();
                    check_unsupported_expr(Some(*value))?;
                    Ok(Some(Expr::Interval(interval)))
                }
            }
        }
        None => {
            Ok(None)
        }
    }
}
