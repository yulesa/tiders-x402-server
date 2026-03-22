//! # Tiders x402 Server
//!
//! This module sets up and runs the HTTP server for the Tiders x402 payment-gated
//! data service. It uses [Axum](https://docs.rs/axum).
//!
//! ## How Axum works (brief overview)
//!
//! Axum is a routing-based web framework. You build an application by:
//! 1. Creating a [`Router`] that maps URL paths to handler functions.
//! 2. Attaching shared application state (via `.with_state(...)`) that handlers
//!    can access on every request.
//! 3. Adding middleware layers (via `.layer(...)`) that wrap every request/response
//!    — for example, logging, authentication, or tracing.
//! 4. Binding the router to a TCP listener and serving it with `axum::serve`.
//!
//! ## What this server exposes
//!
//! - `POST /query` — the main endpoint where clients submit paid data queries.
//! - `GET /` — a root endpoint that returns server metadata and available data offers.

pub mod sqp_parser;
pub mod duckdb_reader;
pub mod price;
pub mod facilitator_client;
pub mod query_handler;
pub mod root_handler;
pub mod payment_processing;
pub mod payment_config;
pub mod database;

use std::sync::{Arc, Mutex};

use axum::routing::post;
use axum::Router;
use dotenvy::dotenv;
use opentelemetry::trace::Status;
use tower_http::trace::TraceLayer;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use url::Url;
use x402_rs::util::Telemetry;
use tokio::signal;
use duckdb::Connection;

use crate::query_handler::query_handler;
use crate::root_handler::root_handler;
pub use price::{PriceTag, TablePaymentOffers};
pub use payment_config::GlobalPaymentConfig;
pub use facilitator_client::FacilitatorClient;

#[derive(Debug, Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub payment_config: Arc<GlobalPaymentConfig>,
}


/// Starts the Axum HTTP server and blocks until a shutdown signal is received.
///
/// # Arguments
/// * `state` — Shared application state (wrapped in `Arc` so it can be safely shared
///   across all request-handling tasks). Axum clones this `Arc` for each incoming
///   request and passes it to the handler.
/// * `base_url` — The URL to bind the server to (host and port are extracted from it).
pub async fn start_server(state: Arc<AppState>, base_url: Url) {

    // Load environment variables from a `.env` file if one exists.
    dotenv().ok();

    // Initialize OpenTelemetry tracing (distributed tracing / observability).
    // The `_telemetry` guard keeps the tracer alive for the lifetime of the server.
    let _telemetry = Telemetry::new()
        .with_name(env!("CARGO_PKG_NAME"))
        .with_version(env!("CARGO_PKG_VERSION"))
        .register();

    // Build the Axum Router.
    // A Router maps HTTP method + path combinations to handler functions.
    let app = Router::new()
        // Register `POST /query` → handled by `query_handler`.
        // `post(...)` is a shorthand that only matches POST requests on this path.
        .route("/query", post(query_handler))
        // Register `GET /` → handled by `root_handler`.
        .route("/", axum::routing::get(root_handler))
        // Attach shared state so handlers can access it via Axum's `State` extractor.
        // Axum "extractors" are typed parameters on handler functions that Axum
        // automatically populates from the incoming request (e.g., State, Json, Path).
        .with_state(state)
        // Add a middleware layer for HTTP request/response tracing.
        // Layers in Axum wrap the entire request pipeline — they run before the
        // handler (on the request) and after (on the response). Tower's `TraceLayer`
        // emits structured log spans for every HTTP request.
        .layer(
            TraceLayer::new_for_http()
                // `make_span_with` creates a tracing span when a request arrives.
                // This span is active for the entire request lifecycle and collects
                // metadata like method, URI, and version.
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
                // `on_response` runs after the handler has produced a response.
                // Here we record the HTTP status and latency into the tracing span,
                // and set the OpenTelemetry span status to Ok or Error accordingly.
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

    // Extract host and port from the provided URL and bind a TCP listener.
    let host = base_url.host_str().unwrap();
    let port = base_url.port().unwrap();
    let bind_addr = format!("{}:{}", host, port);

    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .expect("Can not start server");
    tracing::info!("Listening on {}", listener.local_addr().unwrap());

    // `axum::serve` takes the TCP listener and the router, and starts accepting
    // connections. Each incoming connection spawns a new Tokio task.
    // `with_graceful_shutdown` tells the server to stop accepting new connections
    // when the provided future completes (i.e., when a shutdown signal is received),
    // while still letting in-flight requests finish.
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

/// Waits for a shutdown signal (Ctrl+C or SIGTERM on Unix).
///
/// Returns when either signal is received, allowing the server to begin
/// graceful shutdown — finishing in-flight requests before exiting.
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    // On Unix systems, also listen for SIGTERM (sent by container orchestrators
    // like Docker/Kubernetes when stopping a service).
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    // On non-Unix platforms, SIGTERM doesn't exist, so we use a future that
    // never completes — effectively only Ctrl+C will trigger shutdown.
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    // `tokio::select!` races the two futures and returns as soon as either one
    // completes — whichever signal arrives first triggers the shutdown.
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("Shutdown signal received, starting graceful shutdown");
} 