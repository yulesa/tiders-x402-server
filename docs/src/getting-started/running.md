# Running the Server

The tiders-x402-server is best used as a library — you construct and start the server from your own code. This page walks through the minimal setup: connecting to a facilitator, defining pricing, configuring a table, and starting the server.

Both Rust and Python examples are shown side by side. They follow the same steps and produce identical servers. For the full range of configuration options see the [Server Components](../server/configuration.md) section.

## 1. Connect to a Facilitator

The facilitator handles blockchain-side payment operations (verification and settlement). Point it at a [public facilitator](https://www.x402.org/ecosystem?filter=facilitators) or a running facilitator instance.

**Rust:**
```rust
use std::sync::Arc;
use tiders_x402_server::facilitator_client::FacilitatorClient;

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

## 2. Create a Database

Create a database backend. This example uses DuckDB — see [Database](../server/database.md) for PostgreSQL and ClickHouse options.

**Rust:**
```rust
let db = tiders_x402::database_duckdb::DuckDbDatabase::from_path("data/duckdb.db")
    .expect("Failed to open DuckDB");
```

**Python:**
```python
db = tiders_x402_server.DuckDbDatabase("data/duckdb.db")
```

## 3. Define Pricing and Configure Tables

Each paid table needs a price tag that describes how much to charge and in which token. The schema is optional but recommended — it is shown to clients in the root endpoint.

**Rust:**
```rust
use std::str::FromStr;
use x402_chain_eip155::chain::ChecksummedAddress;
use x402_types::networks::USDC;
use tiders_x402_server::price::{PriceTag, TablePaymentOffers, TokenAmount};
use tiders_x402_server::Database;

let usdc = USDC::base_sepolia();

let price_tag = PriceTag {
    pay_to: ChecksummedAddress::from_str("0x[your_address]").unwrap(),
    amount_per_item: TokenAmount(usdc.parse("0.002").unwrap().amount),
    token: usdc.clone(),
    min_total_amount: None,
    min_items: None,
    max_items: None,
    description: None,
    is_default: true,
};

let schema = db.get_table_schema("uniswap_v3_pool_swap")
    .await
    .expect("Failed to get table schema");

let offers_table = TablePaymentOffers::new(
    "uniswap_v3_pool_swap".to_string(),
    vec![price_tag],
    Some(schema),
)
.with_description("Uniswap V3 swaps".to_string());
```

**Python:**
```python
usdc = tiders_x402_server.USDC("base_sepolia")

price_tag = tiders_x402_server.PriceTag(
    pay_to="0x[your_address]",
    amount_per_item="$0.002",
    token=usdc,
    min_total_amount=None,
    min_items=None,
    max_items=None,
    description=None,
    is_default=True,
)

schema = db.get_table_schema("uniswap_v3_pool_swap")

offers_table = tiders_x402_server.TablePaymentOffers("uniswap_v3_pool_swap", [price_tag], schema)
```

You can add multiple price tags to a table for tiered pricing. See [Configuration](./configuration.md) for details.

## 4. Build the Payment Configuration

The global payment configuration holds the facilitator client and all table offers. The server base URL is used to build the `resource` field in payment requirements.

**Rust:**
```rust
use url::Url;
use tiders_x402_server::payment_config::GlobalPaymentConfig;

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

Wrap the database and payment configuration into the application state, then start the server.

**Rust:**
```rust
use tiders_x402_server::{AppState, start_server};

let state = Arc::new(AppState {
    db: Arc::new(db),
    payment_config: Arc::new(config),
});

start_server(state, base_url).await;
```

**Python:**
```python
state = tiders_x402_server.AppState(
    db,
    payment_config=config,
)

tiders_x402_server.start_server_py(state, base_url)
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
