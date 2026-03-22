use axum::extract::State;
use axum::response::IntoResponse;
use std::sync::Arc;
use std::fmt::Write as _;
use crate::AppState;

#[axum::debug_handler]
#[allow(dead_code)]
pub async fn root_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut response = String::new();
    writeln!(response, "Welcome to the Tiders-x402 API!\n").unwrap();
    writeln!(response, "Usage:").unwrap();
    writeln!(response, "- Send a POST request to /query with a JSON body: {{ \"query\": \"SELECT ... FROM ...\" }}").unwrap();
    writeln!(response, "- You must implement the x402 payment protocol to access paid tables.").unwrap();
    writeln!(response, "- See x402 protocol docs: https://x402.gitbook.io/x402\n").unwrap();
    writeln!(response, "Supported tables:").unwrap();
    for (table, offer) in &state.payment_config.table_offers {
        writeln!(response, "- Table: {}", table).unwrap();
        if let Some(schema) = &offer.schema {
            writeln!(response, "  Schema:").unwrap();
            for field in schema.fields() {
                writeln!(response, "    - {}: {}", field.name(), field.data_type()).unwrap();
            }
        } else {
            writeln!(response, "  Schema: unavailable").unwrap();
        }
        if let Some(desc) = &offer.description {
            writeln!(response, "  Description: {}", desc).unwrap();
        }
        writeln!(response, "  Payment required: {}", offer.requires_payment).unwrap();
    }
    writeln!(response, "\nSQL parser rules:").unwrap();
    writeln!(response, "- Only SELECT statements are supported.").unwrap();
    writeln!(response, "- Only one statement per request.").unwrap();
    writeln!(response, "- Only one table in the FROM clause.").unwrap();
    writeln!(response, "- No GROUP BY, HAVING, JOIN, or subqueries.").unwrap();
    writeln!(response, "- Only simple field names in SELECT, no expressions.").unwrap();
    writeln!(response, "- WHERE, ORDER BY, and LIMIT are supported with restrictions.").unwrap();
    response
}
