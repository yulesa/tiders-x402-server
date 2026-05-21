# CLI Quick Start

The CLI is the fastest way to run a tiders-x402-server — no code required. Write a YAML config file, point the CLI at it, and the server is live.

If you just want to try the server, head to the CLI at `examples/CLI` and run the example.

Tiders-x402-server assumes you already have a database populated with the data you want to sell. If you don't, the [Tiders ingestion tool](https://github.com/yulesa/tiders) can help you stand one up and load it with crypto data — see [Choosing a Database](https://yulesa.github.io/tiders-docs/getting_started/choosing_a_database.html) for guidance on picking a backend.

## 1. Install

```bash
pip install tiders-x402-server
# or
cargo install tiders-x402-server
```

Both commands install the same `tiders-x402-server` binary with DuckDB, PostgreSQL, and ClickHouse backends bundled.

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

dashboard: # Optional
  entries:
    - slug: my_dashboard
      title: "My Dashboard"
      description: "Description text for the dashboard"
      tags: ["Tag1", "Tag2"]
```

This is a minimal config. See the [YAML Configuration Reference](../cli/yaml-reference.md) for all options.

## 3. Environment Variables

Use `${VAR_NAME}` placeholders anywhere in the YAML to keep secrets and environment-specific values out of your config file. This works for any string field — provider URLs, credentials, file paths, etc.

```yaml
database:
  postgresql:
    connection_string: "host=${PG_HOST} user=${PG_USER} password=${PG_PASSWORD} dbname=tiders"
```

At startup, the CLI automatically loads a `.env` file from the current working directory (and parents), then substitutes all `${VAR_NAME}` placeholders with their values. If a variable is referenced in the YAML but not defined, the CLI raises an error.

Create a `.env` file alongside your config:

```bash
PG_HOST=localhost
PG_USER=postgres
PG_PASSWORD=secret
```

You can also point to a different `.env` file using `--env-file`:

```bash
tiders-x402-server start --env-file /path/to/.env
```

## 4. Validate (optional)

Before starting, check that the config parses and the database is reachable:

```bash
tiders-x402-server validate
```

On success it logs the number of registered tables; on failure it prints a descriptive error and exits non-zero.

## 5. Start the Server

```bash
# Auto-discovers the config file in the current directory
tiders-x402-server start

# Or specify the path explicitly
tiders-x402-server start path/to/config.yaml
```

The CLI auto-discovers `.yaml`/`.yml` files in the current directory that contain the required top-level keys (`server`, `facilitator`, `database`). If exactly one candidate is found, it is used automatically.

By default the CLI watches the config file for changes and hot-reloads tables, pricing, facilitator settings, and dashboard configuration without restarting. Disable this with `--no-watch`.

## 6. Verify

```bash
curl http://localhost:4021/api/
```

You should get a JSON discovery document listing your tables, pricing tiers, and endpoints.

To run a query:

```bash
curl --get http://localhost:4021/api/query \
  --data-urlencode "query=SELECT * FROM my_table LIMIT 10"
# Returns 402 with payment options
```

Use one of the [client scripts](https://github.com/yulesa/tiders-x402-server/tree/main/client-scripts) (Python or TypeScript) to handle the x402 payment flow end-to-end.

## 7. Create the dashboards (optional)

You can create dashboards before or after starting the server. Scaffold all dashboards defined in the YAML at once, or a specific one by slug:

```bash
tiders-x402-server dashboard          # scaffold all entries
tiders-x402-server dashboard <slug>   # scaffold one
```

This copies a minimal [Evidence](https://evidence.dev/) project template into `<dashboards>/<slug>/`. From there, edit the files, mainly `<dashboards>/<slug>/pages/index.md`, to build your reports — the [Evidence docs](https://docs.evidence.dev/) cover the full dashboard authoring workflow.

> **Note:** Data visible in the dashboard can be scraped freely without payment. Only expose data you are comfortable sharing publicly, and leave anything sensitive behind the paid API instead.

Once the dashboard is ready, build it into a static site:

```bash
(cd <dashboards>/<slug> && npm install && npm run build)
```

The server will pick up and serve the built files automatically. Dashboards are static — they do not update live. Rebuild whenever the underlying data changes.

## Next Steps

- [CLI Overview](../cli/cli-overview.md) — commands, auto-discovery, hot-reload, logging
- [YAML Configuration Reference](../cli/yaml-reference.md) — all config sections and options
- [Dashboards](./dashboards.md) — scaffold and serve Evidence dashboards from the same binary
- [API Endpoints](../api/endpoints.md) — the HTTP API exposed by the server
- [Payment Flow](./payment-flow.md) — how the x402 payment protocol works
