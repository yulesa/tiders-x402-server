mod sqp_parser;
mod duckdb_reader;
mod price;
mod facilitator_client;
mod query_handler;
mod payment_processing;
mod payment_config;
mod database;

use x402_rs::types::{EvmAddress, MoneyAmount};
use std::str::FromStr;
use std::sync::Arc;
use duckdb::Connection;
use std::sync::Mutex;
use url::Url;

use cherry_402::facilitator_client::FacilitatorClient;
use cherry_402::query_handler::AppState;
use cherry_402::payment_config::GlobalPaymentConfig;
use cherry_402::price::{PriceTag, TablePaymentOffers};
use cherry_402::start_server;

#[tokio::main]
async fn main() {
    // Initialize facilitator client
    let facilitator = Arc::new(
        // FacilitatorClient::try_from("https://facilitator.x402.rs")
        FacilitatorClient::try_from("http://localhost:4022")
            .expect("Failed to create facilitator client")
    );

    // Initialize payment configuration
    let base_url = Url::parse("http://localhost:4021").expect("Failed to parse base URL");

    let mut global_payment_config = GlobalPaymentConfig::default(facilitator, base_url);
    
    // Create a default USDC price tag for swaps_df table
    let usdc = x402_rs::network::USDCDeployment::by_network(x402_rs::network::Network::BaseSepolia);

    let swap_price_tag = PriceTag{
        pay_to: EvmAddress::from_str("0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7").unwrap(),
        amount_per_item: MoneyAmount::from_str("0.002").unwrap().as_token_amount(usdc.decimals as u32).unwrap(),
        token: usdc.into(),
        min_total_amount: None,
        min_items: None,
        max_items: None,
        description: None,
        is_default: true
    };

    // Create table payment offer
    let swaps_offer = TablePaymentOffers::new(
        "swaps_df".to_string(),
        vec![swap_price_tag],
    );

    let swap_price_tag_2 = PriceTag{
        pay_to: EvmAddress::from_str("0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7").unwrap(),
        amount_per_item: MoneyAmount::from_str("0.001").unwrap().as_token_amount(usdc.decimals as u32).unwrap(),
        token: usdc.into(),
        min_total_amount: None,
        min_items: Some(2),
        max_items: None,
        description: None,
        is_default: false
    };

    let swaps_offer = swaps_offer.with_payment_offer(swap_price_tag_2);
    global_payment_config.add_table_offer(swaps_offer);

    // Initialize DuckDB connection 
    let db = Connection::open("data/uni_v2_swaps.db").expect("Failed to open DuckDB connection");
    let state = Arc::new(AppState {
        db: Arc::new(Mutex::new(db)),
        payment_config: Arc::new(global_payment_config),
    });

    start_server(state).await;
}