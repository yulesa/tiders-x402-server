# Configuration Reference

The server is configured programmatically -- there are no config files. You set up payment rules, database connections, and server parameters in code.

## GlobalPaymentConfig

The central configuration object that holds all payment-related settings.

```rust
let config = GlobalPaymentConfig::default(facilitator, base_url);
```

| Field | Type | Description |
|-------|------|-------------|
| `facilitator` | `Arc<FacilitatorClient>` | Client for the x402 facilitator service |
| `base_url` | `Url` | Server's base URL, used to construct resource URLs in payment requirements |
| `mime_type` | `String` | Response MIME type (default: `application/vnd.apache.arrow.stream`) |
| `max_timeout_seconds` | `u64` | Maximum payment timeout (default: 300) |
| `default_description` | `String` | Fallback description for payment requirements |
| `offers_tables` | `HashMap<String, TablePaymentOffers>` | Per-table payment configuration |

## PriceTag

Defines a pricing tier for a table. Each table can have multiple price tags to support tiered pricing.

```rust
let price_tag = PriceTag {
    pay_to: ChecksummedAddress::from_str("0x...").unwrap(),
    amount_per_item: TokenAmount(usdc.parse("0.002").unwrap().amount),
    token: usdc.clone(),
    min_total_amount: None,
    min_items: None,        // No minimum row count for this tier
    max_items: None,        // No maximum row count for this tier
    description: None,
    is_default: true,
};
```

| Field | Type | Description |
|-------|------|-------------|
| `pay_to` | `ChecksummedAddress` | EVM address that receives payment |
| `amount_per_item` | `TokenAmount` | Price per row in the token's smallest unit |
| `token` | `Eip155TokenDeployment` | Token deployment info (address, decimals, EIP712) |
| `min_total_amount` | `Option<TokenAmount>` | Minimum total charge regardless of row count |
| `min_items` | `Option<usize>` | Minimum rows for this tier to apply |
| `max_items` | `Option<usize>` | Maximum rows for this tier to apply |
| `description` | `Option<String>` | Human-readable description |
| `is_default` | `bool` | Whether this is the default pricing tier |

## TablePaymentOffers

Groups pricing tiers for a specific table.

```rust
let offer = TablePaymentOffers::new(
    "my_table".to_string(),
    vec![default_price, bulk_price],
    Some(schema),
).with_description("My dataset description".to_string());

config.add_offers_table(offer);
```

### Free Tables

Tables can be exposed without payment:

```rust
let free_offer = TablePaymentOffers::new_free_table(
    "public_table".to_string(),
    Some(schema),
);
config.add_offers_table(free_offer);
```

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

// Bulk: $0.001/row for queries returning 2+ rows
let bulk_tier = PriceTag {
    amount_per_item: TokenAmount(usdc.parse("0.001").unwrap().amount),
    min_items: Some(2),
    max_items: None,
    is_default: false,
    ..base_tag
};

let offer = TablePaymentOffers::new("my_table".to_string(), vec![default_tier], Some(schema))
    .with_payment_offer(bulk_tier);
```

When a query estimates 5 rows, both tiers apply (since 5 >= 2 and the default has no minimum). The client receives both options in the 402 response and can choose the cheaper one.

## Supported Networks

Currently supported USDC deployments:

| Network | Constructor |
|---------|------------|
| Base Sepolia (testnet) | `USDC::base_sepolia()` |
| Base | `USDC::base()` |
| Avalanche Fuji (testnet) | `USDC::avalanche_fuji()` |
| Avalanche | `USDC::avalanche()` |
| Polygon | `USDC::polygon()` |
| Polygon Amoy (testnet) | `USDC::polygon_amoy()` |
