use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use axum::body::Bytes;
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use duckdb::{Connection, Result as DuckResult};
use x402_rs::types::VerifyRequest;
use std::sync::{Arc, Mutex};
use serde::Deserialize;
use tracing::instrument;

use crate::facilitator_client::FacilitatorClient;
use crate::sqp_parser::{analyze_query, create_estimate_rows_query};
use crate::duckdb_reader::create_duckdb_query;
use crate::payment_processing::{
    create_payment_required_response, create_payment_requirements, settle_payment, verify_payment
};

#[derive(Debug, Deserialize)]
pub struct QueryRequest {
    pub query: String,
}

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub facilitator: Arc<FacilitatorClient>,
    pub table_pricing: std::collections::HashMap<String, f64>, // price per row in USDC
}

#[axum::debug_handler]
#[instrument(skip_all)]
pub async fn query_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query_req): Json<QueryRequest>,
) -> impl IntoResponse {
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

    // Extract table name and get pricing
    let table_name = &analyzed_query.body.from;
    let price_per_row = match state.table_pricing.get(table_name) {
        Some(price) => *price,
        None => {
            let response = (
                StatusCode::BAD_REQUEST,
                [("content-type", "text/plain")],
                format!("Table not supported: {}", table_name),
            )
            .into_response();
            tracing::info!("Request failed: {:?}", response);
            return response;
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
            )
            .into_response();
            tracing::info!("Request failed: {:?}", response);
            return response;
        }
    };

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
                    )
                    .into_response();
                    tracing::info!("Request failed: {:?}", response);
                    return response;
                }
            };
            // Calculate total price
            let total_price = price_per_row * estimated_rows as f64;

            let response = create_payment_required_response(
                &format!("No crypto payment found. Implement x402 protocol (https://www.x402.org/) to pay for this API request."),
                total_price,
                table_name,
                estimated_rows,
                "/query",
            );
            tracing::info!("Request failed: {:?}", response);
            return response;
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
            )
            .into_response();
            tracing::info!("Request failed: {:?}", response);
            return response;
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
                )
                .into_response();
                tracing::info!("Request failed: {:?}", response);
                return response;
            }
        }
    };
    
    // Verify actual row count matches payment
    let actual_rows = batches.iter().map(|batch| batch.num_rows()).sum::<usize>();
    let actual_price = price_per_row * actual_rows as f64;

    // Create payment requirements for verification
    let payment_requirements = create_payment_requirements(
        actual_price,
        table_name,
        actual_rows,
        "/query",
    );
        
    // Verify payment
    let verify_request = VerifyRequest {
        x402_version: payment_payload.x402_version,
        payment_payload,
        payment_requirements,
    };

    let verify_response = match verify_payment(
        &state.facilitator,
        &verify_request,
    ).await {
        Ok(result) => result,
        //An Error here doesn't mean the payment is invalid, it means the connection to the facilitator, or the facilitator is having issues
        Err(e) => {
            let response = (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("content-type", "text/plain")],
                format!("Payment verification failed due to facilitator error: {}", e),
            )
            .into_response();
            tracing::info!("Request failed: {:?}", response);
            return response;
        }
    };
    
    // Verify the payment is valid based on the facilitator's response
    match verify_response {
        x402_rs::types::VerifyResponse::Valid { .. } => {
            // Payment verified, proceed to settlement
        }
        x402_rs::types::VerifyResponse::Invalid { reason, .. } => {
            return create_payment_required_response(
                &format!("Payment provided is invalid, verification failed: {}", reason),
                actual_price,
                table_name,
                actual_rows,
                "/query",
            );
        }
    }

    // Settle payment
    match settle_payment(
        verify_response,
        &state.facilitator,
        verify_request,
    ).await {
        Ok(_) => {
            // Payment settled, execute query and verify row count
        }
        Err(e) => {
            // Payment settlement failed
            return create_payment_required_response(
                &format!("Settlement of the provided payment failed: {}", e),
                actual_price,
                table_name,
                actual_rows,
                "/query",
            );
        }
    }
    
    // Serialize batches to Arrow IPC
    let mut buffer = Vec::new();
    if let Some(first_batch) = batches.first() {
        let schema = first_batch.schema();
        let mut writer = StreamWriter::try_new(&mut buffer, &schema).unwrap();
        for batch in batches {
            writer.write(&batch).unwrap();
        }
        writer.finish().unwrap();
    }
    
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