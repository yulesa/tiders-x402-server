# Architecture Overview

## System Components

```
                           +-------------------+
                           |   x402 Facilitator|
                           | (verify / settle) |
                           +--------^----------+
                                    |
+----------+    HTTP    +-----------+-----------+    DuckDB
|  Client   +---------->+   Tiders x402 Server  +----------+
| (TS/Py)  <------------+                       |   (data)  |
+----------+   Arrow    |  +--query_handler--+  |           |
               IPC      |  |  sqp_parser     |  |           |
                        |  |  duckdb_reader  |  |           |
                        |  |  payment_config |  |           |
                        |  +-----------------+  |           |
                        +-----------------------+-----------+
```

## Module Structure

The server is organized into the following modules:

| Module | File | Purpose |
|--------|------|---------|
| `query_handler` | `query_handler.rs` | Main HTTP handler for `/` and `/query` routes |
| `sqp_parser` | `sqp_parser.rs` | SQL parser that validates and analyzes SELECT queries |
| `duckdb_reader` | `duckdb_reader.rs` | Converts analyzed queries to DuckDB SQL and executes them |
| `payment_config` | `payment_config.rs` | Configuration for table pricing and payment requirements |
| `price` | `price.rs` | `PriceTag` and `TablePaymentOffers` data structures |
| `facilitator_client` | `facilitator_client.rs` | HTTP client for the remote x402 facilitator |
| `payment_processing` | `payment_processing.rs` | Helper functions for verify/settle operations |
| `database` | `database.rs` | Query execution and Arrow IPC serialization |

## Request Lifecycle

1. **Axum** receives the HTTP request and routes it to `query_handler`.
2. **sqp_parser** parses and validates the SQL (rejects unsafe operations).
3. **duckdb_reader** converts the analyzed query to a DuckDB-compatible SQL string.
4. **payment_config** determines pricing based on the table and estimated row count.
5. **facilitator_client** handles payment verification and settlement with the remote facilitator.
6. **database** executes the query and serializes results to Arrow IPC.

## AppState

The shared application state is passed to all handlers via Axum's state extraction:

```rust
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub payment_config: Arc<GlobalPaymentConfig>,
}
```

- `db` -- DuckDB connection wrapped in `Arc<Mutex<>>` for thread-safe access.
- `payment_config` -- Immutable payment configuration shared across handlers.

## Dependencies

Key crate dependencies:

- **axum** -- HTTP server framework
- **duckdb** -- In-process SQL database
- **arrow** -- Apache Arrow for columnar data
- **x402-rs** -- x402 payment protocol types and utilities
- **sqlparser** -- SQL AST parser
- **alloy** -- EVM/blockchain primitives
- **reqwest** -- HTTP client for facilitator communication
- **tokio** -- Async runtime
- **tracing** / **opentelemetry** -- Observability
