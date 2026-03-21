# Running the Server

## Rust

Build and run directly:

```bash
cargo run --release
```

The server starts on the configured bind address (default `0.0.0.0:4021`).

## Python

Build the Python bindings with maturin, then run:

```bash
cd python
maturin develop
python example/duckdb_server.py
```

## Verifying the Server

Once running, check the root endpoint:

```bash
curl http://localhost:4021/
```

This returns usage information, available tables, their schemas, and SQL parser rules.

## Environment Variables

The server loads `.env` files via `dotenvy`. OpenTelemetry tracing is configured automatically -- set standard OTEL environment variables to export traces:

```bash
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
OTEL_SERVICE_NAME=tiders-x402-server
```

## Graceful Shutdown

The server handles `SIGTERM` and `Ctrl+C` for graceful shutdown, completing in-flight requests before stopping.
