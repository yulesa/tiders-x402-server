# Server Library

The server library (`server/src/lib.rs`) is the entry point of the server. It sets up and runs the Axum HTTP server for the Tiders x402 service. It wires together routing, shared state, tracing, and graceful shutdown.

## AppState

`AppState` is the shared context every request handler can access. It holds the resources handlers need to do their work, plus the lock-free swap handles used for hot reload.

```rust
pub struct AppState {
    pub db: Arc<dyn Database>,
    pub payment_config: Arc<tokio::sync::RwLock<Arc<GlobalPaymentConfig>>>,
    pub server_base_url: Url,
    pub server_bind_address: String,
    pub dashboards: Arc<ArcSwap<DashboardsState>>,
    pub dashboard_router: Arc<ArcSwap<Router>>,
}
```

- **`db`** — The database backend (DuckDB, Postgres, ClickHouse, etc.) behind a trait object.
- **`payment_config`** — The global payment configuration: tables, pricing rules, facilitator settings. Wrapped in `RwLock<Arc<…>>` so the file watcher can hot-swap it without blocking handlers. See [Payment Configuration](./payment-config.md).
- **`server_base_url`** — The server's public URL, used for building `resource.url` fields in payment requirements.
- **`server_bind_address`** — The host:port the listener binds to (e.g. `0.0.0.0:4021`).
- **`dashboards`** — The current set of configured Evidence dashboards, behind `arc-swap` for lock-free reads/swaps.
- **`dashboard_router`** — The Axum sub-router that serves `/<slug>/...` routes for each dashboard. Replaced atomically by the watcher when the `dashboards:` block changes.

`AppState::new` accepts a `DashboardsState` directly — pass an empty one if you don't want to serve dashboards.

## Router

The router is built in two layers:

| Layer | Mounts | Description |
|-------|--------|-------------|
| API sub-router (`/api/*`) | `GET /api/`, `GET /api/query`, `GET /api/table/{name}` | Always mounted |
| Landing page (`GET /`) | `landing_handler` | Only mounted when at least one dashboard is configured |
| Dashboard fallback service | everything else | A `tower::Service` that reads the current dashboard router from `arc-swap` and forwards the request. Lock-free, so config reloads don't block in-flight requests |

The fallback service is what makes hot reload of `dashboards:` cheap: when the YAML changes, the watcher rebuilds a fresh `Router` for the new dashboard list and stores it via `arc-swap`. The next request reads the new pointer; in-flight requests keep using the old router until they finish.

## Telemetry

The server emits structured logs via `tracing` and supports OpenTelemetry export over OTLP/gRPC:

- Set `OTEL_EXPORTER_OTLP_ENDPOINT` to enable OTLP export. When unset, only console logging is active.
- Set `OTEL_SERVICE_NAME` to override the service name (defaults to `tiders-x402`).

`GET /api/query` requests are wrapped in a dedicated `api_query` span with method, URI, and HTTP status; all other requests fall back to a `http_request` debug span. Span status is set to `Status::error` when the response is 4xx/5xx and `Status::Ok` otherwise.

## Middleware

Every request passes through `tower_http::TraceLayer` before reaching its handler. The layer logs each request's method, path, response status, and latency, and records HTTP status as a span attribute for OpenTelemetry export.

## Graceful Shutdown

When the server receives Ctrl+C (or SIGTERM on Unix), it stops accepting new connections but lets in-flight requests finish before exiting. Pending OpenTelemetry spans are flushed on tracer-provider drop, so deploys and container restarts don't drop traces or truncate responses.

## Module Map

`lib.rs` declares the following public modules:

| Module | Path | Purpose |
|--------|------|---------|
| `cli` | `cli/` | YAML config loading + CLI entry (gated by the `cli` feature) |
| `dashboard` | `dashboard/` | Dashboard config, routes, and scaffolder |
| `database` | `database/` | `Database` trait, per-backend impls, SQL parser, SQL generators |
| `payment` | `payment/` | Pricing model, payment config, verify/settle, facilitator client |
| `handler_api_root` | `handler_api_root.rs` | `GET /api/` |
| `handler_api_query` | `handler_api_query.rs` | `GET /api/query` |
| `handler_api_table_detail` | `handler_api_table_detail.rs` | `GET /api/table/{name}` |

Top-level re-exports for SDK ergonomics: `Database`, `GlobalPaymentConfig`, `FacilitatorClient`, `PriceTag`, `PricingModel`, `TablePaymentOffers`.
