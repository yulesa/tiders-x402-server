# Price

The price module (`server/src/price.rs`) defines the pricing model for paid tables. It contains the data structures that describe how much a query costs and the builder methods used to configure tables at startup.

## PriceTag

A `PriceTag` represents a single pricing tier for a table. It answers the question: "if a client requests N rows, how much should they pay and to whom?"

```rust
pub struct PriceTag {
    pub pay_to: ChecksummedAddress,
    pub amount_per_item: TokenAmount,
    pub token: Eip155TokenDeployment,
    pub min_total_amount: Option<TokenAmount>,
    pub min_items: Option<usize>,
    pub max_items: Option<usize>,
    pub description: Option<String>,
    pub is_default: bool,
}
```

Each price tag specifies:

- **`pay_to`** — the recipient wallet address.
- **`amount_per_item`** — the price per row (in the token's smallest unit).
- **`token`** — the ERC-20 token used for payment (chain, contract address, transfer method).
- **`min_total_amount`** — optional minimum charge, enforced even if the per-row calculation is lower.
- **`min_items` / `max_items`** — optional range that determines when this tier applies. A price tag only matches if the row count falls within this range.
- **`description`** — optional human-readable label for this tier.
- **`is_default`** — whether this is the default pricing tier for the table.

A table can have multiple price tags (e.g., different tokens, different tiers for small vs. large queries). The `payment_config` module selects which ones apply for a given row count.

### Price Calculation

The total price for a query is:

```
total = amount_per_item * row_count
```

If `min_total_amount` is set:

```
charge = max(total, min_total_amount)
```

## TablePaymentOffers

`TablePaymentOffers` groups everything needed to describe a table's payment setup:

```rust
pub struct TablePaymentOffers {
    /// Table name
    pub table_name: String,
    /// Available payment options for this table
    pub price_tags: Vec<PriceTag>,
    /// Whether this table requires payment
    pub requires_payment: bool,
    /// Custom description for this table's payment requirements
    pub description: Option<String>,
    /// Table schema: Option<Schema>
    pub schema: Option<Schema>,
}
```

- **`table_name`** — the table this configuration applies to.
- **`price_tags`** — the list of pricing tiers.
- **`requires_payment`** — whether the table is paid or free (derived from whether price tags exist).
- **`description`** — optional description shown to clients in the root endpoint and 402 responses.
- **`schema`** — optional Arrow schema, displayed in the root endpoint to help clients discover available columns.

### Construction

Tables are configured at startup using a builder pattern:

- `TablePaymentOffers::new(name, price_tags, schema)` — creates a paid table with the given pricing tiers.
- `TablePaymentOffers::new_free_table(name, schema)` — creates a free table (no payment required).
- `.with_description(desc)` — adds a human-readable description.
- `.with_payment_offer(price_tag)` — adds an additional pricing tier.
