mod sqp_parser;
mod duckdb_reader;
mod price;
mod facilitator_client;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use axum::http::HeaderMap;
use dotenvy::dotenv;
use opentelemetry::trace::Status;
use serde::Deserialize;
use tower_http::trace::TraceLayer;
use tracing::instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use x402_rs::network::{Network, USDCDeployment};
use x402_rs::telemetry::Telemetry;
use x402_rs::types::{
    Base64Bytes, PaymentPayload, PaymentRequiredResponse, 
    PaymentRequirements, Scheme, SettleRequest,
    VerifyRequest, VerifyResponse, X402Version
};
use std::sync::Arc;
use duckdb::{Connection, Result as DuckResult};
use arrow::record_batch::RecordBatch;
use std::sync::Mutex;
use axum::body::{Body, Bytes};
use arrow::ipc::writer::StreamWriter;
use axum::extract::State;
use crate::facilitator_client::FacilitatorClient;
use crate::price::IntoPriceTag;
use crate::sqp_parser::analyze_query;
use crate::duckdb_reader::create_duckdb_query;
use std::collections::HashMap;
use serde_json::json;
use url::Url;
use std::fmt::Debug;

#[derive(Debug, Deserialize)]
struct QueryRequest {
    query: String,
}

struct AppState {
    db: Arc<Mutex<Connection>>,
    facilitator: Arc<FacilitatorClient>,
    table_pricing: HashMap<String, f64>, // price per row in USDC
}

