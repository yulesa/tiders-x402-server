# tiders-x402-server (CLI)

**Sell access to your data** — install the server, point this server at a database, set a price, and anyone on the internet can pay small cryptocurrency amounts to query your data instantly, with no contracts, accounts, or billing setup.

Buyers submit SQL queries over HTTP and receive results as efficient Apache Arrow IPC streams. You control what data is exposed, which tables are available, and how much each query costs — per row returned or a flat fee to access a table. Buyers interact using familiar SQL, but Tiders enforces a safe subset — blocking expensive operations like JOINs and subqueries — so your database stays protected and costs stay predictable.

Payments are handled by the [x402 protocol](https://www.x402.org/), an open standard for HTTP-native micropayments. When a server needs payment it returns a standard `402 Payment Required` response; a client with x402 support signs it, and the transaction settles in under a second using stablecoins — no accounts, no checkout pages, no minimum spend.

Under the hood, Tiders is a [Rust](https://www.rust-lang.org/) server that connects to different databases such as [DuckDB](https://duckdb.org/), [PostgreSQL](https://www.postgresql.org/), or [ClickHouse](https://clickhouse.com/). Buyers query using familiar SQL, but Tiders enforces a safe subset — blocking expensive operations like JOINs and subqueries — so your database stays protected and costs stay predictable.

Serving raw data from your database through a paid API is the server's core job. As an optional addition — because selling data is hard without showing buyers what they're getting — the same server application can scaffold and publish interactive **dashboards**: fully customizable visual reports you design and your buyers explore in a browser, with a convenience button that hits the server's x402-gated APIs to download the underlying data. The server works exactly the same with or without dashboards.

Think of the dashboard feature as a vending machine for data: buyers browse the dashboard to preview what's available, then pay per request to access the full dataset. You stay in full control — what data is freely visible in the dashboard, which tables the server exposes, and how much each query costs, whether that's per row returned or a flat fee to access a table.

Full documentation: <https://yulesa.github.io/tiders-x402-server/>

## Installation

```bash
pip install tiders-x402-server
# or
cargo install tiders-x402-server
```

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
tiders-x402-server start
```

The CLI auto-discovers YAML config files, supports `${VAR_NAME}` environment variable expansion, and hot-reloads configuration on file changes.

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

- [`tiders-x402-server`](https://crates.io/crates/tiders-x402-server) — Rust library and binary on crates.io.

## License

Licensed under either of [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT license](https://opensource.org/licenses/MIT) at your option.
