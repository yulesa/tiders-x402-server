mod sqp_parser;
mod duckdb_reader;
mod price;
mod facilitator_client;
mod query_handler;
mod payment_processing;
mod payment_config;

use axum::routing::post;
use axum::Router;
use dotenvy::dotenv;
use opentelemetry::trace::Status;
use tower_http::trace::TraceLayer;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use x402_rs::telemetry::Telemetry;
use x402_rs::types::{EvmAddress, MoneyAmount};
use std::str::FromStr;
use std::sync::Arc;
use duckdb::Connection;
use std::sync::Mutex;
use url::Url;
use std::collections::HashMap;

use crate::facilitator_client::FacilitatorClient;
use crate::query_handler::{AppState, query_handler};
use crate::payment_config::{GlobalPaymentConfig, TablePaymentOffers};
use crate::price::PriceTag;

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

    // Initialize payment configuration
    let base_url = Url::parse("http://localhost:4021").expect("Failed to parse base URL");

    let mut global_payment_config = GlobalPaymentConfig{
        facilitator: facilitator.clone(),
        base_url: base_url,
        mime_type: "application/json".to_string(),
        max_timeout_seconds: 3600,
        default_description: "Default description".to_string(),
        table_offers: HashMap::new(),
    };

    
    // Create a default USDC price tag for swaps_df table
    let usdc = x402_rs::network::USDCDeployment::by_network(x402_rs::network::Network::BaseSepolia);

    let swap_price_tag = PriceTag{
        pay_to: EvmAddress::from_str("0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7").unwrap(),
        amount_per_item: MoneyAmount::from_str("0.001").unwrap().as_token_amount(usdc.decimals as u32).unwrap(),
        token: usdc.into(),
        min_total_amount: None,
        min_rows: None,
        max_rows: None,
        description: None,
        is_default: true
    };

    let swap_price_tag_2 = PriceTag{
        pay_to: EvmAddress::from_str("0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7").unwrap(),
        amount_per_item: MoneyAmount::from_str("0.002").unwrap().as_token_amount(usdc.decimals as u32).unwrap(),
        token: usdc.into(),
        min_total_amount: None,
        min_rows: Some(2),
        max_rows: None,
        description: None,
        is_default: false
    };

    // Create table payment offer
    let swaps_offer = TablePaymentOffers::new(
        "swaps_df".to_string(),
        vec![swap_price_tag, swap_price_tag_2],
    );

    global_payment_config.add_table_offer(swaps_offer);

    // Initialize DuckDB connection 
    let db = Connection::open("data/uni_v2_swaps.db").expect("Failed to open DuckDB connection");
    let state = Arc::new(AppState {
        db: Arc::new(Mutex::new(db)),
        payment_config: Arc::new(global_payment_config),
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

