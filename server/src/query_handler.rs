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
use x402_rs::proto::v1::{PaymentPayload, VerifyResponse};
use x402_rs::util::Base64Bytes;
use std::sync::Arc;
use serde::Deserialize;
use tracing::instrument;
use crate::AppState;
use crate::sqp_parser::{analyze_query, create_estimate_rows_query};
use crate::duckdb_reader::create_duckdb_query;
use crate::payment_processing::{
    settle_payment, verify_payment
};
use crate::payment_config::GlobalPaymentConfig;
use crate::database::{execute_query, execute_row_count_query, serialize_batches_to_arrow_ipc};

/// JSON body for the `/query` endpoint.
#[derive(Debug, Deserialize)]
pub struct QueryRequest {
    /// The SQL query to execute against the DuckDB database.
    pub query: String,
}

/// Main axum handler for the `/query` route.
///
/// Workflow:
/// 1. Parse and validate the SQL query.
/// 2. If the target table is free, execute and return Arrow IPC data.
/// 3. If the table requires payment and no `X-Payment` header is present,
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

    // Create the executable DuckDB SQL query
    let duckdb_sql = create_duckdb_query(&analyzed_query)
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
            let buffer = run_query_to_ipc(&state, &duckdb_sql)?;
            return Ok(success_response(buffer));
        }
        // Payment required, continue to payment processing
        Some(true) => {}
    }

    match headers.get("X-Payment") {
        None => {
            // Phase 1: No payment header - return 402 with pricing info
            // Estimate row count
            let estimated_rows = estimate_row_count(&state, &duckdb_sql)?;
            Err(QueryError::payment(
                &state.payment_config,
                "No crypto payment found. Implement x402 protocol (https://www.x402.org/) to pay for this API request.".to_string(),
                table_name,
                estimated_rows,
                path,
            ))
        }
        
        // Phase 2: Payment header present - verify payment and return data
        // Parse payment payload
        Some(payment_header) => {
            let payment_payload = decode_payment_payload(payment_header)?;
            let batches = execute_db_query(&state, &duckdb_sql)?;
            let actual_rows = batches.iter().map(|b| b.num_rows()).sum::<usize>();

            process_payment(&state, &payment_payload, table_name, actual_rows, path, &batches).await
        }
    }
}

/// Convenience alias used by the internal helper functions.
type QueryResult<T> = Result<T, QueryError>;

/// Executes `duckdb_sql` and serializes the result batches into Arrow IPC bytes.
fn run_query_to_ipc(state: &AppState, duckdb_sql: &str) -> QueryResult<Vec<u8>> {
    let batches = execute_db_query(state, duckdb_sql)?;
    serialize_batches_to_arrow_ipc(&batches)
        .map_err(|e| QueryError::Internal(format!("Failed to serialize batches to Arrow IPC: {}", e)))
}

/// Wraps `duckdb_sql` in a `COUNT(*)` query and returns the estimated row count.
fn estimate_row_count(state: &AppState, duckdb_sql: &str) -> QueryResult<usize> {
    let estimated_rows_query = create_estimate_rows_query(duckdb_sql);
    let db = state.db.lock()
        .map_err(|e| QueryError::Internal(format!("Failed to lock database: {}", e)))?;
    execute_row_count_query(&db, &estimated_rows_query)
        .map_err(|e| QueryError::Internal(format!("Failed to execute row count query: {}", e)))
}

/// Acquires the database lock and executes `duckdb_sql`, returning Arrow record batches.
fn execute_db_query(state: &AppState, duckdb_sql: &str) -> QueryResult<Vec<RecordBatch>> {
    let db = state.db.lock()
        .map_err(|e| QueryError::Internal(format!("Failed to lock database: {}", e)))?;
    execute_query(&db, duckdb_sql)
        .map_err(|e| QueryError::Internal(format!("Failed to execute query: {}", e)))
}

/// Base64-decodes and JSON-deserializes the `X-Payment` header into a [`PaymentPayload`].
fn decode_payment_payload(payment_header: &HeaderValue) -> QueryResult<PaymentPayload> {
    let base64 = Base64Bytes::from(payment_header.as_bytes());
    let decoded = base64.decode()
        .map_err(|e| QueryError::BadRequest(format!("Failed to decode payment header: {}", e)))?;
    serde_json::from_slice(&decoded)
        .map_err(|e| QueryError::BadRequest(format!("Failed to parse payment payload: {}", e)))
}

