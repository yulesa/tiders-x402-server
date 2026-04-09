//! Axum handler for the `GET /` (root) endpoint.
//!
//! Returns a plain-text overview of the server: usage instructions,
//! available tables with their names, descriptions, and payment status.

use crate::AppState;
use axum::extract::State;
use axum::response::IntoResponse;
use std::fmt::Write as _;
use std::sync::Arc;

/// Handles `GET /` — returns a plain-text summary of available tables.
#[axum::debug_handler]
#[allow(dead_code)]
pub async fn root_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let payment_config = state.payment_config.read().await.clone();

    let mut response = String::new();
    writeln!(response, "Welcome to the Tiders-x402 API!\n").unwrap();
    writeln!(response, "Usage:").unwrap();
    writeln!(
        response,
        "- Send a POST request to /query with a JSON body: {{ \"query\": \"SELECT ... FROM ...\" }}"
    )
    .unwrap();
    writeln!(
        response,
        "- You must implement the x402 payment protocol to access paid tables."
    )
    .unwrap();
    writeln!(
        response,
        "- See x402 protocol docs: https://x402.gitbook.io/x402\n"
    )
    .unwrap();
    writeln!(response, "Supported tables:").unwrap();
    for (table, offer) in &payment_config.offers_tables {
        writeln!(response, "- Table: {table}").unwrap();
        if let Some(desc) = &offer.description {
            writeln!(response, "  Description: {desc}").unwrap();
        }
        writeln!(response, "  Payment required: {}", offer.requires_payment).unwrap();
        writeln!(response, "  Details: GET /table/{table}").unwrap();
    }
    writeln!(response, "\nSQL parser rules:").unwrap();
    writeln!(response, "- Only SELECT statements are supported.").unwrap();
    writeln!(response, "- Only one statement per request.").unwrap();
    writeln!(response, "- Only one table in the FROM clause.").unwrap();
    writeln!(response, "- No GROUP BY, HAVING, JOIN, or subqueries.").unwrap();
    writeln!(
        response,
        "- Only simple field names in SELECT, no expressions."
    )
    .unwrap();
    writeln!(
        response,
        "- WHERE, ORDER BY, and LIMIT are supported with restrictions."
    )
    .unwrap();
    response
}
