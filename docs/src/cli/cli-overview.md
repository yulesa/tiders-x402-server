# CLI Overview

The `tiders-x402-server` CLI runs a payment-enabled database API server from a YAML configuration file. No Rust or Python code is needed -- define your database, tables, pricing, and facilitator in YAML and the CLI handles the rest.

## Commands

**`start`**

Starts the server.

```bash
tiders-x402-server start [CONFIG] [--no-watch]
```

| Argument | Description |
|----------|-------------|
| `CONFIG` | Path to the YAML config file. If omitted, auto-discovers a config in the current directory. |
| `--no-watch` | Disable automatic config file watching (hot reload). |

**`validate`**

Validates the config file and tests database connectivity, then exits. Useful for CI or pre-deploy checks.

```bash
tiders-x402-server validate [CONFIG]
```

| Argument | Description |
|----------|-------------|
| `CONFIG` | Path to the YAML config file. If omitted, auto-discovers a config in the current directory. |

On success, prints the number of registered tables and confirms the database is reachable. On failure, prints a descriptive error and exits with a non-zero code.

## Environment Variables

YAML values can reference environment variables using `${VAR_NAME}` syntax. Variables are expanded before the YAML is parsed.

```yaml
database:
  clickhouse:
    url: "http://${CLICKHOUSE_HOST}:${CLICKHOUSE_PORT}"
    user: "${CLICKHOUSE_USER}"
    password: "${CLICKHOUSE_PASSWORD}"
```
The CLI automatically loads a .env file from the same directory as the config file before substitution. Use --env-file to point to a different location:

```bash
tiders-x402-server start --env-file /path/to/.env
```

If a referenced variable is not set, the CLI exits with an error listing all missing variables.

See [`examples/cli/.env.example`](https://github.com/yulesa/tiders-x402-server/blob/main/examples/cli/.env.example) for a template.

## Hot Reload

By default, the CLI watches the config file for changes. When a modification is detected, it reloads:

- **Tables** -- added, removed, or modified table definitions and pricing tiers
- **Payment settings** -- `max_timeout_seconds`, `default_description`
- **Facilitator** -- URL, timeout, and headers

The **database connection is not reloaded** -- changing the database backend requires a restart.

Hot reload is enabled by default. Disable it with `--no-watch`:

```bash
tiders-x402-server start --no-watch
```

## Logging

The CLI uses [tracing](https://crates.io/crates/tracing) for structured logging. Control the log level with the `RUST_LOG` environment variable:

```bash
# Default level is info
RUST_LOG=info tiders-x402-server start

# Debug logging
RUST_LOG=debug tiders-x402-server start

# Quiet (errors only)
RUST_LOG=error tiders-x402-server start
```

OpenTelemetry tracing is also supported. See [Observability](../getting-started/server-overview.md) for details.
