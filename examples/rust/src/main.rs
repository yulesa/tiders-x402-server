use x402_chain_eip155::chain::ChecksummedAddress;
use x402_chain_eip155::KnownNetworkEip155;
use x402_types::networks::USDC;
use std::str::FromStr;
use std::sync::Arc;
use url::Url;

use tiders_x402::facilitator_client::FacilitatorClient;
use tiders_x402::payment_config::GlobalPaymentConfig;
use tiders_x402::price::{PriceTag, TablePaymentOffers, TokenAmount};
use tiders_x402::{AppState, Database, start_server};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    // Initialize facilitator client
    let facilitator = Arc::new(
        FacilitatorClient::try_from("https://facilitator.x402.rs")
        // FacilitatorClient::try_from("http://localhost:4022")
            .expect("Failed to create facilitator client")
    );

    // Initialize payment configuration
    let base_url = Url::parse("http://0.0.0.0:4021").expect("Failed to parse base URL");
    // let base_url = Url::parse("http://localhost:4021").expect("Failed to parse base URL");





    // ########## If you're using a Duckdb database: ##########

    // let conn = duckdb::Connection::open("../data/duckdb.db").expect("Failed to open DuckDB connection");
    // let db = tiders_x402::database_duckdb::DuckDbDatabase::new(conn);

    // ########## If you're using a Postgresql database: ##########

    // let pg_user = std::env::var("POSTGRES_USER").expect("POSTGRES_USER not set");
    // let pg_password = std::env::var("POSTGRES_PASSWORD").expect("POSTGRES_PASSWORD not set");
    // let pg_db = std::env::var("POSTGRES_DB").expect("POSTGRES_DB not set");
    // let pg_host = std::env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".to_string());
    // let pg_port = std::env::var("POSTGRES_PORT").unwrap_or_else(|_| "5432".to_string());
    // let conn_str = format!(
    //     "host={} port={} user={} password={} dbname={}",
    //     pg_host, pg_port, pg_user, pg_password, pg_db
    // );

    // --- Option A: Connect via connection string ---
    // let db = tiders_x402::database_postgresql::PostgresqlDatabase::from_connection_string(&conn_str)
    //     .await
    //     .expect("Failed to connect to PostgreSQL");

    // --- Option B: Connect via a user-managed pool ---
    // let pg_config: tokio_postgres::Config = conn_str
    //     .parse()
    //     .expect("Failed to parse Postgres config");
    
    // let mgr = deadpool_postgres::Manager::from_config(
    //     pg_config,
    //     tokio_postgres::NoTls,
    //     deadpool_postgres::ManagerConfig {
    //         recycling_method: deadpool_postgres::RecyclingMethod::Fast,
    //     },
    // );
    // let pool = deadpool_postgres::Pool::builder(mgr)
    //     .max_size(16)
    //     .build()
    //     .expect("Failed to build pool");
    
    // let db = tiders_x402::database_postgresql::PostgresqlDatabase::from_pool(pool);

    // ########## If you're using a ClickHouse database: ##########

    let ch_host = std::env::var("CLICKHOUSE_HOST").unwrap_or_else(|_| "localhost".to_string());
    let ch_port = std::env::var("CLICKHOUSE_PORT").unwrap_or_else(|_| "8123".to_string());
    let ch_user = std::env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "default".to_string());
    let ch_password = std::env::var("CLICKHOUSE_PASSWORD").unwrap_or_else(|_| "default".to_string());
    let ch_database = std::env::var("CLICKHOUSE_DATABASE").unwrap_or_else(|_| "default".to_string());
    let ch_secure = std::env::var("CLICKHOUSE_SECURE").unwrap_or_else(|_| "false".to_string());
    let ch_scheme = if ch_secure == "true" { "https" } else { "http" };
    let ch_url = format!("{}://{}:{}", ch_scheme, ch_host, ch_port);

    // --- Option A: Connect via URL ---
    // let db = tiders_x402::database_clickhouse::ClickHouseDatabase::from_url(&ch_url)
    //     .expect("Failed to create ClickHouse client");

    // --- Option B: Connect via a user-managed client ---
    let client = clickhouse::Client::default()
        .with_url(&ch_url)
        .with_user(&ch_user)
        .with_password(&ch_password)
        .with_database(&ch_database);

    let db = tiders_x402::database_clickhouse::ClickHouseDatabase::from_client(client);



    
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
