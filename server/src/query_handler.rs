use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use axum::body::Bytes;
use duckdb::Connection;
use http::Uri;
use x402_rs::types::{PaymentRequirements, VerifyRequest};
use std::sync::{Arc, Mutex};
use serde::Deserialize;
use tracing::instrument;

use crate::sqp_parser::{analyze_query, create_estimate_rows_query};
use crate::duckdb_reader::create_duckdb_query;
use crate::payment_processing::{
    settle_payment, verify_payment
};
use crate::payment_config::GlobalPaymentConfig;
use crate::database::{execute_query, execute_row_count_query, serialize_batches_to_arrow_ipc};

#[derive(Debug, Deserialize)]
pub struct QueryRequest {
    pub query: String,
}

#[derive(Debug, Clone)]

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub payment_config: Arc<GlobalPaymentConfig>,
}

#[axum::debug_handler]
#[instrument(skip_all)]
#[allow(dead_code)] // Used by axum router via function pointer
pub async fn query_handler(
    State(state): State<Arc<AppState>>,
    uri: Uri,
    headers: HeaderMap,
    Json(query_req): Json<QueryRequest>,
) -> impl IntoResponse {
    // Extract the path from the request URI
    let path = uri.path();
    
    // Parse and validate query first
    tracing::info!("Received query: {}", query_req.query);
    let analyzed_query = match analyze_query(&query_req.query) {
        Ok(query) => query,
        Err(e) => {
            return QueryResponse::bad_request(format!("Invalid query: {}", e)).into_response();
        }
    };

    // Create the executable DuckDB SQL query
    let duckdb_sql = match create_duckdb_query(analyzed_query.clone()) {
        Ok(sql) => sql,
        Err(e) => {
            return QueryResponse::internal_error(format!("Failed to create executable query: {}", e)).into_response();
        }
    };

    // Extract table name and check if payment is required 
    let table_name = &analyzed_query.body.from;
    match state.payment_config.table_requires_payment(table_name) {
        None => {
            return QueryResponse::bad_request(format!("Table not supported: {}", table_name)).into_response();
        },
        Some(false) => {
            // Execute query and return data
            let batches = {
                let db = match state.db.lock() {
                    Ok(db) => db,
                    Err(e) => {
                        return QueryResponse::internal_error(format!("Failed to lock database: {}", e)).into_response();
                    }
                };
                match execute_query(&db, &duckdb_sql) {
                    Ok(batches) => batches,
                    Err(e) => {
                        return QueryResponse::internal_error(format!("Failed to execute query: {}", e)).into_response();
                    }
                }
            };
    
            let buffer = match serialize_batches_to_arrow_ipc(&batches) {
                Ok(buffer) => buffer,
                Err(e) => {
                    return QueryResponse::internal_error(format!("Failed to serialize batches to Arrow IPC: {}", e)).into_response();
                }
            };
            return QueryResponse::success(buffer).into_response();
        }
        Some(true) => {
            // Payment required, continue to payment processing
        }
    }

    let payment_header = match headers.get("X-Payment") {
        Some(header) => header,
        None => {
            // Phase 1: No payment header - return 402 with pricing info
            // Estimate row count
            let estimated_rows_query = create_estimate_rows_query(&duckdb_sql);
            let db = match state.db.lock() {
                Ok(db) => db,
                Err(e) => {
                    return QueryResponse::internal_error(format!("Failed to lock database: {}", e)).into_response();
                }
            };
            let estimated_rows = match execute_row_count_query(&db, &estimated_rows_query) {
                Ok(rows) => rows,
                Err(e) => {
                    return QueryResponse::internal_error(format!("Failed to execute row count query: {}", e)).into_response();
                }
            };
            return create_payment_response(
                &state.payment_config,
                &format!("No crypto payment found. Implement x402 protocol (https://www.x402.org/) to pay for this API request."),
                table_name,
                estimated_rows,
                path,
            ).into_response();
        }
    };
    
    // Phase 2: Payment header present - verify payment and return data
    // Parse payment payload
    let base64 = x402_rs::types::Base64Bytes::from(payment_header.as_bytes());
    let payment_payload = match x402_rs::types::PaymentPayload::try_from(base64) {
        Ok(payment_payload) => payment_payload,
        Err(e) => {
            return QueryResponse::bad_request(format!("Failed to parse payment payload: {}", e)).into_response();
        }
    };

    // Execute query and verify row count
    let batches = {
        let db = match state.db.lock() {
            Ok(db) => db,
            Err(e) => {
                return QueryResponse::internal_error(format!("Failed to lock database: {}", e)).into_response();
            }
        };
        match execute_query(&db, &duckdb_sql) {
            Ok(batches) => batches,
            Err(e) => {
                return QueryResponse::internal_error(format!("Failed to pre-execute query: {}", e)).into_response();
            }
        }
    };
    let actual_rows = batches.iter().map(|batch| batch.num_rows()).sum::<usize>();

    // Find matching payment requirements for the provided payment payload.
    let payment_requirements = state.payment_config.find_matching_payment_requirements(
        table_name,
        actual_rows,
        path,
        &payment_payload,
    );
    if payment_requirements.is_empty() {
        return QueryResponse::internal_error("No payment offer was found matching the provided payment payload".to_string()).into_response();
    }

    // Verify payment for each payment requirement, ideally we should only have one.
    let mut verify_responses = Vec::new();
    let mut verify_requests = Vec::new();
    for payment_requirement in payment_requirements {
        let verify_request = VerifyRequest {
            x402_version: payment_payload.x402_version,
            payment_payload,
            payment_requirements: payment_requirement,
        };
        verify_requests.push(verify_request.clone());
        let verify_response = match verify_payment(
            &state.payment_config.facilitator,
            &verify_request,
        ).await {
            Ok(result) => result,
            //An Error here doesn't mean the payment is invalid, it means the connection to the facilitator, or the facilitator is having issues
            Err(e) => {
                return QueryResponse::internal_error(format!("Payment verification failed due to facilitator error: {}", e)).into_response();
            }
        };
        verify_responses.push(verify_response);
    }

    let mut invalid_reasons = Vec::new();
    let mut valid_verify_requests = Vec::new();
    let mut valid_verify_responses = Vec::new();

    // Check if there is at least one valid verify response collecting the invalid reasons for an eventual 402 response if all are invalid.
    for (index, response) in verify_responses.iter().enumerate() {
        match response {
            x402_rs::types::VerifyResponse::Valid { .. } => {
                valid_verify_requests.push(verify_requests[index].clone());
                valid_verify_responses.push(response);
            },
            x402_rs::types::VerifyResponse::Invalid { reason, .. } => {
                invalid_reasons.push(reason);
            },
        }
    }

    // If no valid verify responses, return a 402 with the invalid reasons
    if valid_verify_responses.is_empty() {
        return create_payment_response(
            &state.payment_config,
            &format!("Payment provided is invalid, verification failed: {}", invalid_reasons.iter().map(|reason| reason.to_string()).collect::<Vec<String>>().join(", ")),
            table_name,
            actual_rows,
            path,
        ).into_response()
    }

    if valid_verify_responses.len() > 1 {
        tracing::error!("Multiple payment offers were found matching a provided payment payload, duplicate payment offers exist: {:?}", valid_verify_requests.iter().map(|request| &request.payment_requirements).collect::<Vec<&PaymentRequirements>>());
    }

    // Get the first valid verify response and requirement
    let verify_response = valid_verify_responses[0].clone();
    let verify_request = valid_verify_requests[0].clone();
    // Settle payment
    match settle_payment(
        verify_response,
        &state.payment_config.facilitator,
        verify_request,
    ).await {
        Ok(_) => {
            // Payment settled, execute query and verify row count
        }
        Err(e) => {
            // Payment settlement failed
            create_payment_response(
                &state.payment_config,
                &format!("Settlement of the provided payment failed: {}", e),
                table_name,
                actual_rows,
                path,
            ).into_response();
        }
    }
    
    let buffer = match serialize_batches_to_arrow_ipc(&batches) {
        Ok(buffer) => buffer,
        Err(e) => {
            return QueryResponse::internal_error(format!("Failed to serialize batches to Arrow IPC: {}", e)).into_response();
        }
    };
    
    return QueryResponse::success(buffer).into_response();
}