#[tokio::main]
async fn main() {
    dotenv().ok();

    let _telemetry = Telemetry::new()
        .with_name(env!("CARGO_PKG_NAME"))
        .with_version(env!("CARGO_PKG_VERSION"))
        .register();

    // Initialize facilitator client
    let facilitator = Arc::new(
        // FacilitatorClient::try_from("https://facilitator.x402.rs")
        FacilitatorClient::try_from("http://localhost:4022")
            .expect("Failed to create facilitator client")
    );

    // Initialize table pricing (you can modify this as needed)
    let mut table_pricing = HashMap::new();
    table_pricing.insert("swaps_df".to_string(), 0.001); // 0.001 USDC per row
    // Add more tables as needed

    // Initialize DuckDB connection 
    let db = Connection::open("data/uni_v2_swaps.db").expect("Failed to open DuckDB connection");
    let state = Arc::new(AppState {
        db: Arc::new(Mutex::new(db)),
        facilitator,
        table_pricing,
    });

    let app = Router::new()
        .route(
            "/query",
            post(query_handler)
                .with_state(state),
        )
        .layer(
            // Usual HTTP tracing
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<_>| {
                    tracing::info_span!(
                        "http_request",
                        otel.kind = "server",
                        otel.name = %format!("{} {}", request.method(), request.uri()),
                        method = %request.method(),
                        uri = %request.uri(),
                        version = ?request.version(),
                    )
                })
                .on_response(
                    |response: &axum::http::Response<_>,
                     latency: std::time::Duration,
                     span: &tracing::Span| {
                        span.record("status", tracing::field::display(response.status()));
                        span.record("latency", tracing::field::display(latency.as_millis()));
                        span.record(
                            "http.status_code",
                            tracing::field::display(response.status().as_u16()),
                        );

                        // OpenTelemetry span status
                        if response.status().is_success() {
                            span.set_status(Status::Ok);
                        } else {
                            span.set_status(Status::error(
                                response
                                    .status()
                                    .canonical_reason()
                                    .unwrap_or("unknown")
                                    .to_string(),
                            ));
                        }

                        tracing::info!(
                            "status={} elapsed={}ms",
                            response.status().as_u16(),
                            latency.as_millis()
                        );
                    },
                ),
        );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:4021")
        .await
        .expect("Can not start server");
    tracing::info!("Listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

#[axum::debug_handler]
#[instrument(skip_all)]
async fn query_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query_req): Json<QueryRequest>,
) -> impl IntoResponse {
    // Parse and validate query first
    tracing::info!("Received query: {}", query_req.query);
    let analyzed_query = match analyze_query(&query_req.query) {
        Ok(query) => query,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                [("content-type", "text/plain")],
                format!("Invalid query: {}", e),
            )
                .into_response();
        }
    };

    // Extract table name and get pricing
    let table_name = &analyzed_query.body.from;
    let price_per_row = match state.table_pricing.get(table_name) {
        Some(price) => *price,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                [("content-type", "text/plain")],
                format!("Unknown table: {}", table_name),
            )
                .into_response();
        }
    };

    // Estimate row count
    let estimated_rows = match estimate_row_count(&state.db, &analyzed_query) {
        Ok(count) => count,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("content-type", "text/plain")],
                format!("Failed to estimate row count: {}", e),
            )
                .into_response();
        }
    };
    // Calculate total price
    let total_price = price_per_row * estimated_rows as f64;

    // Check if payment header is present
    let payment_header = headers.get("X-Payment");
    match payment_header {
        None => {
            // Phase 1: No payment header - return 402 with pricing info
            let payment_requirements = create_payment_requirements(
                total_price,
                table_name,
                estimated_rows,
                "/query",
            );
            
            let payment_required_response = PaymentRequiredResponse {
                error: "Payment required".to_string(),
                accepts: vec![payment_requirements],
                payer: None,
                x402_version: X402Version::V1,
            };

            let response_body = serde_json::to_vec(&payment_required_response)
                .expect("Failed to serialize payment response");

            (
                StatusCode::PAYMENT_REQUIRED,
                [("content-type", "application/json")],
                Bytes::from(response_body),
            )
                .into_response()
        }
        Some(payment_header) => {
            // Phase 2: Payment header present - verify payment and return data
            match verify_and_settle_payment(
                &state.facilitator,
                payment_header,
                total_price,
                table_name,
                estimated_rows,
                "/query",
            ).await {
                Ok(_) => {
                    // Payment verified, execute query and verify row count
                    let db = state.db.lock().unwrap();
                    let duckdb_sql = match create_duckdb_query(analyzed_query.clone()) {
                        Ok(sql) => sql,
                        Err(e) => {
                            return (
                                StatusCode::BAD_REQUEST,
                                [("content-type", "text/plain")],
                                format!("Failed to create DuckDB query: {}", e),
                            )
                                .into_response();
                        }
                    };

                    match execute_query(&db, &duckdb_sql) {
                        Ok(batches) => {
                            // Verify actual row count matches estimated
                            let actual_rows = batches.iter().map(|batch| batch.num_rows()).sum::<usize>();
                            if actual_rows != estimated_rows {
                                return create_payment_required_response(
                                    "Row count mismatch",
                                    total_price,
                                    table_name,
                                    estimated_rows,
                                    "/query",
                                );
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
                            
                            (
                                StatusCode::OK,
                                [("content-type", "application/vnd.apache.arrow.stream")],
                                Bytes::from(buffer),
                            )
                                .into_response()
                        }
                        Err(e) => {
                            (
                                StatusCode::BAD_REQUEST,
                                [("content-type", "text/plain")],
                                e.to_string(),
                            )
                                .into_response()
                        }
                    }
                }
                Err(e) => {
                    // Payment verification failed
                    create_payment_required_response(
                        &format!("Payment verification failed: {}", e),
                        total_price,
                        table_name,
                        estimated_rows,
                        "/query",
                    )
                }
            }
        }
    }
}

fn execute_query(db: &Connection, query: &str) -> DuckResult<Vec<RecordBatch>> {
    let mut stmt = db.prepare(query)?;
    let batches= stmt.query_arrow([])?.collect::<Vec<RecordBatch>>();
    
    Ok(batches)
}

