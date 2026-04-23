# tiders-x402-server

A payment-enabled database API server that combines analytical databases with the [x402 payment protocol](https://www.x402.org/), enabling pay-per-query data access using cryptocurrency micropayments.

Clients send SQL queries over HTTP, the server estimates the cost and returns an HTTP 402 with payment options. Once the client signs the payment and resends the request, the server verifies and settles it via an x402 facilitator, executes the query, and streams results as Apache Arrow IPC.

Full documentation: <https://yulesa.github.io/tiders-x402-server/>

If you just want to run a server from a YAML file without writing Rust, install the binary directly — this crate ships a `tiders-x402-server` binary by default:

```bash
cargo install tiders-x402-server
# or
pip install tiders-x402-server
```

## Features

- Pay-per-query data access with per-row, fixed, or metadata pricing models
- Volume tiers and free tables
- Pluggable databases via Cargo features: `duckdb`, `postgresql`, `clickhouse`
- Apache Arrow IPC responses (binary columnar, much faster than JSON)
- Restricted SQL dialect (single-table `SELECT` only) — rejects JOINs, subqueries, GROUP BY, CTEs, window functions, and aggregates
- Built-in OpenTelemetry tracing

## Installation

The crate ships both a library and a `tiders-x402-server` CLI binary. By default all three database backends and the CLI dependencies are enabled. If you're embedding the library and want to shed the CLI's transitive deps (`clap`, `serde_yaml`, `notify`, `regex`), opt out of the defaults:

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
| `pyo3` | Python bindings (used by the SDK wheel) |

Default features: `cli`, `duckdb`, `postgresql`, `clickhouse`.

## Quick start

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
| `GET /` | Server metadata: tables, schemas, payment requirements, SQL parser rules. |
| `GET /table/:name` | Full schema and payment offers for a table. Requires payment if the table has a `MetadataPrice` tag. |
| `POST /query` | Execute a SQL query. Returns 402 with payment options, or 200 with an Arrow IPC stream. |

Response formats:

- `200 OK` — `application/vnd.apache.arrow.stream`
- `402 Payment Required` — JSON payment options; resend with `X-Payment` header
- `400 Bad Request` / `500 Internal Server Error` — plain text error

## Dashboard (optional)

Alongside the paid API, the server can expose a free, read-only dashboard at `/dashboard/`. It is useful as a storefront: visitors see what data the server offers before deciding to pay.

Enable it by adding a `dashboard:` section to your YAML config.

```yaml
dashboard:
  enabled: true
  title: "My tiders dashboard"
  default_cache_ttl_minutes: 5     # per-chart TTL; charts may override
  # query_timeout_seconds: 60      # optional, defaults to 60

  charts:
    - id: daily_volume             # ^[a-z0-9][a-z0-9_-]*$, unique
      title: "Daily swap volume"
      sql: |
        SELECT date_trunc('day', to_timestamp(CAST(timestamp AS BIGINT))) AS day,
               COUNT(*) AS swap_count
        FROM uniswap_v3_pool_swap
        GROUP BY day
        ORDER BY day
      module_file: "daily_volume.js"  # resolved against <tiders-server_yaml_dir>/charts/
      # cache_ttl_minutes: 10         # optional per-chart override
```

Each chart's `module_file` points to a JavaScript module under a `charts/` directory next to the config file. The module exports a default function `build(rows, meta) -> EChartsOption` — see [examples/cli/charts/daily_volume.js](../examples/cli/charts/daily_volume.js).

### Dashboard endpoints (all free, no x402)

| Endpoint | Description |
|---|---|
| `GET /dashboard/` | Embedded SPA (placeholder for now) |
| `GET /dashboard/assets/{path}` | Static SPA assets embedded in the binary |
| `GET /dashboard/api/charts` | Chart catalog as JSON: `[{ id, title, moduleUrl, dataUrl }, …]` |
| `GET /dashboard/api/charts/{id}/data` | Arrow IPC stream of the chart's query result. `X-Tiders-Generated-At` header gives the Unix epoch seconds of the cached entry. |
| `GET /dashboard/api/charts/{id}/module` | The chart's JS build module, with `ETag` / `If-None-Match` support |

Chart SQL results are cached in memory per chart with the configured TTL. The dashboard SQL bypasses the restricted parser used by `/query`, so `GROUP BY`, aggregates, joins, and date functions are available.

### Hot reload

When the watcher is enabled (default), editing the `dashboard:` section in the config **or** any file under `<config_dir>/charts/` rebuilds the catalog without a restart. Adding or removing the `dashboard:` section at runtime requires a restart — you'll see a warning in the log.

## Related crates

- [`tiders-x402-server`](https://pypi.org/project/tiders-x402-server/) (PyPI) — the same CLI binary packaged for `pip install`.
- [`tiders-x402-server-sdk`](https://pypi.org/project/tiders-x402-server-sdk/) (PyPI) — Python SDK for embedding the server in your own code.

## License

Licensed under either of [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT license](https://opensource.org/licenses/MIT) at your option.
