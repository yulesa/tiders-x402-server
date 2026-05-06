# YAML Configuration Reference

Complete reference for the tiders-x402-server CLI configuration file. The config uses five top-level sections: `server`, `facilitator`, `database`, `payment`, and `tables`.

Unknown fields are rejected -- the CLI will report an error if a key is misspelled or unsupported.

---

## Server

Required. Network configuration for the HTTP server.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `bind_address` | string | yes | Address and port to bind (e.g., `"0.0.0.0:4021"`) |
| `base_url` | string | yes | Public URL of the server, used in x402 payment responses (e.g., `"https://api.example.com"`) |

```yaml
server:
  bind_address: "0.0.0.0:4021"
  base_url: "http://localhost:4021"
```

---

## Facilitator

Required. Configuration for the x402 facilitator service that handles payment verification and settlement.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `url` | string | yes | Facilitator endpoint (e.g., `"https://facilitator.x402.rs"`) |
| `timeout` | integer | no | Request timeout in seconds |
| `headers` | map | no | Custom HTTP headers sent with every facilitator request |

```yaml
facilitator:
  url: "https://facilitator.x402.rs"
  timeout: 30
  headers:
    X-Api-Key: "${FACILITATOR_API_KEY}"
```

---

## Database

Required. Database backend configuration. Exactly one backend must be specified.

**DuckDb**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | yes | Path to the DuckDB database file |

```yaml
database:
  duckdb:
    path: "./data/my_data.duckdb"
```

**PostgreSQL**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `connection_string` | string | yes | PostgreSQL connection string |

```yaml
database:
  postgresql:
    connection_string: "host=${PG_HOST} port=5432 user=${PG_USER} password=${PG_PASSWORD} dbname=tiders"
```

**Clickhouse**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `url` | string | yes | ClickHouse HTTP endpoint |
| `user` | string | no | Database user |
| `password` | string | no | Database password |
| `database` | string | no | Database name |
| `access_token` | string | no | Access token for authentication |
| `compression` | string | no | Compression mode: `"none"` or `"lz4"` |

```yaml
database:
  clickhouse:
    url: "http://${CLICKHOUSE_HOST}:${CLICKHOUSE_PORT}"
    user: "${CLICKHOUSE_USER}"
    password: "${CLICKHOUSE_PASSWORD}"
    database: "${CLICKHOUSE_DB}"
    compression: "lz4"
```

---

## Payment

**Optional.** Global payment settings. All fields have defaults.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `max_timeout_seconds` | integer | `300` | How long a payment offer remains valid (seconds) |
| `default_description` | string | `"Query execution payment"` | Fallback description for tables without their own |

```yaml
payment:
  max_timeout_seconds: 300
  default_description: "Query execution payment"
```

---

## Tables

**Optional.** List of tables exposed by the server. Each table is auto-discovered from the database; the config defines pricing and descriptions.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | yes | Table name in the database |
| `description` | string | no | Human-readable description |
| `price_tags` | list | no | Pricing tiers. Empty or absent means the table is free |

```yaml
tables:
  - name: my_table
    description: "My dataset"
    price_tags:
      - type: per_row
        pay_to: "0xYourAddress"
        token: usdc/base_sepolia
        amount_per_item: "0.002"
        is_default: true
```

### Price Tag Types

Price tags use a `type` field to select the pricing model. Three types are supported:

**Per Row**

Price scales linearly with the number of rows returned. Supports tiered pricing via `min_items` / `max_items`.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `pay_to` | string | yes | Recipient wallet address |
| `token` | string | yes | Token identifier (see below) |
| `amount_per_item` | string | yes | Price per row as a human-readable decimal (e.g., `"0.002"`) |
| `min_items` | integer | no | Minimum row count for this tier to apply |
| `max_items` | integer | no | Maximum row count for this tier to apply |
| `min_total_amount` | string | no | Minimum total charge, even if per-row calculation is lower |
| `description` | string | no | Label for this tier |
| `is_default` | boolean | no | Whether this is the default tier (default: `false`) |

```yaml
# Default tier: $0.002 per row
- type: per_row
  pay_to: "0xYourAddress"
  token: usdc/base_sepolia
  amount_per_item: "0.002"
  is_default: true

# Bulk tier: $0.001 per row for 100+ rows
- type: per_row
  pay_to: "0xYourAddress"
  token: usdc/base_sepolia
  amount_per_item: "0.001"
  min_items: 100
```