/// Verifies and settles the x402 payment, then returns the pre-computed query
/// results as an Arrow IPC response.
///
/// For each matching payment requirement, a verify request is sent to the
/// facilitator. If at least one verification succeeds, the first valid payment
/// is settled. On settlement success the already-executed `batches` are
/// serialized and returned.
async fn process_payment(
    state: &AppState,
    payment_payload: &PaymentPayload,
    table_name: &str,
    actual_rows: usize,
    path: &str,
    batches: &[RecordBatch],
) -> Result<axum::response::Response, QueryError> {
    // Find matching payment requirements for the provided payment payload.
    let payment_requirements = state.payment_config.find_matching_payment_requirements(
        table_name, actual_rows, path, payment_payload,
    );
    if payment_requirements.is_empty() {
        return Err(QueryError::Internal("No payment offer was found matching the provided payment payload".to_string()));
    }

    let mut verify_results = Vec::new();
    for payment_requirement in payment_requirements {
        // Construct a v1 VerifyRequest and convert to proto
        let v1_verify_request = x402_rs::proto::v1::VerifyRequest {
            x402_version: x402_rs::proto::v1::X402Version1,
            payment_payload: payment_payload.clone(),
            payment_requirements: payment_requirement,
        };
        let proto_verify_request: x402_rs::proto::VerifyRequest = v1_verify_request.try_into()
            .map_err(|e| QueryError::Internal(format!("Failed to construct verify request: {}", e)))?;
        let verify_response = verify_payment(&state.payment_config.facilitator, &proto_verify_request).await
            .map_err(|e| QueryError::Internal(format!("Payment verification failed due to facilitator error: {}", e)))?;
        verify_results.push((proto_verify_request, verify_response));
    }

    let (valid, invalid_reasons): (Vec<_>, Vec<_>) = verify_results.into_iter().partition(|(_, resp)| {
        matches!(resp, VerifyResponse::Valid { .. })
    });
    let invalid_reasons: Vec<String> = invalid_reasons.into_iter().filter_map(|(_, resp)| {
        if let VerifyResponse::Invalid { reason, .. } = resp { Some(reason) } else { None }
    }).collect();

    
    // If no valid verify responses, return a 402 with the invalid reasons
    if valid.is_empty() {
        return Err(QueryError::payment(
            &state.payment_config,
            format!("Payment provided is invalid, verification failed: {}", invalid_reasons.join(", ")),
            table_name,
            actual_rows,
            path,
        ));
    }

    if valid.len() > 1 {
        tracing::error!("Multiple payment offers were found matching a provided payment payload, duplicate payment offers exist");
    }

    let (verify_request, verify_response) = valid.into_iter().next().unwrap();
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

/// Error type returned by the query handler, mapped to HTTP status codes
/// via its [`IntoResponse`] implementation.
#[derive(Debug)]
pub enum QueryError {
    /// 400 — the client sent a malformed query or payment header.
    BadRequest(String),
    /// 500 — an unexpected server-side failure (database lock, serialization, facilitator).
    Internal(String),
    /// 402 — valid request but payment is required or the provided payment was rejected.
    PaymentRequired(Bytes),
}

impl QueryError {
    /// Builds a [`QueryError::PaymentRequired`] with the payment options looked up
    /// from `payment_config`, or falls back to [`QueryError::Internal`] if no
    /// matching payment configuration exists.
    fn payment(payment_config: &GlobalPaymentConfig, message: String, table_name: &str, row_count: usize, path: &str) -> Self {
        match payment_config.create_payment_required_response(&message, table_name, row_count, path) {
            Some(payment_response) => {
                let response_body = serde_json::to_vec(&payment_response).expect("Failed to serialize payment response");
                Self::PaymentRequired(Bytes::from(response_body))
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
            QueryError::PaymentRequired(body) => {
                (StatusCode::PAYMENT_REQUIRED, [("content-type", "application/json")], body).into_response()
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
