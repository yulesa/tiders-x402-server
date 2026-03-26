# Configuration Reference

The server is configured programmatically -- there are no config files. You set up payment rules, database connections, and server parameters in code. All configuration objects provide both Rust functions and Python bindings.

## AppState

The shared application state accessible by every request handler. Holds the database connection, payment configuration, and server URL.

| Field | Type | Description |
|-------|------|-------------|
| `db` | `Arc<dyn Database>` | Database backend (DuckDB, Postgres, ClickHouse) |
| `payment_config` | `Arc<GlobalPaymentConfig>` | Global payment configuration |
| `server_base_url` | `Url` | Server's public URL, used for binding and resource URLs in payment requirements |

**Construction**

```rust
// Rust
let state = AppState {
    db: Arc::new(db),
    payment_config: Arc::new(config),
    server_base_url: Url::parse("http://0.0.0.0:4021").unwrap(),
};
```

```python
# Python
state = AppState(database, payment_config, "http://0.0.0.0:4021")
```

**Getters**

| Getter | Rust | Python | Returns |
|--------|------|--------|---------|
| Server base URL | `state.server_base_url` (pub field) | `state.server_base_url` | `Url` / `str` |

**Setters**

| Setter | Rust | Python |
|--------|------|--------|
| Set server base URL | `state.server_base_url = Url::parse("...").unwrap()` | `state.set_server_base_url("http://0.0.0.0:4021")` |

---

## FacilitatorClient

HTTP client for communicating with a remote x402 facilitator. Wraps a base URL and derives `/verify`, `/settle`, and `/supported` endpoints automatically.

**Construction**

```rust
// Rust
let facilitator = FacilitatorClient::try_from("https://facilitator.x402.rs")
    .expect("Failed to create facilitator client");
```

```python
# Python
facilitator = FacilitatorClient("https://facilitator.x402.rs")
```

**Getters**

| Getter | Rust | Python | Returns |
|--------|------|--------|---------|
| Base URL | `facilitator.base_url()` | `facilitator.base_url` | `&Url` / `str` |
| Verify URL | `facilitator.verify_url()` | `facilitator.verify_url` | `&Url` / `str` |
| Settle URL | `facilitator.settle_url()` | `facilitator.settle_url` | `&Url` / `str` |
| Headers | `facilitator.headers()` | -- | `&HeaderMap` |
| Timeout | `facilitator.timeout()` | `facilitator.timeout_ms` | `&Option<Duration>` / `Optional[int]` (ms) |

**Setters**

| Setter | Rust | Python |
|--------|------|--------|
| Set custom headers | `facilitator.with_headers(header_map)` | `facilitator.set_headers({"Authorization": "Bearer ..."})` |
| Set timeout | `facilitator.with_timeout(Duration::from_millis(5000))` | `facilitator.set_timeout(5000)` |

---

## GlobalPaymentConfig

Central payment configuration shared across all request handlers. Holds the facilitator client, response format, timeout, and per-table pricing rules.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `facilitator` | `Arc<FacilitatorClient>` | -- | Client for the x402 facilitator service |
| `mime_type` | `String` | `application/vnd.apache.arrow.stream` | Response MIME type |
| `max_timeout_seconds` | `u64` | `300` | How long a payment offer remains valid |
| `default_description` | `String` | `Query execution payment` | Fallback description for tables without their own |
| `offers_tables` | `HashMap<String, TablePaymentOffers>` | empty | Per-table payment configuration |

**Construction**

All fields except `facilitator` are optional and fall back to sensible defaults.

```rust
// Rust - with defaults
let config = GlobalPaymentConfig::default(Arc::new(facilitator));

// Rust - with custom values
let config = GlobalPaymentConfig::new(
    Arc::new(facilitator),
    Some("text/csv".to_string()),       // mime_type
    Some(600),                           // max_timeout_seconds
    Some("Custom description".to_string()), // default_description
    None,                                // offers_tables (defaults to empty)
);
```

```python
# Python - with defaults
config = GlobalPaymentConfig(facilitator)

# Python - with custom values
config = GlobalPaymentConfig(
    facilitator,
    mime_type="text/csv",
    max_timeout_seconds=600,
    default_description="Custom description",
)
```

**Getters**

| Getter | Rust | Python | Returns |
|--------|------|--------|---------|
| MIME type | `config.mime_type` (pub field) | `config.mime_type` | `String` / `str` |
| Max timeout | `config.max_timeout_seconds` (pub field) | `config.max_timeout_seconds` | `u64` / `int` |
| Default description | `config.default_description` (pub field) | `config.default_description` | `String` / `str` |
| Get table offers | `config.get_offers_table("table")` | -- | `Option<&TablePaymentOffers>` |
| Table requires payment | `config.table_requires_payment("table")` | `config.table_requires_payment("table")` | `Option<bool>` |

**Setters**

| Setter | Rust | Python |
|--------|------|--------|
| Set facilitator | `config.set_facilitator(arc_client)` | `config.set_facilitator(facilitator)` |
| Set MIME type | `config.set_mime_type("text/csv".to_string())` | `config.set_mime_type("text/csv")` |
| Set max timeout | `config.set_max_timeout_seconds(600)` | `config.set_max_timeout_seconds(600)` |
| Set default description | `config.set_default_description("...".to_string())` | `config.set_default_description("...")` |
| Add table offers | `config.add_offers_table(offer)` | `config.add_offers_table(offer)` |

---

## PriceTag

