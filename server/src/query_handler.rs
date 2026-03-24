//! Axum handler for the `/query` endpoint.
//!
//! Parses incoming SQL queries, checks whether the target table requires
//! payment via the x402 protocol, and either returns results directly
//! (free tables) or orchestrates the payment-verify-settle flow before
//! returning Arrow IPC–encoded data.

use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use axum::body::Bytes;
use http::Uri;
use arrow::record_batch::RecordBatch;
use x402_types::proto::v2::{PaymentPayload, PaymentRequirements, VerifyResponse};
use x402_types::util::Base64Bytes;
use std::sync::Arc;
use serde::Deserialize;
use tracing::instrument;
use crate::AppState;
use crate::sqp_parser::{analyze_query, create_estimate_rows_query};
use crate::payment_processing::{
    settle_payment, verify_payment
};
use crate::payment_config::GlobalPaymentConfig;
use crate::database::serialize_batches_to_arrow_ipc;

/// JSON body for the `/query` endpoint.
#[derive(Debug, Deserialize)]
pub struct QueryRequest {
    /// The SQL query to execute against the database.
    pub query: String,
}

/// Main axum handler for the `/query` route.
///
/// Workflow:
/// 1. Parse and validate the SQL query.
/// 2. If the target table is free, execute and return Arrow IPC data.
/// 3. If the table requires payment and no `Payment-Signature` header is present,
///    return HTTP 402 with estimated cost and payment requirements.
/// 4. If a payment header is present, decode it, execute the query,
///    verify/settle payment via the x402 facilitator, and return the data.
#[axum::debug_handler]
#[instrument(skip_all)]
#[allow(dead_code)]
pub async fn query_handler(
    State(state): State<Arc<AppState>>,
    uri: Uri,
    headers: HeaderMap,
    Json(query_req): Json<QueryRequest>,
) -> Result<axum::response::Response, QueryError> {
    // Extract the path from the request URI
    let path = uri.path();

    // Parse and validate query first
    tracing::info!("Received query: {}", query_req.query);
    let analyzed_query = analyze_query(&query_req.query)
        .map_err(|e| QueryError::BadRequest(format!("Invalid query: {}", e)))?;

    // Create the executable SQL query for the configured database backend
    let sql = state.db.create_sql_query(&analyzed_query)
        .map_err(|e| QueryError::Internal(format!("Failed to create executable query: {}", e)))?;

    // Extract table name and check if payment is required
    let table_name = &analyzed_query.body.from;
    match state.payment_config.table_requires_payment(table_name) {
        // Table not found.
        None => {
            return Err(QueryError::BadRequest(format!("Table not supported: {}", table_name)));
        }
        // No payment required, execute query and return data
        Some(false) => {
            let buffer = run_query_to_ipc(&state, &sql).await?;
            return Ok(success_response(buffer));
        }
        // Payment required, continue to payment processing
        Some(true) => {}
    }

    match headers.get("Payment-Signature") {
        None => {
            // Step 1: No payment header - return 402 with pricing info
            // Estimate row count
            let estimated_rows = estimate_row_count(&state, &sql).await?;
            Err(QueryError::payment(
                &state.payment_config,
                "No crypto payment found. Implement x402 protocol (https://www.x402.org/) to pay for this API request.".to_string(),
                table_name,
                estimated_rows,
                path,
            ))
        }

        // Step 2: Payment header present - verify payment and return data
        // Parse payment payload
        Some(payment_header) => {
            let payment_payload = decode_payment_payload(payment_header)?;
            let batches = execute_db_query(&state, &sql).await?;
            let actual_rows = batches.iter().map(|b| b.num_rows()).sum::<usize>();

            process_payment(&state, &payment_payload, table_name, actual_rows, path, &batches).await
        }
    }
}

/// Convenience alias used by the internal helper functions.
type QueryResult<T> = Result<T, QueryError>;

/// Executes `sql` and serializes the result batches into Arrow IPC bytes.
async fn run_query_to_ipc(state: &AppState, sql: &str) -> QueryResult<Vec<u8>> {
    let batches = execute_db_query(state, sql).await?;
    serialize_batches_to_arrow_ipc(&batches)
        .map_err(|e| QueryError::Internal(format!("Failed to serialize batches to Arrow IPC: {}", e)))
}

/// Wraps `sql` in a `COUNT(*)` query and returns the estimated row count.
async fn estimate_row_count(state: &AppState, sql: &str) -> QueryResult<usize> {
    let estimated_rows_query = create_estimate_rows_query(sql);
    state.db.execute_row_count_query(&estimated_rows_query).await
        .map_err(|e| QueryError::Internal(format!("Failed to execute row count query: {}", e)))
}

/// Executes `sql` via the database backend, returning Arrow record batches.
async fn execute_db_query(state: &AppState, sql: &str) -> QueryResult<Vec<RecordBatch>> {
    state.db.execute_query(sql).await
        .map_err(|e| QueryError::Internal(format!("Failed to execute query: {}", e)))
}

