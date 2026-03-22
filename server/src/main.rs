use x402_rs::chain::eip155::{ChecksummedAddress, TokenAmount};
use x402_rs::networks::{KnownNetworkEip155, USDC};
use std::str::FromStr;
use std::sync::Arc;
use duckdb::Connection;
use std::sync::Mutex;
use url::Url;

use tiders_x402::facilitator_client::FacilitatorClient;
use tiders_x402::payment_config::GlobalPaymentConfig;
use tiders_x402::price::{PriceTag, TablePaymentOffers};
use tiders_x402::{AppState, start_server};
use tiders_x402::duckdb_reader::get_duckdb_table_schema;

#[tokio::main]
async fn main() {
    // Initialize facilitator client
    let facilitator = Arc::new(
        FacilitatorClient::try_from("https://facilitator.x402.rs")
        // FacilitatorClient::try_from("http://localhost:4022")
            .expect("Failed to create facilitator client")
    );

    // Initialize payment configuration
    let base_url = Url::parse("http://0.0.0.0:4021").expect("Failed to parse base URL");
    // let base_url = Url::parse("http://localhost:4021").expect("Failed to parse base URL");


    let db = Connection::open("data/uni_v2_swaps.db").expect("Failed to open DuckDB connection");

    let mut global_payment_config = GlobalPaymentConfig::default(facilitator, base_url.clone());

    // Create a default USDC price tag for swaps_df table
    let usdc = USDC::base_sepolia();

    let swap_price_tag = PriceTag{
        pay_to: ChecksummedAddress::from_str("0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7").unwrap(),
        amount_per_item: TokenAmount(usdc.parse("0.002").unwrap().amount),
        token: usdc.clone(),
        min_total_amount: None,
        min_items: None,
        max_items: None,
        description: None,
        is_default: true
    };


    // Create table payment offer
    let swaps_schema = get_duckdb_table_schema(&db, "swaps_df").unwrap();
    let swaps_offer = TablePaymentOffers::new(
        "swaps_df".to_string(),
        vec![swap_price_tag],
        Some(swaps_schema),
    ).with_description("Uniswap v2 swaps".to_string());


    let swap_price_tag_2 = PriceTag{
        pay_to: ChecksummedAddress::from_str("0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7").unwrap(),
        amount_per_item: TokenAmount(usdc.parse("0.001").unwrap().amount),
        token: usdc.clone(),
        min_total_amount: None,
        min_items: Some(2),
        max_items: None,
        description: None,
        is_default: false
    };

    let swaps_offer = swaps_offer.with_payment_offer(swap_price_tag_2);
    global_payment_config.add_table_offer(swaps_offer);

    let state = Arc::new(AppState {
        db: Arc::new(Mutex::new(db)),
        payment_config: Arc::new(global_payment_config),
    });

    start_server(state, base_url).await;
}
