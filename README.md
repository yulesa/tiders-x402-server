<img src="/resources/tiders_logo2.png" alt="Tiders" width="1000">

# Tiders x402 Server

[![Documentation](https://img.shields.io/badge/documentation-blue?style=for-the-badge&logo=readthedocs)](https://yulesa.github.io/tiders-x402-server/)
[![PyPI CLI](https://img.shields.io/badge/PyPI%20CLI-lightgreen?style=for-the-badge&logo=pypi&labelColor=white)](https://pypi.org/project/tiders-x402-server/)
[![telegram](https://img.shields.io/badge/telegram-blue?style=for-the-badge&logo=telegram)](https://t.me/tidersindexer)

**Sell access to your data** — install the server, point this server at a database, set a price, and anyone on the internet can pay small cryptocurrency amounts to query your data instantly, with no contracts, accounts, or billing setup.

Buyers submit SQL queries over HTTP and receive results as efficient Apache Arrow IPC streams. You control what data is exposed, which tables are available, and how much each query costs — per row returned or a flat fee to access a table. Buyers interact using familiar SQL, but Tiders enforces a safe subset — blocking expensive operations like JOINs and subqueries — so your database stays protected and costs stay predictable.

Payments are handled by the [x402 protocol](https://www.x402.org/), an open standard for HTTP-native micropayments. When a server needs payment it returns a standard `402 Payment Required` response; a client with x402 support signs it, and the transaction settles in under a second using stablecoins — no accounts, no checkout pages, no minimum spend.

Under the hood, Tiders is a [Rust](https://www.rust-lang.org/) server that connects to different databases such as [DuckDB](https://duckdb.org/), [PostgreSQL](https://www.postgresql.org/), or [ClickHouse](https://clickhouse.com/). Buyers query using familiar SQL, but Tiders enforces a safe subset — blocking expensive operations like JOINs and subqueries — so your database stays protected and costs stay predictable.

Serving raw data from your database through a paid API is the server's core job. As an optional addition — because selling data is hard without showing buyers what they're getting — the same server application can scaffold and publish interactive **dashboards**: fully customizable visual reports you design and your buyers explore in a browser, with a convenience button that hits the server's x402-gated APIs to download the underlying data. The server works exactly the same with or without dashboards.

Think of the dashboard feature as a vending machine for data: buyers browse the dashboard to preview what's available, then pay per request to access the full dataset. You stay in full control — what data is freely visible in the dashboard, which tables the server exposes, and how much each query costs, whether that's per row returned or a flat fee to access a table.

<img src="/resources/tiders_x402_server_components.png" alt="Tiders-x402-server Components">

## How Paid Requests Work

```
1. A client sends a SQL query to `GET /api/query?query=…`.
2. The server parses and validates the query, then estimates the payment options.
3. If payment is required, the server responds with HTTP 402 and a list of payment options.
4. The client signs a payment using their crypto wallet and resends the request with a `Payment-Signature` header.
5. The server verifies and settles the payment through a facilitator, then returns the query results as Arrow IPC.
```

<img src="/resources/payment_flow.png" alt="Server Payment Flow">

## Features

- **Pay-per-query data access** — charge a flat fee, per row returned, or a one-time fee for table metadata
- **Tiered pricing** — volume tiers, multiple tokens, and multiple networks per table
- **Multiple databases** — DuckDB, PostgreSQL, and ClickHouse backends
- **CLI and SDK** — run from a YAML config file (no code) or embed as a Rust/Python library
- **Apache Arrow responses** — efficient binary columnar format, significantly faster than JSON
- **Safe SQL subset** — parser blocks JOINs, GROUP BY, subqueries, and other expensive operations
- **Embedded dashboards** — scaffold and serve Evidence dashboards from the same binary, with x402 wallet-connect download buttons baked in
- **Hot reload** — tables, pricing, facilitator settings, and dashboard config reload on file change without a restart
- **Observability** — built-in OpenTelemetry tracing support

## Documentation

Full documentation is available at the [documentation site](https://yulesa.github.io/tiders-x402-server/).

## Installation

### CLI (prebuilt binary, no Rust toolchain needed)

```bash
pip install tiders-x402-server
# or
cargo install tiders-x402-server
```

Both commands install the same `tiders-x402-server` binary with DuckDB, PostgreSQL, and ClickHouse backends bundled.


## Quick Start

Tiders-x402-server assumes you already have a database populated with the data you want to sell. If you don't, the [Tiders ingestion tool](https://github.com/yulesa/tiders) can help you stand one up and load it with crypto data — see [Choosing a Database](https://yulesa.github.io/tiders-docs/getting_started/choosing_a_database.html) for guidance on picking a backend.

1. Install the CLI:

```bash
pip install tiders-x402-server
```

2. Create a `tiders-x402-server.yaml`:

```yaml
server:
  bind_address: "0.0.0.0:4021"
  base_url: "http://localhost:4021"

facilitator:
  url: "https://facilitator.x402.rs"

database:
  duckdb:
    path: "./data/my_data.duckdb"

tables:
  - name: my_table
    description: "My dataset"
    price_tags:
      - type: per_row
        pay_to: "0xYourWalletAddress"
        token: usdc/base_sepolia
        amount_per_item: "0.002"
        is_default: true

dashboard: # Optional
  entries:
    - slug: my_dashboard
      title: "My Dashboard"
      description: "Description text for the dashboard"
      tags: ["Tag1", "Tag2"]
```
3. Start the server:

```bash
tiders-x402-server start # auto-discovery in the current folder
tiders-x402-server start <tiders-config-yaml-path>
```

The CLI auto-discovers YAML config files  in the current folder, supports `${VAR_NAME}` environment variable expansion, and hot-reloads configuration on file changes.

4. Optional - Create the dashboards:

You can create dashboards before or after starting the server. Scaffold dashboards defined in the YAML with the CLI command:

```bash
tiders-x402-server dashboard          # scaffold all entries
tiders-x402-server dashboard <slug>   # scaffold one
```

This copies a minimal [Evidence](https://evidence.dev/) project template into `dashboards/<slug>/`. From there, edit the files, mainly `dashboards/<slug>/pages/index.md`, to build your reports — the [Evidence docs](https://docs.evidence.dev/) cover the full dashboard authoring workflow.

> **Note:** Data visible in the dashboard can be scraped freely without payment. Only expose data you are comfortable sharing publicly, and leave anything sensitive behind the paid API instead.

Once the dashboard is ready, build it into a static site:

```bash
(cd dashboards/<slug> && npm install && npm run build)
```

The server will pick up and serve the built files automatically. Dashboards are static — they do not update live. Rebuild whenever the underlying data changes.

## Server API

### `GET /`

Landing page listing every enabled dashboard. Only present when `dashboards:` is configured.

### `GET /api/`

Returns server metadata: available tables, schemas, payment requirements, and SQL parser rules.

### `GET /api/table/:name`

Returns full schema and payment offers for a specific table. Requires payment if the table has a `MetadataPrice` tag.

### `GET /api/query`

Execute a SQL query. The SQL is passed via the `query` URL parameter. Queries must use the restricted SQL dialect — single-table `SELECT` only; JOINs, subqueries, GROUP BY, CTEs, window functions, and aggregates are rejected.

When payment is required, returns `402` with payment options. Resend the same URL with the `X-Payment` header containing the signed payment.

```bash
# Step 1: Get pricing
curl --get http://localhost:4021/api/query \
  --data-urlencode "query=SELECT * FROM my_table LIMIT 10"
# Returns 402 with payment options

# Step 2: Send with payment (typically handled by a x402 client library available at https://github.com/x402-foundation/x402)
curl --get http://localhost:4021/api/query \
  --data-urlencode "query=SELECT * FROM my_table LIMIT 10" \
  -H "X-Payment: <base64-encoded-signed-payment>"
# Returns Arrow IPC binary stream
```

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

## Observability

Set the following environment variables to enable OpenTelemetry tracing:

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
export OTEL_SERVICE_NAME=tiders-x402-server
```

## Technology Stack

| Component | Technology |
|---|---|
| Web framework | Axum |
| Databases | DuckDB, PostgreSQL, ClickHouse |
| Payment protocol | x402 V2 (via `x402-rs` and `x402-chain-eip155`) |
| Data serialization | Apache Arrow IPC |
| SQL parsing | `sqlparser` |
| Dashboards | Evidence (Svelte) + wagmi + viem + `@x402/evm` |
| Observability | OpenTelemetry + tracing |

## Development

```bash
# Build
cargo build -p tiders-x402-server --features duckdb

# Build Python bindings
cd python && maturin develop --uv --features duckdb
```

For persistent local development, patch the crate path in `examples/rust/Cargo.toml`:

```toml
[patch.crates-io]
tiders-x402-server = { path = "../../server" }
```

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
