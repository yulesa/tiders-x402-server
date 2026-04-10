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

A flat fee for accessing table metadata (schema and payment offers) via the `GET /table/:name` endpoint. Without this tag, metadata is returned freely. Charging for the metadata API calls can be used to prevent API abuse.

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
