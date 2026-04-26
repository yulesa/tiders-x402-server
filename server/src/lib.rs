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

#[cfg(feature = "cli")]
pub mod cli;
pub mod database;
#[cfg(feature = "clickhouse")]
pub mod database_clickhouse;
#[cfg(feature = "duckdb")]
pub mod database_duckdb;
#[cfg(feature = "postgresql")]
pub mod database_postgresql;
pub mod facilitator_client;
pub mod payment_config;
pub mod payment_processing;
pub mod price;
pub mod query_handler;
pub mod root_handler;
#[cfg(feature = "clickhouse")]
pub mod sql_clickhouse;
#[cfg(feature = "duckdb")]
pub mod sql_duckdb;
#[cfg(feature = "postgresql")]
pub mod sql_postgresql;
pub mod sql_shared;
pub mod sqp_parser;
pub mod table_detail_handler;

use std::sync::Arc;

use axum::Router;
use axum::routing::post;
use dotenvy::dotenv;
use opentelemetry::trace::{Status, TracerProvider};
use tokio::signal;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use url::Url;

use crate::query_handler::query_handler;
use crate::root_handler::root_handler;
use crate::table_detail_handler::table_detail_handler;
pub use database::Database;
pub use facilitator_client::FacilitatorClient;
pub use payment_config::GlobalPaymentConfig;
pub use price::{PriceTag, PricingModel, TablePaymentOffers};

/// Shared application state accessible by every request handler.
///
/// Holds the database connection and the global payment configuration.
/// Axum clones the wrapping `Arc` for each incoming request, so all
/// handlers share the same underlying state.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Database backend (DuckDB, Postgres, ClickHouse, etc.) behind a trait object.
    pub db: Arc<dyn Database>,
    /// Global payment configuration: offer's tables, pricing rules, and facilitator settings.
    /// Wrapped in `RwLock` to support hot-reloading the configuration at runtime.
    pub payment_config: Arc<tokio::sync::RwLock<Arc<GlobalPaymentConfig>>>,
    /// The server's public URL, used for building resource URLs in payment requirements
    /// (e.g. "https://api.tiders.com"). This is the URL the x402 facilitator uses
    /// for payment verification callbacks.
    pub server_base_url: Url,
    /// The address and port the server binds to (e.g. "0.0.0.0:4021").
    pub server_bind_address: String,
}

impl AppState {
    /// Creates a new `AppState`.
    ///
    /// Accepts either a concrete `impl Database` or a pre-wrapped
    /// `Arc<dyn Database>` — all other wrapping is handled internally.
    pub fn new(
        db: impl Into<Arc<dyn Database>>,
        payment_config: GlobalPaymentConfig,
        server_base_url: Url,
        server_bind_address: String,
    ) -> Self {
        Self {
            db: db.into(),
            payment_config: Arc::new(tokio::sync::RwLock::new(Arc::new(payment_config))),
            server_base_url,
            server_bind_address,
        }
    }
}

/// Starts the Axum HTTP server and blocks until a shutdown signal is received.
///
/// # Arguments
/// * `state` — Application state. Wrapped in `Arc` internally so it can be
///   safely shared across all request-handling tasks. The server binds to
///   `state.server_bind_address`.
pub async fn start_server(state: AppState) {
    let state = Arc::new(state);
    // Load environment variables from a `.env` file if one exists.
    dotenv().ok();

    // Initialize tracing subscriber for structured logging.
    // If OTEL_EXPORTER_OTLP_ENDPOINT is set, also export spans via OTLP (gRPC).
    let env_filter =
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    let fmt_layer = tracing_subscriber::fmt::layer();

    if std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok() {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .build()
            .expect("failed to build OTLP span exporter");

        let service_name =
            std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "tiders-x402".to_string());
        let resource = opentelemetry_sdk::Resource::builder()
            .with_attribute(opentelemetry::KeyValue::new("service.name", service_name))
            .build();

        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_resource(resource)
            .with_batch_exporter(exporter)
            .build();

        opentelemetry::global::set_tracer_provider(provider.clone());
        let tracer = provider.tracer("tiders-x402");
        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        let _ = tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .with(otel_layer)
            .try_init();

        tracing::info!("OTLP tracing exporter enabled");
    } else {
        let _ = tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .try_init();
    };

    let bind_addr = state.server_bind_address.clone();

    // Build the Axum Router.
    // A Router maps HTTP method + path combinations to handler functions.
    let app = Router::new()
        // Register `POST /query` → handled by `query_handler`.
        // `post(...)` is a shorthand that only matches POST requests on this path.
        .route("/query", post(query_handler))
        // Register `GET /` → handled by `root_handler`.
        .route("/", axum::routing::get(root_handler))
        .route("/table/{name}", axum::routing::get(table_detail_handler))
        // Serve the Evidence dashboard static build under `/dashboard/*`.
        // Path is relative to the server's working directory.
        .nest_service("/dashboard", ServeDir::new("dashboard/build"))
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

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .unwrap_or_else(|e| {
            panic!(
                "Failed to bind to '{}': {}. server_bind_address must be in host:port format (e.g. \"0.0.0.0:4021\")",
                bind_addr, e
            )
        });
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

    // The OTLP tracer provider flushes pending spans on Drop.
}

/// Waits for a shutdown signal (Ctrl+C or SIGTERM on Unix).
///
/// Returns when either signal is received, allowing the server to begin
/// graceful shutdown — finishing in-flight requests before exiting.
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
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
