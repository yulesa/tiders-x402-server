use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use axum::body::Bytes;
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use duckdb::{Connection, Result as DuckResult};
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

#[derive(Debug, Deserialize)]
pub struct QueryRequest {
    pub query: String,
}

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub payment_config: Arc<GlobalPaymentConfig>,
}

#[axum::debug_handler]
#[instrument(skip_all)]
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
            let response = (
                StatusCode::BAD_REQUEST,
                [("content-type", "text/plain")],
                format!("Invalid query: {}", e),
            );
            tracing::info!("Request failed: {:?}", response);
            return response.into_response();
        }
    };

    // Create the executable DuckDB SQL query
    let duckdb_sql = match create_duckdb_query(analyzed_query.clone()) {
        Ok(sql) => sql,
        Err(e) => {
            let response = (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("content-type", "text/plain")],
                format!("Failed to create executable query: {}", e),
            );
            tracing::info!("Request failed: {:?}", response);
            return response.into_response();
        }
    };

    // Extract table name and check if payment is required 
    let table_name = &analyzed_query.body.from;
    // LOGIC IS WRONG, CHANGE PLACE
    match state.payment_config.table_requires_payment(table_name) {
        None => {
            let response = (
                StatusCode::BAD_REQUEST,
                [("content-type", "text/plain")],
                format!("Table not supported: {}", table_name),
            );
            tracing::info!("Request failed: {:?}", response);
            return response.into_response();
        },
        Some(false) => {
            // Execute query and return data
            let batches = {
                let db = state.db.lock().unwrap();
                match execute_query(&db, &duckdb_sql) {
                    Ok(batches) => batches,
                    Err(e) => {
                        let response = (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            [("content-type", "text/plain")],
                            format!("Failed to execute query: {}", e),
                        );
                        tracing::info!("Request failed: {:?}", response);
                        return response.into_response();
                    }
                }
            };
    
            let buffer = serialize_batches_to_arrow_ipc(&batches);
            
            let response = (
                StatusCode::OK,
                [("content-type", "application/vnd.apache.arrow.stream")],
                Bytes::from(buffer),
            )
                .into_response();
            tracing::info!("Request succeeded: {:?}", response);
            return response;
        }
        Some(true) => {
            // Payment required, continue to payment processing
        }
    }

    // Check if payment header is present
    let payment_header = headers.get("X-Payment");
    let payment_header = match payment_header {
        None => {
            // Phase 1: No payment header - return 402 with pricing info
            // Estimate row count
            let estimated_rows_query = create_estimate_rows_query(&duckdb_sql);
            let estimated_rows = match execute_row_count_query(&state.db, &estimated_rows_query) {
                Ok(rows) => rows,
                Err(e) => {
                    let response = (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        [("content-type", "text/plain")],
                        format!("Failed to execute row count query: {}", e),
                    );
                    tracing::info!("Request failed: {:?}", response);
                    return response.into_response();
                }
            };
            let response = match state.payment_config.create_payment_required_response(
                &format!("No crypto payment found. Implement x402 protocol (https://www.x402.org/) to pay for this API request."),
                table_name,
                estimated_rows,
                path,
            ) {
                Some(payment_response) => {
                    let response_body = serde_json::to_vec(&payment_response)
                        .expect("Failed to serialize payment response");
                    (
                        StatusCode::PAYMENT_REQUIRED,
                        [("content-type", "application/json")],
                        axum::body::Bytes::from(response_body),
                    )
                }
                None => {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        [("content-type", "text/plain")],
                        axum::body::Bytes::from("Failed to find payment options for the table request"),
                    )
                }
            };
            tracing::info!("Request failed: {:?}", response);
            return response.into_response();
        }
        Some(payment_header) => payment_header,
    };

    // Phase 2: Payment header present - verify payment and return data
    // Parse payment payload
    let base64 = x402_rs::types::Base64Bytes::from(payment_header.as_bytes());
    let payment_payload = match x402_rs::types::PaymentPayload::try_from(base64) {
        Ok(payment_payload) => payment_payload,
        Err(e) => {
            let response = (
                StatusCode::BAD_REQUEST,
                [("content-type", "text/plain")],
                format!("Failed to parse payment payload: {}", e),
            );
            tracing::info!("Request failed: {:?}", response);
            return response.into_response();
        }
    };

    // Execute query and verify row count
    let batches = {
        let db = state.db.lock().unwrap();
        match execute_query(&db, &duckdb_sql) {
            Ok(batches) => batches,
            Err(e) => {
                let response = (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [("content-type", "text/plain")],
                    format!("Failed to pre-execute query: {}", e),
                );
                tracing::info!("Request failed: {:?}", response);
                return response.into_response();
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
        let response = (
            StatusCode::INTERNAL_SERVER_ERROR,
            [("content-type", "text/plain")],
            axum::body::Bytes::from("No payment offer was found matching the provided payment payload"),
        );
        tracing::info!("Request failed: {:?}", response);
        return response.into_response();
    }

    // Verify payment for each payment requirement, this is where the actual payment is verified, but we need to interact with the facilitator.
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
                let response = (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [("content-type", "text/plain")],
                    format!("Payment verification failed due to facilitator error: {}", e),
                );
                tracing::info!("Request failed: {:?}", response);
                return response.into_response();
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
        let response = match state.payment_config.create_payment_required_response(
            &format!("Payment provided is invalid, verification failed: {}", invalid_reasons.iter().map(|reason| reason.to_string()).collect::<Vec<String>>().join(", ")),
            table_name,
            actual_rows,
            path,
        ) {
            Some(payment_response) => {
                let response_body = serde_json::to_vec(&payment_response)
                    .expect("Failed to serialize payment response");
                (
                    StatusCode::PAYMENT_REQUIRED,
                    [("content-type", "application/json")],
                    axum::body::Bytes::from(response_body),
                )
            }
            None => {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [("content-type", "text/plain")],
                    axum::body::Bytes::from("Failed to find payment options for the table request"),
                )
            }
        };
        tracing::info!("Request failed: {:?}", response);
        return response.into_response();
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
            let response = match state.payment_config.create_payment_required_response(
                &format!("Settlement of the provided payment failed: {}", e),
                table_name,
                actual_rows,
                path,
            ) {
                Some(payment_response) => {
                    let response_body = serde_json::to_vec(&payment_response)
                        .expect("Failed to serialize payment response");
                    (
                        StatusCode::PAYMENT_REQUIRED,
                        [("content-type", "application/json")],
                        axum::body::Bytes::from(response_body),
                    )
                }
                None => {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        [("content-type", "text/plain")],
                        axum::body::Bytes::from("Failed to create payment requirements"),
                    )
                }
            };
            return response.into_response();
        }
    }
    
    let buffer = serialize_batches_to_arrow_ipc(&batches);
    
    let response = (
        StatusCode::OK,
        [("content-type", "application/vnd.apache.arrow.stream")],
        Bytes::from(buffer),
    )
        .into_response();
    tracing::info!("Request succeeded: {:?}", response);
    return response;
}

pub fn execute_query(db: &Connection, query: &str) -> DuckResult<Vec<RecordBatch>> {
    let mut stmt = db.prepare(query)?;
    let batches= stmt.query_arrow([])?.collect::<Vec<RecordBatch>>();
    
    Ok(batches)
}

pub fn execute_row_count_query(db: &Arc<Mutex<Connection>>, query: &str) -> DuckResult<usize> {
    let db = db.lock().unwrap();
    let mut stmt = db.prepare(query)?;
    stmt.query_row([], |row| {
        let count: i64 = row.get(0)?;
        Ok(count as usize)
    })
} 

pub fn serialize_batches_to_arrow_ipc(batches: &Vec<RecordBatch>) -> Vec<u8> {
    let mut buffer = Vec::new();
    if let Some(first_batch) = batches.first() {
        let schema = first_batch.schema();
        let mut writer = StreamWriter::try_new(&mut buffer, &schema).unwrap();
        for batch in batches {
            writer.write(&batch).unwrap();
        }
        writer.finish().unwrap();
    }
    buffer
}