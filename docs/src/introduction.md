![Tiders](resources/tiders_logo2.png)

# Tiders x402 Server

Tiders x402 Server is a **payment-enabled database API server** that combines [DuckDB](https://duckdb.org/) with the [x402 payment protocol](https://www.x402.org/) to create a pay-per-query data marketplace.

Data providers can expose DuckDB tables through a REST API where each query requires a cryptocurrency micropayment. Pricing is calculated per row returned, with support for tiered pricing based on result size.

## Two Ways to Use

| Mode | How | When to use |
|------|-----|-------------|
| **CLI** | Write a YAML config file, run `tiders-x402-server start` | Quick setup, no coding required, config-driven deployments |
| **SDK Library** | Import in Rust or Python, configure programmatically | Custom logic, embedding in larger applications, maximum flexibility |

See [CLI Quick Start](./getting-started/cli-quickstart.md) or [SDK Library Overview](./sdk-library/sdk-building.md) to get started.

## Key Features

- **Pay-per-query data access** -- Charge users fixed or per row of data returned using cryptocurrency micropayments.
- **x402 protocol integration** -- Standard HTTP 402 Payment Required flow with automatic payment negotiation.
- **Multiple databases** -- DuckDB, PostgreSQL, and ClickHouse backends.
- **CLI and SDK** -- Run from a YAML config file (no code) or embed as a Rust/Python library.
- **Apache Arrow responses** -- Efficient columnar data transfer using Arrow IPC format instead of JSON.
- **Tiered pricing** -- Multiple price tiers based on the number of rows requested (e.g., bulk discounts).
- **Multi-language support** -- Rust server with Python bindings (via PyO3).
- **Familiar SQL for Requests** -- A safe SQL subset that prevents expensive operations like JOINs, GROUP BY, and subqueries.

## How It Works

1. A client sends a SQL query to the server.
2. The server parses and validates the query, then estimates the payment options.
3. If payment is required, the server responds with HTTP 402 and payment options.
4. The client signs a payment using their crypto wallet and resends the request with an `X-Payment` header.
5. The server verifies and settles the payment through a facilitator, then returns the query results as Arrow IPC.

## Project Structure

```
tiders-x402-server/
  server/          # Rust server library (Axum-based REST API)
  cli/             # CLI binary — run a server from a YAML config file
  python/          # Python bindings via PyO3 + maturin
  examples/        # Python, Rust, and CLI server examples
  client-scripts/  # Python and TypeScript client scripts using x402-fetch
  docs/            # MDbook documentation page
  Cargo.toml       # Workspace configuration
```

## Technology Stack

| Component | Technology |
|-----------|-----------|
| Web framework | Axum |
| Database | DuckDB, ClickHouse, PostgreSQL |
| Async runtime | Tokio |
| Payment protocol | x402 (via `x402-rs`) |
| Data serialization | Apache Arrow IPC |
| SQL parsing | `sqlparser` |
| Blockchain primitives | Alloy |
| Observability | OpenTelemetry + tracing |
| Python FFI | PyO3 |
