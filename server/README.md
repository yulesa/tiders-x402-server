# tiders-x402-server

**Sell access to your data** — install the server, point this server at a database, set a price, and anyone on the internet can pay small cryptocurrency amounts to query your data instantly, with no contracts, accounts, or billing setup.

Buyers submit SQL queries over HTTP and receive results as efficient Apache Arrow IPC streams. You control what data is exposed, which tables are available, and how much each query costs — per row returned or a flat fee to access a table. Buyers interact using familiar SQL, but Tiders enforces a safe subset — blocking expensive operations like JOINs and subqueries — so your database stays protected and costs stay predictable.

Payments are handled by the [x402 protocol](https://www.x402.org/), an open standard for HTTP-native micropayments. When a server needs payment it returns a standard `402 Payment Required` response; a client with x402 support signs it, and the transaction settles in under a second using stablecoins — no accounts, no checkout pages, no minimum spend.

Under the hood, Tiders is a [Rust](https://www.rust-lang.org/) server that connects to different databases such as [DuckDB](https://duckdb.org/), [PostgreSQL](https://www.postgresql.org/), or [ClickHouse](https://clickhouse.com/). Buyers query using familiar SQL, but Tiders enforces a safe subset — blocking expensive operations like JOINs and subqueries — so your database stays protected and costs stay predictable.

Serving raw data from your database through a paid API is the server's core job. As an optional addition — because selling data is hard without showing buyers what they're getting — the same server application can scaffold and publish interactive **dashboards**: fully customizable visual reports you design and your buyers explore in a browser, with a convenience button that hits the server's x402-gated APIs to download the underlying data. The server works exactly the same with or without dashboards.

Think of the dashboard feature as a vending machine for data: buyers browse the dashboard to preview what's available, then pay per request to access the full dataset. You stay in full control — what data is freely visible in the dashboard, which tables the server exposes, and how much each query costs, whether that's per row returned or a flat fee to access a table.

```bash
pip install tiders-x402-server
# or
cargo install tiders-x402-server
```

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

The crate ships both a library and a `tiders-x402-server` CLI binary. By default all three database backends and the CLI dependencies are enabled. To embed the library without the CLI's transitive deps (`clap`, `serde_yaml`, `notify`, `regex`), opt out of the defaults:

```toml
[dependencies]
tiders-x402-server = { version = "0.2", default-features = false, features = ["duckdb"] }
```

Available features:

| Feature | Description |
|---|---|
| `duckdb` | DuckDB backend |
| `postgresql` | PostgreSQL backend |
| `clickhouse` | ClickHouse backend |
| `cli` | CLI/YAML loader and `tiders-x402-server` binary (default) |

Default features: `cli`, `duckdb`, `postgresql`, `clickhouse`.

## Quick Start

```rust,no_run
use tiders_x402_server::{
    start_server, AppState, GlobalPaymentConfig, TablePaymentOffers,
    PriceTag, PricingModel, FacilitatorClient, DuckDbDatabase,
};
use url::Url;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let facilitator = FacilitatorClient::try_from("https://facilitator.x402.rs")?;
    let db = DuckDbDatabase::in_memory()?;

    let price_tag = PriceTag::new(pay_to, PricingModel::per_row("2000000000000000"), token);
    let mut offers = TablePaymentOffers::new("my_table", "table description");
    offers.add_payment_offer(price_tag);

    let mut payment_config = GlobalPaymentConfig::new(facilitator);
    payment_config.add_offers_table(offers);

    let base_url = Url::parse("http://0.0.0.0:4021")?;
    let state = AppState::new(db, payment_config, base_url, "0.0.0.0:4021".to_string());
    start_server(state).await;
    Ok(())
}
```

## HTTP API

| Endpoint | Description |
|---|---|
| `GET /api/` | Server metadata: tables, schemas, payment requirements, SQL parser rules |
| `GET /api/table/:name` | Full schema and payment offers for a table. Requires payment if the table has a `MetadataPrice` tag |
| `GET /api/query?query=…` | Execute a SQL query (SQL in the `query` URL parameter). Returns `402` with payment options, or `200` with an Arrow IPC stream |
| `GET /<slug>/` | Static Evidence dashboard, one per `dashboards:` entry |

Response formats:

- `200 OK` — `application/vnd.apache.arrow.stream`
- `402 Payment Required` — JSON payment options; resend with `X-Payment` header
- `400 Bad Request` / `500 Internal Server Error` — plain text error

## Related packages

- [`tiders-x402-server`](https://pypi.org/project/tiders-x402-server/) (PyPI) — the same CLI binary packaged for `pip install`.

## License

Licensed under either of [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT license](https://opensource.org/licenses/MIT) at your option.
