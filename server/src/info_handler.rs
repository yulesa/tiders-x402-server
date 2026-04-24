//! Axum handler for the `GET /api` endpoint — the API's root.
//!
//! Returns a JSON summary of the server: name, version, the list of
//! tables the server offers, and the SQL parser rules enforced on
//! `POST /api/query`. Consumed by CLIs, discovery tools, and any
//! client that wants a machine-readable view of what the server sells.

use crate::AppState;
use axum::Json;
use axum::extract::State;
use serde::Serialize;
use std::sync::Arc;

/// Handles `GET /api` — returns a JSON summary of the server.
#[axum::debug_handler]
#[allow(dead_code)]
pub async fn info_handler(State(state): State<Arc<AppState>>) -> Json<InfoResponse> {
    let payment_config = state.payment_config.read().await.clone();

    let mut tables: Vec<TableSummary> = payment_config
        .offers_tables
        .iter()
        .map(|(name, offer)| TableSummary {
            name: name.clone(),
            description: offer.description.clone(),
            payment_required: offer.requires_payment,
            details_url: format!("/api/table/{name}"),
        })
        .collect();
    // HashMap iteration order is non-deterministic; sort so /api is
    // stable across requests and process restarts.
    tables.sort_by(|a, b| a.name.cmp(&b.name));

    Json(InfoResponse {
        name: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
        tables,
        sql_rules: SQL_RULES,
        x402_docs_url: "https://x402.gitbook.io/x402",
    })
}

/// Top-level `/api` response body.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoResponse {
    pub name: &'static str,
    pub version: &'static str,
    pub tables: Vec<TableSummary>,
    pub sql_rules: &'static [&'static str],
    pub x402_docs_url: &'static str,
}

/// One table entry in the `/api` response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSummary {
    pub name: String,
    pub description: Option<String>,
    pub payment_required: bool,
    pub details_url: String,
}

/// Rules the `POST /api/query` SQL parser enforces. Kept as a static list
/// so `GET /api` can advertise them verbatim without duplicating logic
/// from `sqp_parser`.
const SQL_RULES: &[&str] = &[
    "Only SELECT statements are supported.",
    "Only one statement per request.",
    "Only one table in the FROM clause.",
    "No GROUP BY, HAVING, JOIN, or subqueries.",
    "Only simple field names in SELECT, no expressions.",
    "WHERE, ORDER BY, and LIMIT are supported with restrictions.",
];
