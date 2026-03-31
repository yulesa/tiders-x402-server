<img src="/resources/tiders_logo2.png" alt="Tiders" width="1000">

# Tiders x402 Server

[![Documentation](https://img.shields.io/badge/documentation-blue?style=for-the-badge&logo=readthedocs)](https://yulesa.github.io/tiders-x402-server/)
[![PyPI](https://img.shields.io/badge/PyPI-lightgreen?style=for-the-badge&logo=pypi&labelColor=white)](https://pypi.org/project/tiders-x402-server/)

Tiders-x402-server is a payment-enabled database API server that combines analytical databases with the [x402 payment protocol](https://www.x402.org/), enabling pay-per-query data access using cryptocurrency micropayments.

Clients send SQL queries over HTTP, the server estimates the cost, and returns an HTTP 402 response with payment options. Once the client signs a payment and resends the request, the server verifies and settles the payment, then returns results as efficient Apache Arrow IPC streams.

<img src="/resources/tiders_x402_server_components.png" alt="Tiders-x402-server Components">

## How It Works

```
1. Client sends SQL query
2. Server validates SQL, calculate the cost based on the table, returns 402 with pricing
3. Client signs payment with crypto, resends the request with X-Payment header
4. Server verifies/settles payment via x402 facilitator
5. Server executes query, returns results as Arrow IPC
```
<img src="/resources/payment_flow.png" alt="Server Payment Flow">

## Features

- **Pay-per-query data access** — monetize your datasets with cryptocurrency micropayments
- **Tiered pricing** — per-row pricing with volume tiers, or fixed pricing per query
- **Multiple databases** — DuckDB, PostgreSQL, and ClickHouse backends
- **Apache Arrow responses** — efficient binary columnar format, significantly faster than JSON
- **Familiar simplified SQL** — parser prevents JOINs, GROUP BY, subqueries, and other expensive operations
- **Multi-language** — Rust server for efficience, Python bindings (PyO3) for convenience
- **Observability** — built-in OpenTelemetry tracing support

## Documentation

Full documentation is available at the [documentation site](https://yulesa.github.io/tiders-x402-server/).

## Installation

### Python

```bash
uv pip install tiders-x402-server
```

### Rust

Add to your `Cargo.toml` with at least one database feature enabled:

```toml
[dependencies]
tiders-x402 = { version = "0.1.0", features = ["duckdb"] }
```

Available features: `duckdb`, `postgresql`, `clickhouse`

## Quick Start

### Python

```python
import tiders_x402_server

# 1. Create facilitator client
facilitator = tiders_x402_server.FacilitatorClient("https://facilitator.x402.rs")

# 2. Create database and load data
db = tiders_x402_server.DuckDbDatabase.file("my_data.duckdb")

# 3. Define pricing
token = tiders_x402_server.base_sepolia_usdc()
price_tag = tiders_x402_server.PriceTag(
    pay_to="0xYourWalletAddress",
    pricing=tiders_x402_server.PricingModel.per_row(amount_per_item="2000000000000000"),
    token=token,
)

# 4. Build payment config
offers = tiders_x402_server.TablePaymentOffers("my_table", "table_description")
offers.add_payment_offer(price_tag)

payment_config = tiders_x402_server.GlobalPaymentConfig(facilitator)
payment_config.add_offers_table(offers)

# 5. Start 
server_base_url = "http://0.0.0.0:4021"
state = tiders_x402_server.AppState(db, payment_config, server_base_url)
tiders_x402_server.start_server(state)
```

### Rust

```rust
use tiders_x402::{
    start_server, AppState, GlobalPaymentConfig, TablePaymentOffers,
    PriceTag, PricingModel, FacilitatorClient,
};

let facilitator = Arc::new(FacilitatorClient::try_from("https://facilitator.x402.rs")?);
let db = DuckDbDatabase::in_memory()?;

let price_tag = PriceTag::new(pay_to, PricingModel::per_row("2000000000000000"), token);
let mut offers = TablePaymentOffers::new("my_table", "table_description");
offers.add_payment_offer(price_tag);

let mut payment_config = GlobalPaymentConfig::new(facilitator);
payment_config.add_offers_table(offers);

let server_base_url = Url::parse("http://0.0.0.0:4021").expect("Failed to parse server base URL");
let state = AppState::new(db, payment_config, server_base_url);
start_server(state).await?;
```

The server starts on `server_base_url`. Verify with:

```bash
curl http://localhost:4021/
```

## API

### `GET /`

Returns server metadata: available tables, schemas, payment requirements, and SQL parser rules.

### `POST /query`

Execute a SQL query. 
When a payment is necessary, the server returns 402 with payment options. A client implementing x-402 protocol resend the request with `X-Payment` payload containing the signed payment.

```bash
# Step 1: Get pricing
curl -X POST http://localhost:4021/query \
  -H "Content-Type: application/json" \
  -d '{"query": "SELECT * FROM my_table LIMIT 10"}'
# Returns 402 with payment options

# Step 2: Send with payment (typically handled by client library)
curl -X POST http://localhost:4021/query \
  -H "Content-Type: application/json" \
  -H "X-Payment: <base64-encoded-signed-payment>" \
  -d '{"query": "SELECT * FROM my_table LIMIT 10"}'
# Returns Arrow IPC binary stream
```

**Response formats:**
- `200 OK` — Arrow IPC binary stream (`application/vnd.apache.arrow.stream`)
- `402 Payment Required` — JSON with payment options
- `400 Bad Request` — plain text error (invalid SQL or malformed payment)
- `500 Internal Server Error` — plain text error

## Reading Arrow IPC Responses

**TypeScript:**
```typescript
import * as arrow from "apache-arrow";
const table = arrow.tableFromIPC(arrayBuffer);
```

**Python:**
```python
import pyarrow as pa
reader = pa.ipc.open_stream(response_bytes)
table = reader.read_all()
```

**Rust:**
```rust
use arrow_ipc::reader::StreamReader;
let reader = StreamReader::try_new(Cursor::new(bytes), None)?;
```

## Examples

| Example | Language | Database |
|---|---|---|
| [DuckDB Server](examples/python/duckdb_server.py) | Python | DuckDB |
| [Rust Server](examples/rust/src/main.rs) | Rust | DuckDB / Clickhouse/ PostgreSQL |

Both examples load sample Uniswap V3 swap data and serve it with tiered per-row pricing.

### Client Scripts

Sample client scripts are provided in [`client-scripts/`](client-scripts/) to test a running server. They send a query, handle the x402 payment flow automatically, and parse the Arrow IPC response.

| Client | Language | Run |
|---|---|---|
| [TypeScript](client-scripts/typescript-client.ts) | TypeScript | `npx tsx typescript-client.ts` |
| [Python](client-scripts/python-client.py) | Python | `uv run python-client.py` |

## Pricing Models

Tiders-x402-server implements 2 pricing models. **Per-row pricing** charges based on the number of rows returned, with support for volume tiers:

```python
# Default tier: $0.002 per row
default_tag = PriceTag(pay_to, PricingModel.per_row("2000000000000000"), token)

# Bulk tier: $0.001 per row for 100+ rows
bulk_tag = PriceTag(pay_to, PricingModel.per_row("1000000000000000", min_items=100), token)
```

**Fixed pricing** charges a flat fee regardless of result size:

```python
fixed_tag = PriceTag(pay_to, PricingModel.fixed("5000000000000000"), token)
```

Tables can also be marked as free, requiring no payment.

## Observability

The server supports OpenTelemetry tracing. Set the following environment variables:

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
export OTEL_SERVICE_NAME=tiders-x402-server
```

## Technology Stack

| Component | Technology |
|---|---|
| Web framework | Axum |
| Databases | DuckDB, PostgreSQL, ClickHouse |
| Async runtime | Tokio |
| Payment protocol | x402-rs |
| Data serialization | Apache Arrow IPC |
| SQL parsing | sqlparser |
| Python bindings | PyO3 |
| Observability | OpenTelemetry + tracing |

## Development

If you're modifying `tiders-x402-server` repo locally, you probably want to build it against your local version.

```bash
# Build
cargo build -p tiders-x402-server --features duckdb

# Build Python bindings
cd python && maturin develop --uv --features duckdb
```

**Persistent local development**

For persistent local development, you can put this in `examples/rust/Cargo.toml`:

```toml
[patch.crates-io]
tiders-x402-server = { path = "../../server" }
```

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
