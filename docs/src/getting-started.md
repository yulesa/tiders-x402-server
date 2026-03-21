# Getting Started

## Prerequisites

- **Rust** (stable, 2021 edition)
- A DuckDB database file with your data
- A crypto wallet address to receive payments
- Access to an x402 facilitator service (e.g., `https://facilitator.x402.rs`)

## Building

```bash
cargo build --release
```

The server binary will be at `target/release/tiders-x402-server`.

## Quick Start (Rust)

```rust
use std::sync::{Arc, Mutex};
use duckdb::Connection;
use url::Url;
use std::str::FromStr;
use x402_rs::chain::eip155::{ChecksummedAddress, TokenAmount};
use x402_rs::networks::{KnownNetworkEip155, USDC};

use tiders_x402::facilitator_client::FacilitatorClient;
use tiders_x402::query_handler::AppState;
use tiders_x402::payment_config::GlobalPaymentConfig;
use tiders_x402::price::{PriceTag, TablePaymentOffers};
use tiders_x402::duckdb_reader::get_duckdb_table_schema;
use tiders_x402::start_server;

#[tokio::main]
async fn main() {
    let facilitator = Arc::new(
        FacilitatorClient::try_from("https://facilitator.x402.rs")
            .expect("Failed to create facilitator client")
    );

    let base_url = Url::parse("http://0.0.0.0:4021")
        .expect("Failed to parse base URL");

    let db = Connection::open("data/my_database.db")
        .expect("Failed to open DuckDB connection");

    let mut config = GlobalPaymentConfig::default(facilitator, base_url.clone());

    let usdc = USDC::base_sepolia();
    let price_tag = PriceTag {
        pay_to: ChecksummedAddress::from_str("0xYOUR_ADDRESS_HERE").unwrap(),
        amount_per_item: TokenAmount(usdc.parse("0.002").unwrap().amount),
        token: usdc,
        min_total_amount: None,
        min_items: None,
        max_items: None,
        description: None,
        is_default: true,
    };

    let schema = get_duckdb_table_schema(&db, "my_table").unwrap();
    let offer = TablePaymentOffers::new(
        "my_table".to_string(),
        vec![price_tag],
        Some(schema),
    ).with_description("My dataset".to_string());

    config.add_table_offer(offer);

    let state = Arc::new(AppState {
        db: Arc::new(Mutex::new(db)),
        payment_config: Arc::new(config),
    });

    start_server(state, base_url).await;
}
```

## Quick Start (Python)

First, build the Python bindings:

```bash
cd python
maturin develop
```

Then run the server:

```python
import tiders_x402_server

facilitator = tiders_x402_server.FacilitatorClient("https://x402.org/facilitator")
usdc = tiders_x402_server.USDCDeployment.by_network("base_sepolia")

price_tag = tiders_x402_server.PriceTag(
    pay_to="0xYOUR_ADDRESS_HERE",
    amount_per_item="$0.002",
    token=usdc,
    is_default=True
)

schema = tiders_x402_server.get_duckdb_table_schema_py("./data/my_database.db", "my_table")
offer = tiders_x402_server.TablePaymentOffers("my_table", [price_tag], schema)

config = tiders_x402_server.GlobalPaymentConfig(facilitator, base_url="http://0.0.0.0:4021")
config.add_table_offer(offer)

state = tiders_x402_server.AppState(db_path="./data/my_database.db", payment_config=config)
server = tiders_x402_server.Server(state)
server.start_server("http://0.0.0.0:4021")
```
