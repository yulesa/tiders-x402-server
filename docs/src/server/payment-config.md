# Payment Configuration

The payment configuration module (`server/src/payment_config.rs`) manages pricing rules and generates x402-compatible payment requirements.

## GlobalPaymentConfig

The central configuration object:

```rust
pub struct GlobalPaymentConfig {
    pub facilitator: Arc<FacilitatorClient>,
    pub base_url: Url,
    pub mime_type: String,               // default: "application/vnd.apache.arrow.stream"
    pub max_timeout_seconds: u64,        // default: 300
    pub default_description: String,     // default: "Query execution payment"
    pub table_offers: HashMap<String, TablePaymentOffers>,
}
```

### Key Methods

**`table_requires_payment(table_name) -> Option<bool>`**

Returns `Some(true)` if the table exists and has payment requirements, `Some(false)` if it exists but is free, or `None` if the table is not configured.

**`get_all_payment_requirements(table_name, estimated_items, path) -> Vec<PaymentRequirements>`**

Generates all applicable payment requirements for a table given the estimated row count. Iterates through all price tags and includes those whose `min_items`/`max_items` range covers the estimated count.

**`find_matching_payment_requirements(table_name, item_count, path, payment_payload) -> Vec<PaymentRequirements>`**

Filters payment requirements to find those matching a specific payment payload. Matches on:
- `scheme` == `payment_payload.scheme`
- `network` == `payment_payload.network`
- `pay_to` == `authorization.to` from the payload
- `max_amount_required` == `authorization.value` from the payload

**`create_payment_required_response(error, table_name, estimated_items, path) -> Option<PaymentRequired>`**

Constructs the full 402 response body with error message and all applicable payment options.

## Price Calculation

For each `PriceTag`, the total price is:

```
total = amount_per_item * item_count
```

If `min_total_amount` is set, the actual charge is:

```
charge = max(total, min_total_amount)
```

## PaymentRequirements Generation

Each `PriceTag` generates a `PaymentRequirements` struct:

```rust
PaymentRequirements {
    scheme: "exact",
    network: "<chain name>",           // e.g., "base-sepolia"
    max_amount_required: "<total>",    // in token's smallest unit
    resource: "<base_url>/<path>",
    description: "<table desc> - <N> rows",
    mime_type: "application/vnd.apache.arrow.stream",
    pay_to: "<recipient address>",
    max_timeout_seconds: 300,
    asset: "<token contract address>",
    extra: { "name": "USDC", "version": "2" },
}
```
