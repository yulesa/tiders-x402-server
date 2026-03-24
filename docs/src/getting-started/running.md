# Running the Server

The tiders-x402-server is best used as a library — you construct and start the server from your own code. This page walks through the full setup: setting up a connection with a facilitator, defining pricing, configuring tables, building the application state, and starting the server.

Both Rust and Python examples are shown side by side. They follow the same steps and produce identical servers.

## 1. Connect to a Facilitator

The facilitator handles blockchain-side payment operations (verification and settlement). Point it at a [public facilitators](https://www.x402.org/ecosystem?filter=facilitators) or to a running facilitator instance. 

**Rust:**
```rust
use std::sync::Arc;
use tiders_x402::facilitator_client::FacilitatorClient;

let facilitator = Arc::new(
    FacilitatorClient::try_from("https://facilitator.x402.rs")
        .expect("Failed to create facilitator client")
);
```

**Python:**
```python
import tiders_x402_server

facilitator = tiders_x402_server.FacilitatorClient("https://facilitator.x402.rs")
```

## 2. Define Pricing

Each paid table needs one or more price tags that describe how much to charge and in which token. The token defines the blockchain network and contract address.

**Rust:**
```rust
use std::str::FromStr;
use x402_chain_eip155::chain::ChecksummedAddress;
use x402_types::networks::USDC;
use tiders_x402::price::{PriceTag, TokenAmount};

let usdc = USDC::base_sepolia();

// Default tier: $0.002 per row
let price_tag = PriceTag {
    pay_to: ChecksummedAddress::from_str("0x[your_address]").unwrap(),
    amount_per_item: TokenAmount(usdc.parse("0.002").unwrap().amount),
    token: usdc.clone(),
    min_total_amount: None,
    min_items: None,
    max_items: 99,
    description: None,
    is_default: true,
};

// Bulk tier: $0.001 per row for queries returning 100+ rows
let bulk_price_tag = PriceTag {
    pay_to: ChecksummedAddress::from_str("0x[your_address]").unwrap(),
    amount_per_item: TokenAmount(usdc.parse("0.001").unwrap().amount),
    token: usdc.clone(),
    min_total_amount: None,
    min_items: Some(100),
    max_items: None,
    description: None,
    is_default: false,
};
```

**Python:**
```python
usdc = tiders_x402_server.USDCDeployment.by_network("base_sepolia")

# Default tier: $0.002 per row
price_tag = tiders_x402_server.PriceTag(
    pay_to="0x[your_address]",
    amount_per_item="$0.002",
    token=usdc,
    min_total_amount=None,
    min_items=None,
    max_items=99,
    description=None,
    is_default=True,
)

# Bulk tier: $0.001 per row for queries returning 100+ rows
bulk_price_tag = tiders_x402_server.PriceTag(
    pay_to="0x[your_address]",
    amount_per_item="0.001",
    token=usdc,
    min_total_amount=None,
    min_items=100,
    max_items=None,
    description=None,
    is_default=False,
)
```

When a query estimates multiple rows, both tiers apply and the client receives both options in the 402 response. See [Configuration](./configuration.md) for more details on tiered pricing.

## 3. Configure Tables

Create a offers' table that groups the pricing tiers and associates them with a table in the DB. The schema is optional but recommended — it is shown to clients in the root endpoint.

**Rust:**
```rust
use duckdb::Connection;
use tiders_x402::price::TablePaymentOffers;
use tiders_x402::duckdb_reader::get_duckdb_table_schema;

let db = Connection::open("data/duckdb.db").expect("Failed to open DuckDB");
let schema = get_duckdb_table_schema(&db, "uniswap_v3_pool_swap").unwrap();

let offers_table = TablePaymentOffers::new(
    "uniswap_v3_pool_swap".to_string(),
    vec![price_tag],
    Some(schema),
)
.with_description("Uniswap V3 swaps".to_string())
.with_payment_offer(bulk_price_tag);
```

**Python:**
```python
schema = tiders_x402_server.get_duckdb_table_schema_py("./data/duckdb.db", "uniswap_v3_pool_swap")

offers_table = tiders_x402_server.TablePaymentOffers("uniswap_v3_pool_swap", [price_tag], schema)
offers_table.with_payment_offer(bulk_price_tag)
```

## 4. Build the Payment Configuration

The global payment configuration holds the facilitator client and all offer's tables. The server base URL is used to build the `resource` field in payment requirements.

**Rust:**
```rust
use url::Url;
use tiders_x402::payment_config::GlobalPaymentConfig;

let base_url = Url::parse("http://0.0.0.0:4021").expect("Failed to parse base URL");
let mut config = GlobalPaymentConfig::default(facilitator, base_url.clone());
config.add_offers_table(offers_table);
```

**Python:**
```python
base_url = "http://0.0.0.0:4021"

config = tiders_x402_server.GlobalPaymentConfig(
    facilitator,
    base_url=base_url,
)
config.add_offers_table(offers_table)
```

## 5. Create State and Start the Server

Wrap the database connection and payment configuration into the application state, then start the server.

**Rust:**
```rust
use std::sync::{Arc, Mutex};
use tiders_x402::{AppState, start_server};

let state = Arc::new(AppState {
    db: Arc::new(Mutex::new(db)),
    payment_config: Arc::new(config),
});

start_server(state, base_url).await;
```

**Python:**
```python
state = tiders_x402_server.AppState(
    db_path="./data/duckdb.db",
    payment_config=config,
)

server = tiders_x402_server.Server(state)
server.start_server(base_url)
```

The server blocks until it receives a shutdown signal (Ctrl+C or SIGTERM).

## Verifying the Server

Once running, check the root endpoint:

```bash
curl http://localhost:4021/
```

This returns the available tables, their schemas, and the SQL parser rules.

## Environment Variables

The server loads `.env` files via `dotenvy`. OpenTelemetry tracing is opt-in — set these variables to export traces:

```bash
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
OTEL_SERVICE_NAME=tiders-x402-server
```