#[derive(Debug)]
pub struct QueryResponse {
    status: StatusCode,
    content_type: &'static str,
    body: Bytes,
}

// Helper function to create payment required responses
fn create_payment_response(
    payment_config: &GlobalPaymentConfig,
    message: &str,
    table_name: &str,
    row_count: usize,
    path: &str,
) -> QueryResponse {
    match payment_config.create_payment_required_response(message, table_name, row_count, path) {
        Some(payment_response) => {
            let response_body = serde_json::to_vec(&payment_response).expect("Failed to serialize payment response");
            QueryResponse::payment_required(Bytes::from(response_body))
        }
        None => QueryResponse::internal_error(
            "Failed to find payment options for the table request".to_string(),
        ),
    }
}

impl QueryResponse {
    pub fn success(data: Vec<u8>) -> Self {
        Self {
            status: StatusCode::OK,
            content_type: "application/vnd.apache.arrow.stream",
            body: Bytes::from(data),
        }
    }

    pub fn internal_error(message: String) -> Self {
        let response = Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            content_type: "text/plain",
            body: Bytes::from(message),
        };
        tracing::error!("Request failed: {:?}", response);
        response
    }

    pub fn bad_request(message: String) -> Self {
        let response = Self {
            status: StatusCode::BAD_REQUEST,
            content_type: "text/plain",
            body: Bytes::from(message),
        };
        tracing::info!("Request failed: {:?}", response);
        response
    }

    pub fn payment_required(payment_response: Bytes) -> Self {
        Self {
            status: StatusCode::PAYMENT_REQUIRED,
            content_type: "application/json",
            body: payment_response,
        }
    }

    pub fn into_response(self) -> impl IntoResponse {
        (
            self.status,
            [("content-type", self.content_type)],
            self.body,
        )
    }
}