// Helper function to estimate row count for a query
fn estimate_row_count(db: &Arc<Mutex<Connection>>, analyzed_query: &crate::sqp_parser::AnalyzedQuery) -> Result<usize, Box<dyn std::error::Error>> {
    let db = db.lock().unwrap();
    
    let duckdb_sql = create_duckdb_query(analyzed_query.clone())?;
    // wrap the query in a SELECT COUNT(*)
    let count_query = format!("SELECT COUNT(*) as num_rows FROM ({})", duckdb_sql);
    
    let mut stmt = db.prepare(&count_query)?;
    let result = stmt.query_row([], |row| {
        let count: i64 = row.get(0)?;
        Ok(count as usize)
    })?;
    
    Ok(result)
}

// Helper function to create payment requirements
fn create_payment_requirements(
    total_price: f64,
    table_name: &str,
    estimated_rows: usize,
    path: &str,
) -> PaymentRequirements {
    let usdc = USDCDeployment::by_network(Network::BaseSepolia);
    let pay_to_address = "0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7";
    
    // Create USDC amount using the builder pattern
    let price_tag = usdc.pay_to(pay_to_address).amount(total_price).unwrap();
    
    PaymentRequirements {
        scheme: Scheme::Exact,
        network:  price_tag.token.network(),
        max_amount_required: price_tag.amount,
        resource: Url::parse(&format!("http://localhost:4021{}", path)).unwrap(),
        description: format!("Query on table '{}' returning {} rows", table_name, estimated_rows),
        mime_type: "application/vnd.apache.arrow.stream".to_string(),
        pay_to: price_tag.pay_to.into(),
        max_timeout_seconds: 300,
        asset: price_tag.token.asset.address.into(),
        extra: Some(json!({
            "name": price_tag.token.eip712.name,
            "version": price_tag.token.eip712.version
        })),
        output_schema: None,
    }
}

// Helper function to verify and settle payment
async fn verify_and_settle_payment(
    facilitator: &Arc<FacilitatorClient>,
    payment_header: &axum::http::HeaderValue,
    expected_price: f64,
    table_name: &str,
    estimated_rows: usize,
    path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Parse payment payload
    let base64 = Base64Bytes::from(payment_header.as_bytes());
    let payment_payload = PaymentPayload::try_from(base64)?;
    
    // Create payment requirements for verification
    let payment_requirements = create_payment_requirements(
        expected_price,
        table_name,
        estimated_rows,
        path,
    );
        
    // Verify payment
    let verify_request = VerifyRequest {
        x402_version: payment_payload.x402_version,
        payment_payload,
        payment_requirements,
    };
    let verify_response = facilitator.verify(&verify_request).await?;
    match verify_response {
        VerifyResponse::Valid { .. } => {
            // Settle payment
            let settle_request = SettleRequest {
                x402_version: verify_request.x402_version,
                payment_payload: verify_request.payment_payload,
                payment_requirements: verify_request.payment_requirements,
            };
            
            let settle_response = facilitator.settle(&settle_request).await?;
            
            if settle_response.success {
                Ok(())
            } else {
                Err("Payment settlement failed".into())
            }
        }
        VerifyResponse::Invalid { reason, .. } => {
            Err(format!("Payment verification failed: {}", reason).into())
        }
    }
}

// Helper function to create payment required response
fn create_payment_required_response(
    error: &str,
    total_price: f64,
    table_name: &str,
    estimated_rows: usize,
    path: &str,
) -> Response {
    let payment_requirements = create_payment_requirements(
        total_price,
        table_name,
        estimated_rows,
        path,
    );
    
    let payment_required_response = PaymentRequiredResponse {
        error: error.to_string(),
        accepts: vec![payment_requirements],
        payer: None,
        x402_version: X402Version::V1,
    };

    let response_body = serde_json::to_vec(&payment_required_response)
        .expect("Failed to serialize payment response");

    Response::builder()
        .status(StatusCode::PAYMENT_REQUIRED)
        .header("content-type", "application/json")
        .body(Body::from(response_body))
        .expect("Failed to construct response")
}