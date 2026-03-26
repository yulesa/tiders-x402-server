# Payment Configuration

The payment configuration module (`server/src/payment_config.rs`) is the central place where pricing rules are defined and x402 V2 payment requirements are generated. It determines how much each query costs and what payment options the server offers to clients.

## GlobalPaymentConfig

`GlobalPaymentConfig` holds everything the server needs to price queries and communicate payment options:

```rust
pub struct GlobalPaymentConfig {
    pub facilitator: Arc<FacilitatorClient>,
    pub mime_type: String,               // default: "application/vnd.apache.arrow.stream"
    pub max_timeout_seconds: u64,        // default: 300
    pub default_description: String,     // default: "Query execution payment"
    pub offers_tables: HashMap<String, TablePaymentOffers>,
}
```

- **`facilitator`** — The client used to verify and settle payments with the x402 facilitator.
- **`mime_type`** — The response format advertised to clients (defaults to `"application/vnd.apache.arrow.stream"`).
- **`max_timeout_seconds`** — How long a payment remains valid before expiring (defaults to 300 seconds).
- **`default_description`** — Fallback description when a table doesn't have its own (defaults to `"Query execution payment"`).
- **`offers_tables`** — A map of table names to their payment offers (pricing tiers, schemas, descriptions).

**Construction**

All fields except `facilitator` are optional and fall back to sensible defaults.

```rust
// Rust - with defaults
let config = GlobalPaymentConfig::default(Arc::new(facilitator));

// Rust - with custom values
let config = GlobalPaymentConfig::new(
    Arc::new(facilitator),
    Some("text/csv".to_string()),
    Some(600),
    Some("Custom description".to_string()),
    None,
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

See the [Configuration Reference](./configuration.md#globalpaymentconfig) for the full API.

## What It Does

The module answers four questions for the query handler:

1. **Does this table require payment?** — `table_requires_payment` returns whether a table is free, paid, or unknown.

2. **What are the payment options for this query?** — `get_all_payment_requirements` takes a table name and estimated row count, then returns all applicable payment requirements. Each price tag is checked against its `min_items`/`max_items` range to determine if it applies.

3. **Does the client's payment match what we expect?** — `find_matching_payment_requirements` compares the `PaymentRequirements` the client echoed back (in `PaymentPayload.accepted`) against the server-generated requirements using direct equality.

4. **What should the 402 response look like?** — `create_payment_required_response` assembles the full `PaymentRequired` response body including the error message, resource info, and all applicable payment options.

## Price Calculation

For each price tag, the total price is based on the number of rows:

```
total = amount_per_item * item_count
```

If `min_total_amount` is set, the server enforces a minimum charge:

```
charge = max(total, min_total_amount)
```

## Payment Requirements

Each applicable price tag produces a x402 `PaymentRequirements` entry sent to the client in the 402 response. The key fields are:

| Field | Description |
|-------|-------------|
| `scheme` | Payment scheme style. Always `"exact"` — the client must pay the exact amount |
| `network` | The blockchain network (e.g., `"base-sepolia"`) |
| `amount` | Total price in the token's smallest unit |
| `pay_to` | The recipient wallet address |
| `max_timeout_seconds` | How long the payment offer is valid |
| `asset` | The token contract address |
| `extra` | Token metadata (e.g., name, version) |