# CLI Quick Start

The fastest way to run a tiders-x402-server -- no code required. Write a YAML config file, point the CLI at it, and the server is live.

## 1. Install

```bash
cargo install tiders-x402-server-cli
```

> **Note:** The crate is not yet published. For now, build from source:
> ```bash
> cargo build -p tiders-x402-server-cli --release
> ```

## 2. Create a Config File

Create a file called `tiders-x402-server.yaml`:

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
```

This is a minimal config. See the [YAML Configuration Reference](../cli/yaml-reference.md) for all options.

## 3. Environment Variables

Use ${VAR_NAME} placeholders anywhere in the YAML to keep secrets and environment-specific values out of your config file. This works for any string field — provider URLs, credentials, file paths, etc.

```yaml
database:
  postgresql:
    connection_string: "host=${PG_HOST} user=${PG_USER} password=${PG_PASSWORD} dbname=tiders"
```

At startup, the CLI automatically loads a `.env` file from the same directory as the config file, then substitutes all `${VAR_NAME}` placeholders with their values. If a variable is referenced in the YAML but not defined, the CLI raises an error.

Create a `.env` file alongside your config:

```bash
PG_HOST=localhost
PG_USER=postgres
PG_PASSWORD=secret
```

You can also point to a different `.env` file using the `--env-file` flag:

```bash
tiders-x402-server start --env-file /path/to/.env
```

## 4. Start the Server

```bash
# Auto-discovers the config file in the current directory
tiders-x402-server start

# Or specify the path explicitly
tiders-x402-server start path/to/config.yaml
```

The CLI auto-discovers `.yaml`/`.yml` files in the current directory that contain the required top-level keys (`server`, `facilitator`, `database`). If exactly one candidate is found, it is used automatically.

By default, the CLI watches the config file for changes and hot-reloads payment configuration (tables, pricing, facilitator settings) without restarting. Disable this with `--no-watch`.

## Next Steps

- [CLI Overview](../cli/cli-overview.md) -- commands, auto-discovery, hot-reload, and logging
- [YAML Configuration Reference](../cli/yaml-reference.md) -- all config sections and options
- [API Endpoints](../api/endpoints.md) -- the HTTP API exposed by the server
- [Payment Flow](./payment-flow.md) -- how the x402 payment protocol works