**Fixed**

A flat fee regardless of how many rows are returned.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `pay_to` | string | yes | Recipient wallet address |
| `token` | string | yes | Token identifier (see below) |
| `amount` | string | yes | Fixed amount as a human-readable decimal (e.g., `"1.00"`) |
| `description` | string | no | Label for this tier |
| `is_default` | boolean | no | Whether this is the default tier (default: `false`) |

```yaml
- type: fixed
  pay_to: "0xYourAddress"
  token: usdc/base_sepolia
  amount: "1.00"
  description: "Fixed price query"
```

**Metadata Price**

A flat fee for accessing table metadata (schema and payment offers) via the `GET /api/table/{name}` endpoint. Without this tag, metadata is returned freely. Charging for the metadata API calls can be used to prevent API abuse.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `pay_to` | string | yes | Recipient wallet address |
| `token` | string | yes | Token identifier (see below) |
| `amount` | string | yes | Fixed amount as a human-readable decimal (e.g., `"0.01"`) |
| `description` | string | no | Label for this tier |
| `is_default` | boolean | no | Whether this is the default tier (default: `false`) |

```yaml
- type: metadata_price
  pay_to: "0xYourAddress"
  token: usdc/base_sepolia
  amount: "1.00"
  description: "Metadata access fee"
```

### Token Identifiers

Token identifiers use the format `token_name/network`. Supported tokens:

| Identifier | Token | Network |
|-----------|-------|---------|
| `usdc/base_sepolia` | USDC | Base Sepolia (testnet) |
| `usdc/base` | USDC | Base |
| `usdc/avalanche_fuji` | USDC | Avalanche Fuji (testnet) |
| `usdc/avalanche` | USDC | Avalanche |
| `usdc/polygon` | USDC | Polygon |
| `usdc/polygon_amoy` | USDC | Polygon Amoy (testnet) |

See also [`examples/cli/`](https://github.com/yulesa/tiders-x402-server/tree/main/examples/cli) for a ready-to-use config and `.env.example`.

---

## Dashboards

**Optional.** Configures Evidence dashboards served by the server. Each entry is served as a static site at `/<slug>/`. Use `tiders-x402-server dashboard <slug>` to scaffold the Evidence project on disk.

### Top-level fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `root` | string | `./dashboards/` | Root directory where dashboard project folders are scaffolded (resolved relative to the config file) |
| `entries` | list | `[]` | List of dashboard definitions |

```yaml
dashboards:
  root: "./dashboards"   # optional
  entries:
    - slug: my-dashboard
      title: "My Dashboard"
      description: "A short description shown on the landing page."
      tags: ["DeFi", "Ethereum"]
```

### Entry fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `slug` | string | yes | URL-safe identifier; becomes the route prefix `/<slug>/`. Must match `^[a-z0-9][a-z0-9_-]*$`. Reserved: `api`, `assets`, `static` |
| `title` | string | yes | Human-readable name shown on the landing page |
| `description` | string | no | One-line description shown under the title on the landing page card |
| `tags` | list | no | Labels rendered as pills on the landing page card (e.g., `["DeFi", "Ethereum"]`) |
| `disabled` | boolean | no | Set to `true` to exclude this dashboard from the server without removing the entry (default: `false`) |
| `folder_path` | string | no | Path where the Evidence project will be scaffolded. Defaults to `<root>/<slug>/`. Resolved relative to the config file |
| `build_path` | string | no | Path to the built static site served at runtime. Defaults to `<folder_path>/build`. Resolved relative to the config file |

```yaml
dashboards:
  entries:
    - slug: uniswap_v3
      title: "Uniswap V3"
      description: "Pool swaps and liquidity events on Uniswap V3."
      tags: ["Dex", "DeFi", "Ethereum"]
      # disabled: true
      # folder_path: "./dashboards/uniswap_v3"
      # build_path: "./dashboards/uniswap_v3/build"
```

### Scaffolding and serving

Scaffold a dashboard project with:

```bash
tiders-x402-server dashboard uniswap_v3        # scaffold one dashboard
tiders-x402-server dashboard                    # scaffold all dashboards
tiders-x402-server dashboard uniswap_v3 --force # overwrite managed files
```

After scaffolding, build the Evidence project:

```bash
cd dashboards/uniswap_v3 && npm install && npm run build
```

Then start the server — the built site is served automatically at `/<slug>/`.
