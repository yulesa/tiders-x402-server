# Python Bindings

The `python/` directory contains PyO3 bindings that expose the Rust server functionality to Python. This lets you configure and run a Tiders x402 server entirely from Python.

## Installation

Build with maturin:

```bash
cd python
pip install maturin
maturin develop
```

## Available Classes

### `USDCDeployment`

Represents a USDC token deployment on a supported network.

```python
usdc = tiders_x402_server.USDCDeployment.by_network("base_sepolia")
```

Supported networks: `base_sepolia`, `base`, `avalanche_fuji`, `avalanche`, `polygon`, `polygon_amoy`.

### `PriceTag`

Defines pricing for a table.

```python
price_tag = tiders_x402_server.PriceTag(
    pay_to="0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7",
    amount_per_item="$0.002",    # or "0.002" or integer (smallest unit)
    token=usdc,
    min_total_amount=None,       # optional minimum charge
    min_items=None,              # optional tier minimum
    max_items=None,              # optional tier maximum
    description=None,
    is_default=True
)
```

The `amount_per_item` parameter accepts:
- A string like `"$0.002"` or `"0.002"` -- interpreted as a human-readable amount and converted using the token's decimals.
- An integer -- interpreted as the smallest token unit (e.g., `2000` for 0.002 USDC).

### `TablePaymentOffers`

Groups pricing for a table.

```python
schema = tiders_x402_server.get_duckdb_table_schema_py("./data/db.db", "my_table")

offer = tiders_x402_server.TablePaymentOffers("my_table", [price_tag], schema)
offer.with_description("My dataset")
offer.with_payment_offer(bulk_price_tag)  # add additional tiers

# Or create a free table
free_offer = tiders_x402_server.TablePaymentOffers.new_free_table("public_table", schema)
```

### `FacilitatorClient`

Connects to an x402 facilitator service.

```python
facilitator = tiders_x402_server.FacilitatorClient("https://x402.org/facilitator")
```

### `GlobalPaymentConfig`

Central payment configuration.

```python
config = tiders_x402_server.GlobalPaymentConfig(facilitator, base_url="http://0.0.0.0:4021")
config.add_offers_table(offer)
```

### `AppState`

Application state with database and payment config.

```python
state = tiders_x402_server.AppState(
    db_path="./data/my_database.db",
    payment_config=config
)
```

### `Server`

Runs the server (blocking).

```python
server = tiders_x402_server.Server(state)
server.start_server("http://0.0.0.0:4021")  # blocks until shutdown
```

### `Schema`

Wraps an Arrow schema, convertible to/from PyArrow.

```python
# Get schema from DuckDB
schema = tiders_x402_server.get_duckdb_table_schema_py("./data/db.db", "my_table")

# Convert to pyarrow
pa_schema = schema.to_pyarrow()
```

## Complete Example

```python
import tiders_x402_server

facilitator = tiders_x402_server.FacilitatorClient("https://x402.org/facilitator")
usdc = tiders_x402_server.USDCDeployment.by_network("base_sepolia")

price_tag_default = tiders_x402_server.PriceTag(
    pay_to="0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7",
    amount_per_item="$0.002",
    token=usdc,
    is_default=True
)

price_tag_bulk = tiders_x402_server.PriceTag(
    pay_to="0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7",
    amount_per_item="0.001",
    token=usdc,
    min_items=2,
    is_default=False
)

schema = tiders_x402_server.get_duckdb_table_schema_py("./data/db.db", "swaps")
offer = tiders_x402_server.TablePaymentOffers("swaps", [price_tag_default], schema)
offer.with_payment_offer(price_tag_bulk)

config = tiders_x402_server.GlobalPaymentConfig(facilitator, base_url="http://0.0.0.0:4021")
config.add_offers_table(offer)

state = tiders_x402_server.AppState(db_path="./data/db.db", payment_config=config)
server = tiders_x402_server.Server(state)
server.start_server("http://0.0.0.0:4021")
```
