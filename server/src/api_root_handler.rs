//! Axum handler for the `GET /api/` endpoint.
//!
//! Returns a plain-text overview of the server: usage instructions,
//! available tables with their names, descriptions, and payment status.

use crate::AppState;
use axum::extract::State;
use axum::response::IntoResponse;
use std::fmt::Write as _;
use std::sync::Arc;

/// Handles `GET /api/` — returns a plain-text summary of available tables.
#[axum::debug_handler]
pub async fn api_root_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let payment_config = state.payment_config.read().await.clone();

    let mut response = String::new();
    let _ = writeln!(response, "Welcome to the Tiders-x402 API!\n");
    let _ = writeln!(response, "Usage:");
    let _ = writeln!(
        response,
        "- Send a POST request to /api/query with a JSON body: {{ \"query\": \"SELECT ... FROM ...\" }}"
    );
    let _ = writeln!(
        response,
        "- You must implement the x402 payment protocol to access paid tables."
    );
    let _ = writeln!(
        response,
        "- See x402 protocol docs: https://x402.gitbook.io/x402\n"
    );
    let _ = writeln!(response, "Supported tables:");
    for (table, offer) in &payment_config.offers_tables {
        let _ = writeln!(response, "- Table: {table}");
        if let Some(desc) = &offer.description {
            let _ = writeln!(response, "  Description: {desc}");
        }
        let _ = writeln!(response, "  Payment required: {}", offer.requires_payment);
        let _ = writeln!(response, "  Details: GET /api/table/{table}");
    }
    let _ = writeln!(response, "\nSQL parser rules:");
    let _ = writeln!(response, "- Only SELECT statements are supported.");
    let _ = writeln!(response, "- Only one statement per request.");
    let _ = writeln!(response, "- Only one table in the FROM clause.");
    let _ = writeln!(response, "- No GROUP BY, HAVING, JOIN, or subqueries.");
    let _ = writeln!(
        response,
        "- Only simple field names in SELECT, no expressions."
    );
    let _ = writeln!(
        response,
        "- WHERE, ORDER BY, and LIMIT are supported with restrictions."
    );
    response
}
