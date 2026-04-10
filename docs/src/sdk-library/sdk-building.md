# Building a Server with the SDK

When using tiders-x402-server as a library, you construct and start the server from your own code. This gives you full control over database setup, pricing logic, and server lifecycle.

Both Rust and Python examples are shown side by side. They follow the same steps and produce identical servers. For the full range of configuration options see the [Configuration Reference](./sdk-configuration.md).

> **Prefer zero-code setup?** Use the [CLI](../cli/cli-overview.md) instead -- define everything in a YAML config file and run
>```bash
>tiders-x402-server start
> ```

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

Create a database backend. 

*The examples in the repo ship with a sample CSV file (`uniswap_v3_pool_swap.csv`) containing Uniswap V3 swap data. For DuckDB, you can load it directly. For PostgreSQL and ClickHouse, seed scripts are provided.*

### DuckDB

**Rust:**
```rust
// Load sample data from CSV into an in-memory DuckDB database.
let conn = duckdb::Connection::open_in_memory().unwrap();
let db = tiders_x402_server::database_duckdb::DuckDbDatabase::new(conn);
```

**Python:**
```python
import duckdb
db_path = "data/duckdb.db"
conn = duckdb.connect(db_path)
db = tiders_x402_server.DuckDbDatabase(db_path)
```

## 3. Define Price Tags

A `PriceTag` describes a single pricing tier: who gets paid, how much, in which token, and under which pricing model. Tables can have multiple price tags for tiered pricing -- clients receive all applicable options in the 402 response.

Three pricing models are supported: **per-row**, **fixed**, and **metadata price**. Amounts accept human-readable decimal strings (e.g., `"0.002"`).

**Per Row**

Price scales linearly with the number of rows returned. Supports tiered pricing via `min_items` / `max_items`.

**Rust:**
```rust
use std::str::FromStr;
use x402_chain_eip155::chain::ChecksummedAddress;
use x402_types::networks::USDC;
use tiders_x402_server::price::{PriceTag, PricingModel, TokenAmount};

let usdc = USDC::base_sepolia();

// Default tier: $0.002 per row
let default_tag = PriceTag {
    pay_to: ChecksummedAddress::from_str("0x[your_address]").unwrap(),
    pricing: PricingModel::PerRow {
        amount_per_item: TokenAmount(usdc.parse("0.002").unwrap().amount),
        min_total_amount: None,
        min_items: None,
        max_items: None,
    },
    token: usdc.clone(),
    description: None,
    is_default: true,
};

// Bulk tier: $0.001 per row for 100+ rows
let bulk_tag = PriceTag {
    pay_to: ChecksummedAddress::from_str("0x[your_address]").unwrap(),
    pricing: PricingModel::PerRow {
        amount_per_item: TokenAmount(usdc.parse("0.001").unwrap().amount),
        min_total_amount: None,
        min_items: Some(100),
        max_items: None,
    },
    token: usdc.clone(),
    description: None,
    is_default: false,
};
```

**Python:**
```python
usdc = tiders_x402_server.USDC("base_sepolia")

# Default tier: $0.002 per row
default_tag = tiders_x402_server.PriceTag(
    pay_to="0x[your_address]",
    amount_per_item="0.002",
    token=usdc,
    is_default=True,
)

# Bulk tier: $0.001 per row for 100+ rows
bulk_tag = tiders_x402_server.PriceTag(
    pay_to="0x[your_address]",
    amount_per_item="0.001",
    token=usdc,
    min_items=100,
)
```

**Fixed**

A flat fee regardless of how many rows are returned.

**Rust:**
```rust
let fixed_tag = PriceTag {
    pay_to: ChecksummedAddress::from_str("0x[your_address]").unwrap(),
    pricing: PricingModel::Fixed {
        amount: TokenAmount(usdc.parse("1.00").unwrap().amount),
    },
    token: usdc.clone(),
    description: Some("Fixed price query".to_string()),
    is_default: false,
};
```

**Python:**
```python
fixed_tag = tiders_x402_server.PriceTag.fixed(
    pay_to="0x[your_address]",
    fixed_amount="1.00",
    token=usdc,
    description="Fixed price query",
)
```

**Metadata Price**

A flat fee for accessing table metadata (schema and payment offers) via the `GET /table/:name` endpoint. Without this tag, metadata is returned freely. Charging for metadata API calls can be used to prevent API abuse.

**Rust:**
```rust
let metadata_tag = PriceTag {
    pay_to: ChecksummedAddress::from_str("0x[your_address]").unwrap(),
    pricing: PricingModel::MetadataPrice {
        amount: TokenAmount(usdc.parse("1.00").unwrap().amount),
    },
    token: usdc.clone(),
    description: Some("Metadata access fee".to_string()),
    is_default: false,
};
```

**Python:**
```python
metadata_tag = tiders_x402_server.PriceTag.metadata_price(
    pay_to="0x[your_address]",
    amount="1.00",
    token=usdc,
    description="Metadata access fee",
)
```

## 4. Build Table Payment Offers

A `TablePaymentOffers` groups the price tags for a single table along with its description and schema. The schema is optional but recommended -- it is shown to clients in the root endpoint.

**Rust:**
```rust
use tiders_x402_server::price::TablePaymentOffers;
use tiders_x402_server::Database;

let schema = db.get_table_schema("uniswap_v3_pool_swap")
    .await
    .expect("Failed to get table schema");

let offers_table = TablePaymentOffers::new(
    "uniswap_v3_pool_swap".to_string(),
    vec![default_tag, bulk_tag, fixed_tag],
    Some(schema),
)
.with_description("Uniswap V3 swaps".to_string());
```

**Python:**
```python
schema = db.get_table_schema("uniswap_v3_pool_swap")

offers_table = tiders_x402_server.TablePaymentOffers(
    "uniswap_v3_pool_swap",
    [default_tag, bulk_tag, fixed_tag],
    schema=schema,
    description="Uniswap V3 swaps",
)
```

For free tables that require no payment, use `TablePaymentOffers::new_free_table` (Rust) or `TablePaymentOffers.new_free_table` (Python). See the [Configuration Reference](./sdk-configuration.md) for all available methods.

## 5. Build the Payment Configuration

The global payment configuration holds the facilitator client and all table offers.

**Rust:**
```rust
use tiders_x402_server::payment_config::GlobalPaymentConfig;

let mut global_payment_config = GlobalPaymentConfig::default(facilitator);
global_payment_config.add_offers_table(offers_table);
```

**Python:**
```python

global_payment_config = tiders_x402_server.GlobalPaymentConfig(
    facilitator,
)
global_payment_config.add_offers_table(offers_table)
```

## 6. Create State and Start the Server

Wrap the database and payment configuration into the application state, then start the server.

**Rust:**
```rust
use url::Url;
use tiders_x402_server::{AppState, start_server};

let server_base_url = Url::parse("http://localhost:4021").expect("Failed to parse server base URL");
let state = AppState::new(db, global_payment_config, server_base_url, "0.0.0.0:4021".to_string());

start_server(state).await;
```

**Python:**
```python
state = tiders_x402_server.AppState(
    db,
    global_payment_config,
    "http://localhost:4021",
    "0.0.0.0:4021",
)

tiders_x402_server.start_server(state)
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