A single pricing tier for a table. Defines who gets paid, how much per row, and in which token. A table can have multiple price tags for tiered pricing.

| Field | Type | Description |
|-------|------|-------------|
| `pay_to` | `ChecksummedAddress` | Recipient wallet address |
| `amount_per_item` | `TokenAmount` | Price per row in the token's smallest unit |
| `token` | `Eip155TokenDeployment` | ERC-20 token (chain, contract address, transfer method) |
| `min_total_amount` | `Option<TokenAmount>` | Minimum charge regardless of row count |
| `min_items` | `Option<usize>` | Minimum rows for this tier to apply (inclusive) |
| `max_items` | `Option<usize>` | Maximum rows for this tier to apply (inclusive) |
| `description` | `Option<String>` | Human-readable label for this tier |
| `is_default` | `bool` | Whether this is the default pricing tier |

**Construction**

```rust
// Rust
let price_tag = PriceTag {
    pay_to: ChecksummedAddress::from_str("0x...").unwrap(),
    amount_per_item: TokenAmount(usdc.parse("0.002").unwrap().amount),
    token: usdc.clone(),
    min_total_amount: None,
    min_items: None,
    max_items: None,
    description: None,
    is_default: true,
};
```

```python
# Python
# amount_per_item accepts a string ("0.002") or int (2000) for smallest token units
price_tag = PriceTag(
    pay_to="0x...",
    amount_per_item="0.002",
    token=usdc,
    min_total_amount=None,
    min_items=None,
    max_items=None,
    description=None,
    is_default=True,
)
```

`PriceTag` is immutable after creation -- create a new one to change values.

---

## TablePaymentOffers

Groups the payment configuration for a single table: its pricing tiers, whether payment is required, and metadata shown to clients.

| Field | Type | Description |
|-------|------|-------------|
| `table_name` | `String` | The table this configuration applies to |
| `price_tags` | `Vec<PriceTag>` | Available pricing tiers |
| `requires_payment` | `bool` | Whether queries require payment (derived from price tags) |
| `description` | `Option<String>` | Description shown in root endpoint and 402 responses |
| `schema` | `Option<Schema>` | Arrow schema for client discovery |

**Construction**

```rust
// Rust - paid table
let offer = TablePaymentOffers::new("my_table".to_string(), vec![price_tag], Some(schema))
    .with_description("My dataset".to_string());

// Rust - free table
let free = TablePaymentOffers::new_free_table("public_table".to_string(), Some(schema));
```

```python
# Python - paid table (description can be set at creation)
offer = TablePaymentOffers("my_table", [price_tag], schema=schema, description="My dataset")

# Python - free table
free = TablePaymentOffers.new_free_table("public_table", schema=schema, description="Public data")
```

**Getters**

| Getter | Rust | Python | Returns |
|--------|------|--------|---------|
| Table name | `offer.table_name` (pub field) | `offer.table_name` | `String` / `str` |
| Requires payment | `offer.requires_payment` (pub field) | `offer.requires_payment` | `bool` |
| Description | `offer.description` (pub field) | `offer.description` | `Option<String>` / `Optional[str]` |
| Price tag count | `offer.price_tags.len()` (pub field) | `offer.price_tag_count` | `usize` / `int` |
| Price tag descriptions | iterate `offer.price_tags` | `offer.price_tag_descriptions` | -- / `List[Optional[str]]` |

**Setters / Mutators**

| Method | Rust | Python | Description |
|--------|------|--------|-------------|
| Set description | `.with_description(desc)` | `.with_description(desc)` | Set or replace the description |
| Add price tag | `.add_payment_offer(tag)` | `.add_payment_offer(tag)` | Add a pricing tier, sets `requires_payment = true` |
| Remove price tag | `.remove_price_tag(index)` | `.remove_price_tag(index)` | Remove by index, returns `bool`, updates `requires_payment` |
| Make free | `.make_free()` | `.make_free()` | Remove all price tags and set `requires_payment = false` |

---

## Tiered Pricing Example

Charge less per row for larger queries:

```rust
// Default: $0.002/row for any query
let default_tier = PriceTag {
    amount_per_item: TokenAmount(usdc.parse("0.002").unwrap().amount),
    min_items: None,
    max_items: None,
    is_default: true,
    ..base_tag.clone()
};

// Bulk: $0.001/row for queries returning 100+ rows
let bulk_tier = PriceTag {
    amount_per_item: TokenAmount(usdc.parse("0.001").unwrap().amount),
    min_items: Some(100),
    max_items: None,
    is_default: false,
    ..base_tag
};

let offer = TablePaymentOffers::new("my_table".to_string(), vec![default_tier], Some(schema))
    .add_payment_offer(bulk_tier);
```

When a query estimates 200 rows, both tiers apply (since 200 >= 100 and the default has no minimum). The client receives both options in the 402 response and can choose the cheaper one.

---

## Supported Networks

Currently supported USDC deployments:

| Network | Rust | Python |
|---------|------|--------|
| Base Sepolia (testnet) | `USDC::base_sepolia()` | `USDC("base_sepolia")` |
| Base | `USDC::base()` | `USDC("base")` |
| Avalanche Fuji (testnet) | `USDC::avalanche_fuji()` | `USDC("avalanche_fuji")` |
| Avalanche | `USDC::avalanche()` | `USDC("avalanche")` |
| Polygon | `USDC::polygon()` | `USDC("polygon")` |
| Polygon Amoy (testnet) | `USDC::polygon_amoy()` | `USDC("polygon_amoy")` |
