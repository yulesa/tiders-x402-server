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
//! - `GET /` — landing page listing enabled dashboards.
//! - `GET /api/` — server metadata and available data offers.
//! - `GET /api/query` — the main endpoint where clients submit paid data queries.
//! - `GET /api/table/{name}` — schema and pricing for a single table.
//! - `GET /<dashboard>/` — static Evidence dashboard, one per `dashboards:` entry.

#[cfg(feature = "cli")]
pub mod cli;
pub mod dashboard;
pub mod database;
pub mod handler_api_query;
pub mod handler_api_root;
pub mod handler_api_table_detail;
pub mod payment;

use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::Router;
use axum::routing::get;
use dotenvy::dotenv;
use opentelemetry::trace::{Status, TracerProvider};
use tokio::signal;
use tower_http::trace::TraceLayer;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use url::Url;

use crate::dashboard::{DashboardSwap, DashboardsState, build_dashboard_router, landing_handler};
use crate::handler_api_query::query_handler;
use crate::handler_api_root::api_root_handler;
use crate::handler_api_table_detail::table_detail_handler;
pub use database::Database;
pub use payment::config::GlobalPaymentConfig;
pub use payment::facilitator_client::FacilitatorClient;
pub use payment::price::{PriceTag, PricingModel, TablePaymentOffers};

/// Shared application state accessible by every request handler.
///
/// Holds the database connection, the payment configuration, the bind/base
/// addresses, and the dashboards state (with its lock-free swappable router).
/// Axum clones the wrapping `Arc` for each incoming request, so all handlers
/// share the same underlying state.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Database backend (DuckDB, Postgres, ClickHouse, etc.) behind a trait object.
    pub db: Arc<dyn Database>,
    /// Global payment configuration: registered tables, pricing rules, and facilitator settings.
    /// Wrapped in `RwLock` so the file watcher can swap it at runtime without dropping requests.
    pub payment_config: Arc<tokio::sync::RwLock<Arc<GlobalPaymentConfig>>>,
    /// The server's public URL, used for building resource URLs in payment requirements
    /// (e.g. <https://api.tiders.com>). This is the URL the x402 facilitator uses
    /// for payment verification callbacks.
    pub server_base_url: Url,
    /// The address and port the server binds to (e.g. "0.0.0.0:4021").
    pub server_bind_address: String,
    /// Dashboards state (root path + list), swappable at runtime so the file
    /// watcher can rebuild the list without a restart.
    pub dashboards: Arc<ArcSwap<DashboardsState>>,
    /// Currently mounted dashboard sub-router. Replaced atomically when the
    /// config watcher reloads the `dashboards:` block.
    pub dashboard_router: Arc<ArcSwap<Router>>,
}

impl AppState {
    /// Creates a new `AppState`.
    ///
    /// Accepts either a concrete `impl Database` or a pre-wrapped
    /// `Arc<dyn Database>` — all other wrapping (`RwLock`, `ArcSwap`) is
    /// handled internally. The dashboard router is built eagerly from
    /// `dashboards_state`; pass an empty [`DashboardsState`] for an
    /// API-only deployment.
    pub fn new(
        db: impl Into<Arc<dyn Database>>,
        payment_config: GlobalPaymentConfig,
        server_base_url: Url,
        server_bind_address: String,
        dashboards_state: DashboardsState,
    ) -> Self {
        let dashboard_router = build_dashboard_router(&dashboards_state.dashboards);
        Self {
            db: db.into(),
            payment_config: Arc::new(tokio::sync::RwLock::new(Arc::new(payment_config))),
            server_base_url,
            server_bind_address,
            dashboards: Arc::new(ArcSwap::from_pointee(dashboards_state)),
            dashboard_router: Arc::new(ArcSwap::from_pointee(dashboard_router)),
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
    //
    // Layout:
    //   GET  /                  → landing_handler (only mounted when dashboards exist)
    //   GET  /api/              → discovery document
    //   GET  /api/query         → SQL query endpoint (SQL passed as `?query=...`)
    //   GET  /api/table/{name}  → table metadata
    //   *                       → DashboardSwap fallback (serves /<slug>/... for each dashboard)
    //
    // The dashboard sub-router lives behind a `DashboardSwap` fallback so the
    // file watcher can replace it atomically (`arc-swap`) on config reload
    // without taking a lock or dropping in-flight requests.
    let api_router = Router::new()
        .route("/", get(api_root_handler))
        .route("/query", get(query_handler))
        .route("/table/{name}", get(table_detail_handler));

    let dashboards_service = DashboardSwap(state.dashboard_router.clone());
    let has_dashboards = !state.dashboards.load().dashboards.is_empty();

    let app = {
        let base = Router::new().nest("/api", api_router);
        if has_dashboards {
            base.route("/", get(landing_handler))
        } else {
            base
        }
    }
    .fallback_service(dashboards_service)
    .with_state(state)
    .layer(
        TraceLayer::new_for_http()
            .make_span_with(|request: &axum::http::Request<_>| {
                let is_query = request.uri().path() == "/api/query"
                    && request.method() == axum::http::Method::GET;
                if is_query {
                    tracing::info_span!(
                        "api_query",
                        otel.kind = "server",
                        otel.name = %format!("{} {}", request.method(), request.uri()),
                        method = %request.method(),
                        uri = %request.uri(),
                        version = ?request.version(),
                    )
                } else {
                    tracing::debug_span!(
                        "http_request",
                        otel.kind = "server",
                        otel.name = %format!("{} {}", request.method(), request.uri()),
                        method = %request.method(),
                        uri = %request.uri(),
                        version = ?request.version(),
                    )
                }
            })
            .on_response(
                |response: &axum::http::Response<_>,
                 latency: std::time::Duration,
                 span: &tracing::Span| {
                    let status = response.status();
                    let is_query_span = span
                        .metadata()
                        .map(|m| m.name() == "api_query")
                        .unwrap_or(false);

                    span.record("status", tracing::field::display(status));
                    span.record("latency", tracing::field::display(latency.as_millis()));
                    span.record("http.status_code", tracing::field::display(status.as_u16()));

                    if status.is_client_error() || status.is_server_error() {
                        span.set_status(Status::error(
                            status.canonical_reason().unwrap_or("unknown").to_string(),
                        ));
                    } else {
                        span.set_status(Status::Ok);
                    }

                    if is_query_span {
                        tracing::info!(
                            "status={}, latency={}ms",
                            status.as_u16(),
                            latency.as_millis()
                        );
                    } else {
                        tracing::debug!(
                            "status={}, latency={}ms",
                            status.as_u16(),
                            latency.as_millis()
                        );
                    }
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
