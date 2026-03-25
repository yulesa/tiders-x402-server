use x402_chain_eip155::chain::ChecksummedAddress;
use x402_chain_eip155::KnownNetworkEip155;
use x402_types::networks::USDC;
use std::str::FromStr;
use std::sync::Arc;
use duckdb::Connection;
use url::Url;

use tiders_x402::facilitator_client::FacilitatorClient;
use tiders_x402::payment_config::GlobalPaymentConfig;
use tiders_x402::price::{PriceTag, TablePaymentOffers, TokenAmount};
use tiders_x402::{AppState, Database, start_server};
use tiders_x402::database_duckdb::DuckDbDatabase;

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


    let conn = Connection::open("../../data/duckdb.db").expect("Failed to open DuckDB connection");

    let mut global_payment_config = GlobalPaymentConfig::default(facilitator, base_url.clone());

    // Create a default USDC price tag for uniswap_v3_pool_swap table
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

    let db = DuckDbDatabase::new(conn);

    // Create table payment offer
    let swaps_schema = db.get_table_schema("uniswap_v3_pool_swap")
        .await
        .expect("Failed to get table schema");

    let swaps_offer = TablePaymentOffers::new(
        "uniswap_v3_pool_swap".to_string(),
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
    global_payment_config.add_offers_table(swaps_offer);

    let state = Arc::new(AppState {
        db: Arc::new(db),
        payment_config: Arc::new(global_payment_config),
    });

    start_server(state, base_url).await;
}
