"""Example: paid data server using tiders-x402-server with DuckDB (Python).

Demonstrates how to configure a DuckDB backend, set up per-row USDC pricing
via the x402 protocol, and start the HTTP server using the Python bindings.
"""

import tiders_x402_server
import duckdb
import os


def main():

    # Create facilitator client
    facilitator_url = "https://facilitator.x402.rs"
    facilitator = tiders_x402_server.FacilitatorClient(facilitator_url)

    # Create USDC token instance for Sepolia
    usdc = tiders_x402_server.USDC("base_sepolia")

    # First price tag: 0.002 USDC per item (default, per-row pricing)
    price_tag_1 = tiders_x402_server.PriceTag(
        pay_to="0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7",
        amount_per_item="$0.002",
        token=usdc,
        is_default=True,
    )

    # Second price tag: 0.001 USDC per item for 2+ items (per-row pricing)
    price_tag_2 = tiders_x402_server.PriceTag(
        pay_to="0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7",
        amount_per_item="0.001",
        token=usdc,
        min_items=100,
        is_default=False,
    )

    # Example: Fixed price tag (flat fee regardless of row count)
    price_tag_fixed = tiders_x402_server.PriceTag.fixed(
        pay_to="0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7",
        fixed_amount="0.01",
        token=usdc,
        is_default=True,
    )

    # Load sample data from CSV into a DuckDB database file.
    # Replace this with your own database path for production use.
    db_path = "../data/duckdb.db"
    os.makedirs(os.path.dirname(db_path), exist_ok=True)
    conn = duckdb.connect(db_path)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS uniswap_v3_pool_swap AS SELECT * FROM read_csv_auto('../uniswap_v3_pool_swap.csv');"
    )
    conn.close()

    db = tiders_x402_server.DuckDbDatabase(db_path)

    # Get schema and create table payment offers
    swap_schema = db.get_table_schema("uniswap_v3_pool_swap")
    swaps_offer = tiders_x402_server.TablePaymentOffers(
        "uniswap_v3_pool_swap", [price_tag_1], swap_schema
    )
    swaps_offer.add_payment_offer(price_tag_2)
    swaps_offer.add_payment_offer(price_tag_fixed)

    server_base_url = "http://localhost:4021"
    server_bind_address = "0.0.0.0:4021"

    global_payment_config = tiders_x402_server.GlobalPaymentConfig(
        facilitator,
    )

    global_payment_config.add_offers_table(swaps_offer)

    state = tiders_x402_server.AppState(
        db,
        payment_config=global_payment_config,
        server_base_url=server_base_url,
        server_bind_address=server_bind_address,
    )

    print("Starting server")
    print("Database: data/duckdb.db")
    print(f"Facilitator: {facilitator_url}")
    print("Table 'uniswap_v3_pool_swap' requires payment")

    # Start the server (blocking call)
    tiders_x402_server.start_server_py(state)


if __name__ == "__main__":
    main()
