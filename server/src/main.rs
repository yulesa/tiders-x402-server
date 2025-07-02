mod sqp_parser;
mod duckdb_reader;
mod price;
mod facilitator_client;
mod query_handler;
mod payment_processing;

use axum::routing::post;
use axum::Router;
use dotenvy::dotenv;
use opentelemetry::trace::Status;
use tower_http::trace::TraceLayer;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use x402_rs::telemetry::Telemetry;
use std::sync::Arc;
use duckdb::Connection;
use std::sync::Mutex;
use std::collections::HashMap;

use crate::facilitator_client::FacilitatorClient;
use crate::query_handler::{AppState, query_handler};

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
                            "status={}, latency={}ms",
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

