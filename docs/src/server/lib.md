# Server Library

The server library (`server/src/lib.rs`) is the entry point of the server. It sets up and runs the Axum HTTP server for the Tiders x402 payment-gated data service. It wires together routing, shared state, tracing, and graceful shutdown.

## AppState

`AppState` is the shared context with data of the whole server and that every request handler has access to. It holds the resources that handlers need to do their work:

```rust
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub payment_config: Arc<GlobalPaymentConfig>,
}
```

- **`db`** — The database connection used to execute queries against DuckDB. Access is serialized (one query at a time), but DuckDB is fast enough that this is not a bottleneck for typical workloads.
- **`payment_config`** — The global payment configuration, including which tables require payment, pricing rules, and facilitator settings. See [Payment Configuration](./payment-config.md).

## Router

The server exposes two routes (API entry points):

| Method | Path     | Handler          | Description |
|--------|----------|------------------|-------------|
| `GET`  | `/`      | `root_handler`   | Returns server metadata and available data offers |
| `POST` | `/query` | `query_handler`  | Accepts SQL queries with x402 payment support |

## Telemetry

The server supports exporting traces to an external observability backend (e.g., Jaeger, Grafana Tempo) via the OpenTelemetry protocol (`opentelemetry_otlp`). This is opt-in: set the `OTEL_EXPORTER_OTLP_ENDPOINT` environment variable to enable it, and optionally `OTEL_SERVICE_NAME` to customize the service name (defaults to `"tiders-x402"`). When not configured, logs are written to the console only.

## Middleware

Every request passes through a tracing middleware layer (`tower_http::TraceLayer`) before reaching the handler. Its role is observability: it automatically logs each request's method, path, response status, and latency so operators can monitor the server without adding logging code to every handler.


## Graceful Shutdown

When the server receives a stop signal (Ctrl+C or a container orchestrator's termination signal), it stops accepting new connections but lets any in-flight requests finish before exiting. This prevents clients from seeing abrupt connection drops during deployments or restarts.

## Module Map

`lib.rs` declares the following public modules:

| Module | Purpose |
|--------|---------|
| `root_handler` | `GET /` handler |
| `query_handler` | `POST /query` handler |
| `price` | Per-row pricing logic |
| `payment_config` | Global payment configuration |
| `payment_processing` | Payment verify/settle orchestration |
| `facilitator_client` | x402 facilitator HTTP client |
| `sqp_parser` | SQL parsing and validation |
| `database` | DuckDB execution and Arrow IPC serialization |
| `duckdb_reader` | DuckDB query construction |
