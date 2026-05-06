![Tiders](resources/tiders_logo2.png)

# Tiders x402 Server

Tiders x402 Server lets you **sell access to your data** — you point it at your database, set a price, and anyone on the internet can make micro payments to query your data instantly, with no contracts, accounts, or billing setup.

Buyers submit SQL queries over HTTP and receive results as efficient Apache Arrow IPC streams. You control what data is exposed, which tables are available, and how much each query costs — per row returned or a flat fee to access a table. Buyers interact using familiar SQL, but Tiders enforces a safe subset — blocking expensive operations like JOINs and subqueries — so your database stays protected and costs stay predictable.

On top of selling raw data, Tiders x402 Server can also publish interactive **dashboards** — fully customizable visual reports you design and your buyers explore in a browser, with a built-in button to pay and download the underlying data.

Think of it as a vending machine for data: buyers browse the dashboard to preview what's available, then pay per request to access the full dataset. You stay in full control — what data is freely visible in the dashboard, which tables the server exposes, and how much each query costs, whether that's per row returned or a flat fee to access a table.

Payments are handled by the [x402 protocol](https://www.x402.org/), an open standard for HTTP-native micropayments. When a server needs payment it returns a standard `402 Payment Required` response; a client with x402 support signs it, and the transaction settles in under a second using stablecoins — no accounts, no checkout pages, no minimum spend.

Under the hood, Tiders is a [Rust](https://www.rust-lang.org/) server that connects to different databases such as [DuckDB](https://duckdb.org/), [PostgreSQL](https://www.postgresql.org/), or [ClickHouse](https://clickhouse.com/). Buyers query using familiar SQL, but Tiders enforces a safe subset — blocking expensive operations like JOINs and subqueries — so your database stays protected and costs stay predictable.

## Three Ways to Use

| Mode | How | When to use |
|------|-----|-------------|
| **CLI** | Write a YAML config file with available tables and prices, run `tiders-x402-server start` | Quick setup, no coding required, config-driven deployments |
| **Dashboards** | Optional `dashboards` can be scaffold with `tiders-x402-server dashboard <slug>` and are fully customizable | Published alongside the API endpoints, allow buyers browse the data |
| **SDK Library** | Import in Rust, configure programmatically | Custom logic, embedding in larger applications, maximum flexibility |

See [CLI Quick Start](./getting-started/cli-quickstart.md) or [SDK Library Overview](./sdk-library/sdk-building.md) to get started.

## Key Features

- **Pay-per-query data access** — charge a flat fee, per row returned, or a one-time fee for table metadata
- **Tiered pricing** — volume tiers, multiple tokens, and multiple networks per table
- **Multiple databases** — DuckDB, PostgreSQL, and ClickHouse backends
- **CLI and SDK** — run from a YAML config file (no code) or embed as a Rust/Python library
- **Apache Arrow responses** — efficient binary columnar format, significantly faster than JSON
- **Safe SQL subset** — parser blocks JOINs, GROUP BY, subqueries, and other expensive operations
- **Embedded dashboards** — scaffold and serve Evidence dashboards from the same binary, with x402 wallet-connect download buttons baked in
- **Hot reload** — tables, pricing, facilitator settings, and dashboard config reload on file change without a restart
- **Observability** — built-in OpenTelemetry tracing support

## How Paid Requests Work

1. A client sends a SQL query to `POST /api/query`.
2. The server parses and validates the query, then estimates the payment options.
3. If payment is required, the server responds with HTTP 402 and a list of payment options.
4. The client signs a payment using their crypto wallet and resends the request with a `Payment-Signature` header.
5. The server verifies and settles the payment through a facilitator, then returns the query results as Arrow IPC.

## Endpoints at a Glance

| Path | Purpose |
|------|---------|
| `GET /` | Dashboards landing page (only when `dashboards:` is configured) |
| `GET /<slug>/` | Static Evidence dashboard, one per `dashboards:` entry |
| `GET /api/` | Server discovery document: tables, pricing, endpoints, version |
| `POST /api/query` | Submit a SQL query (paywalled per the table's price tags) |
| `GET /api/table/{name}` | Full schema and pricing for a single table |

## Project Structure

```
tiders-x402-server/
  server/          # Rust server library + CLI binary (Axum-based REST API + dashboard scaffolder)
    src/
      cli/           # YAML config loader, YAML validator, file watcher
      dashboard/     # Dashboard config, routes, scaffolder + embedded Evidence templates
      database/      # Database trait + DuckDB / PostgreSQL / ClickHouse backends + SQL parser/generators
      payment/       # Pricing model, payment config, x402 verify/settle, facilitator client
      handler_api_*.rs # Axum handlers for /api/, /api/query, /api/table/{name}
      lib.rs         # Server bootstrap (router, middleware, OTLP, graceful shutdown)
  cli-python/      # Python wheel that ships the CLI binary (pip install tiders-x402-server)
  examples/        # Python, Rust, and CLI server examples + sample data
  docs/            # mdBook documentation
  Cargo.toml       # Workspace configuration
```

## Technology Stack

| Component | Technology |
|-----------|-----------|
| Web framework | Axum |
| Database | DuckDB, ClickHouse, PostgreSQL |
| Payment protocol | x402 V2 (via `x402-rs` types and `x402-chain-eip155`) |
| Data serialization | Apache Arrow IPC |
| SQL parsing | `sqlparser` |
| Blockchain primitives | Alloy |
| Dashboards | Evidence (Svelte) + wagmi + viem + `@x402/evm` |
| Observability | OpenTelemetry + tracing |
