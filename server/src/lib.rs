pub mod sqp_parser;
pub mod duckdb_reader;
pub mod price;
pub mod facilitator_client;
pub mod query_handler;
pub mod payment_processing;
pub mod payment_config;
pub mod database;

use std::sync::Arc;

pub use price::{PriceTag, TablePaymentOffers};
pub use payment_config::GlobalPaymentConfig;
pub use facilitator_client::FacilitatorClient;

// Re-export the start_server function from main.rs
pub use crate::query_handler::AppState;

// We need to create a separate function for the server startup since main.rs is not a module
pub async fn start_server(state: Arc<AppState>) {
    use axum::routing::post;
    use axum::Router;
    use dotenvy::dotenv;
    use opentelemetry::trace::Status;
    use tower_http::trace::TraceLayer;
    use tracing_opentelemetry::OpenTelemetrySpanExt;
    use x402_rs::telemetry::Telemetry;
    use crate::query_handler::query_handler;

    dotenv().ok();

    let _telemetry = Telemetry::new()
        .with_name(env!("CARGO_PKG_NAME"))
        .with_version(env!("CARGO_PKG_VERSION"))
        .register();
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