/// Base64-decodes and JSON-deserializes the `Payment-Signature` header into a V2 [`PaymentPayload`].
fn decode_payment_payload(payment_header: &HeaderValue) -> QueryResult<PaymentPayload<PaymentRequirements, serde_json::Value>> {
    let base64 = Base64Bytes::from(payment_header.as_bytes());
    let decoded = base64.decode()
        .map_err(|e| QueryError::BadRequest(format!("Failed to decode payment header: {}", e)))?;
    serde_json::from_slice(&decoded)
        .map_err(|e| QueryError::BadRequest(format!("Failed to parse payment payload: {}", e)))
}

/// Verifies and settles the x402 payment, then returns the pre-computed query
/// results as an Arrow IPC response.
///
/// V2 exact matching: the payload's `accepted` field is compared directly
/// against the generated requirements. If it matches, a single verify/settle
/// cycle is performed.
async fn process_payment(
    state: &AppState,
    payment_payload: &PaymentPayload<PaymentRequirements, serde_json::Value>,
    table_name: &str,
    actual_rows: usize,
    path: &str,
    batches: &[RecordBatch],
) -> Result<axum::response::Response, QueryError> {
    // Find the matching payment requirement using V2 exact matching.
    let payment_requirement = state.payment_config
        .find_matching_payment_requirements(table_name, actual_rows, &payment_payload.accepted)
        .ok_or_else(|| QueryError::Internal("No payment offer was found matching the provided payment payload".to_string()))?;

    // Verify payment with the facilitator
    let (verify_request, verify_response) = verify_payment(
        &state.payment_config.facilitator,
        payment_payload,
        &payment_requirement,
    ).await
        .map_err(|e| QueryError::Internal(format!("Payment verification failed due to facilitator error: {}", e)))?;

    // Check verification result
    if let VerifyResponse::Invalid { reason, .. } = &verify_response {
        return Err(QueryError::payment(
            &state.payment_config,
            format!("Payment provided is invalid, verification failed: {}", reason),
            table_name,
            actual_rows,
            path,
        ));
    }

    // Settle payment
    settle_payment(verify_response, &state.payment_config.facilitator, verify_request).await
        .map_err(|e| QueryError::payment(
            &state.payment_config,
            format!("Settlement of the provided payment failed: {}", e),
            table_name,
            actual_rows,
            path,
        ))?;

    let buffer = serialize_batches_to_arrow_ipc(batches)
        .map_err(|e| QueryError::Internal(format!("Failed to serialize batches to Arrow IPC: {}", e)))?;

    Ok(success_response(buffer))
}

/// All error outcomes from the query handler, each mapping to an HTTP status code.
#[derive(Debug)]
pub enum QueryError {
    /// The client sent an invalid query, unsupported table, or malformed payment header (400).
    BadRequest(String),
    /// An unexpected server-side failure — database, serialization, or facilitator error (500).
    Internal(String),
    /// The query is valid but requires payment, or the provided payment was rejected (402).
    /// Carries both the base64-encoded payment requirements (for the `Payment-Required` header)
    /// and the raw JSON body (for `x402-fetch` clients).
    PaymentRequired { header_value: String, json_body: Vec<u8> },
}

impl QueryError {
    /// Builds a 402 error with payment options for the given table and row count.
    ///
    /// Falls back to [`QueryError::Internal`] if no matching payment configuration exists.
    fn payment(payment_config: &GlobalPaymentConfig, message: String, table_name: &str, row_count: usize, path: &str) -> Self {
        match payment_config.create_payment_required_response(&message, table_name, row_count, path) {
            Some(payment_response) => {
                let json_bytes = serde_json::to_vec(&payment_response).expect("Failed to serialize payment response");
                let encoded = Base64Bytes::encode(&json_bytes);
                let header_value = String::from_utf8(encoded.0.into_owned()).expect("Base64 is valid UTF-8");
                Self::PaymentRequired { header_value, json_body: json_bytes }
            }
            None => Self::Internal("Failed to find payment options for the table request".to_string()),
        }
    }
}

impl IntoResponse for QueryError {
    fn into_response(self) -> axum::response::Response {
        match self {
            QueryError::BadRequest(msg) => {
                tracing::info!("Request failed: {}", msg);
                (StatusCode::BAD_REQUEST, [("content-type", "text/plain")], Bytes::from(msg)).into_response()
            }
            QueryError::Internal(msg) => {
                tracing::error!("Request failed: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, [("content-type", "text/plain")], Bytes::from(msg)).into_response()
            }
            QueryError::PaymentRequired { header_value, json_body } => {
                (StatusCode::PAYMENT_REQUIRED, [("Payment-Required", header_value), ("content-type", "application/json".to_string())], Bytes::from(json_body)).into_response()
            }
        }
    }
}

/// Builds a 200 OK response with `application/vnd.apache.arrow.stream` content type.
fn success_response(data: Vec<u8>) -> axum::response::Response {
    (
        StatusCode::OK,
        [("content-type", "application/vnd.apache.arrow.stream")],
        Bytes::from(data),
    ).into_response()
}
