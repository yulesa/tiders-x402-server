//! Axum handler for the `GET /api/` endpoint.
//!
//! Returns a JSON overview of the server: identity, available endpoints,
//! and per-table payment summaries. Intended as a machine-readable discovery
//! document — pipe it through `jq` or open it in a browser JSON viewer.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::response::IntoResponse;
use serde::Serialize;

use crate::AppState;
use crate::payment::price::{PriceTag, PricingModel};

#[derive(Serialize)]
struct ApiRootResponse {
    server: ServerInfo,
    /// All registered API endpoints and what they do.
    endpoints: BTreeMap<String, EndpointInfo>,
    /// One entry per configured table.
    tables: Vec<TableSummary>,
}

#[derive(Serialize)]
struct EndpointInfo {
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<String>,
}

#[derive(Serialize)]
struct ServerInfo {
    /// Public base URL of this server.
    url: String,
    /// Crate version from Cargo.toml.
    version: &'static str,
    /// x402 facilitator used to verify and settle payments.
    facilitator_url: String,
}

#[derive(Serialize)]
struct TableSummary {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    requires_payment: bool,
    /// Link to the full schema and pricing details for this table.
    details: String,
    /// Pricing tiers for this table. Empty when the table is free.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pricing: Vec<PriceSummary>,
}

/// Human-readable pricing tier. Token amounts are in the token's smallest unit.
#[derive(Serialize)]
#[serde(tag = "model")]
enum PriceSummary {
    PerRow {
        amount_per_item: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        min_items: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_items: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        min_total_amount: Option<String>,
        token_address: String,
        chain: String,
        pay_to: String,
    },
    Fixed {
        amount: String,
        token_address: String,
        chain: String,
        pay_to: String,
    },
}

/// Handles `GET /api/` — returns a JSON discovery document.
#[axum::debug_handler]
pub async fn api_root_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let payment_config = state.payment_config.read().await.clone();

    let mut endpoints = BTreeMap::new();
    endpoints.insert(
        "GET /api/".to_string(),
        EndpointInfo { description: "This document.".to_string(), response_format: None },
    );
    endpoints.insert(
        "GET /api/table/{name}".to_string(),
        EndpointInfo {
            description: "Full schema and pricing details for a specific table.".to_string(),
            response_format: Some("application/json".to_string()),
        },
    );
    endpoints.insert(
        "POST /api/query".to_string(),
        EndpointInfo {
            description: "Submit a SELECT query (JSON body: {\"query\": \"SELECT …\"}). \
                          Paid tables respond with HTTP 402 — use an x402 client library \
                          (https://github.com/x402-foundation/x402) to handle payment automatically."
                .to_string(),
            response_format: Some(payment_config.mime_type.clone()),
        },
    );

    let mut tables: Vec<TableSummary> = payment_config
        .offers_tables
        .values()
        .map(|offer| {
            let pricing = offer
                .price_tags
                .iter()
                .filter_map(price_summary)
                .collect();

            TableSummary {
                name: offer.table_name.clone(),
                description: offer.description.clone(),
                requires_payment: offer.requires_payment,
                details: format!("/api/table/{}", offer.table_name),
                pricing,
            }
        })
        .collect();
    tables.sort_by(|a, b| a.name.cmp(&b.name));

    Json(ApiRootResponse {
        server: ServerInfo {
            url: state.server_base_url.to_string(),
            version: env!("CARGO_PKG_VERSION"),
            facilitator_url: payment_config.facilitator.base_url().to_string(),
        },
        endpoints,
        tables,
    })
}

fn price_summary(tag: &PriceTag) -> Option<PriceSummary> {
    let token_address = tag.token.address.to_string();
    let chain = tag.token.chain_reference.to_string();
    let pay_to = tag.pay_to.to_string();

    match &tag.pricing {
        PricingModel::PerRow {
            amount_per_item,
            min_items,
            max_items,
            min_total_amount,
        } => Some(PriceSummary::PerRow {
            amount_per_item: amount_per_item.0.to_string(),
            min_items: *min_items,
            max_items: *max_items,
            min_total_amount: min_total_amount.as_ref().map(|a| a.0.to_string()),
            token_address,
            chain,
            pay_to,
        }),
        PricingModel::Fixed { amount } => Some(PriceSummary::Fixed {
            amount: amount.0.to_string(),
            token_address,
            chain,
            pay_to,
        }),
        PricingModel::MetadataPrice { .. } => None,
    }
}
