mod sqp_parser;
mod duckdb_reader;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use dotenvy::dotenv;
use opentelemetry::trace::Status;
use serde::Deserialize;
use tower_http::trace::TraceLayer;
use tracing::instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use x402_axum_middleware::{IntoPriceTag, X402Middleware};
use x402_rs::network::{Network, USDCDeployment};
use x402_rs::telemetry::Telemetry;
use std::sync::Arc;
use duckdb::{Connection, Result as DuckResult};
use arrow::record_batch::RecordBatch;
use std::sync::Mutex;
use axum::body::{to_bytes, Body, Bytes};
use arrow::ipc::writer::StreamWriter;
use axum::extract::State;
use crate::sqp_parser::analyze_query;
use crate::duckdb_reader::create_duckdb_query;
use axum::middleware::{from_fn, Next};
use axum::extract::Request;


#[derive(Debug, Deserialize)]
struct QueryRequest {
    query: String,
}

struct AppState {
    db: Arc<Mutex<Connection>>,
}

#[tokio::main]
async fn main() {
    dotenv().ok();

    let _telemetry = Telemetry::new()
        .with_name(env!("CARGO_PKG_NAME"))
        .with_version(env!("CARGO_PKG_VERSION"))
        .register();

    let x402 = X402Middleware::try_from("https://facilitator.x402.rs")
        .unwrap()
        .with_base_url(url::Url::parse("https://localhost:4021/").unwrap());
    let usdc = USDCDeployment::by_network(Network::BaseSepolia).pay_to("0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7");

    // Initialize DuckDB connection 
    let db = Connection::open("data/uni_v2_swaps.db").expect("Failed to open DuckDB connection");
    let state = Arc::new(AppState {
        db: Arc::new(Mutex::new(db)),
    });

    let app = Router::new()
        .route(
            "/query",
            post(query_handler)
                .layer(
                    x402.with_description("DuckDB Query API")
                        .with_mime_type("application/json")
                        .with_price_tag(usdc.amount(0.001).unwrap()),
                )
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

#[instrument(skip_all)]
async fn query_handler(
    State(state): State<Arc<AppState>>,
    Json(query_req): Json<QueryRequest>,
) -> impl IntoResponse {
    let db = state.db.lock().unwrap();
    println!("query_req: {:?}", query_req.query);
    let analyzed_query = analyze_query(&query_req.query).unwrap();
    let duckdb_sql = create_duckdb_query(analyzed_query).unwrap();
    println!("duckdb_sql: {:?}", duckdb_sql);

    match execute_query(&db, &duckdb_sql) {
        Ok(batches) => {
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
            println!("error: {:?}", e);
            (
            StatusCode::BAD_REQUEST,
            [("content-type", "text/plain")],
            e.to_string(),
            )
                .into_response()
        }
    }
}

fn execute_query(db: &Connection, query: &str) -> DuckResult<Vec<RecordBatch>> {
    let mut stmt = db.prepare(query)?;
    let batches= stmt.query_arrow([])?.collect::<Vec<RecordBatch>>();
    
    Ok(batches)
}