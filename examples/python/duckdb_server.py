import tiders_x402_server

def main():
    
    # Create facilitator client
    facilitator_url = "https://facilitator.x402.rs"
    facilitator = tiders_x402_server.FacilitatorClient(facilitator_url)

    # Create USDC token instance for Sepolia
    usdc = tiders_x402_server.USDC("base_sepolia")

    # First price tag: 0.002 USDC per item (default)
    price_tag_1 = tiders_x402_server.PriceTag(
        pay_to="0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7",
        amount_per_item="$0.002",
        token=usdc,
        min_total_amount=None,
        min_items=None,
        max_items=None,
        description=None,
        is_default=True
    )
    
    # Second price tag: 0.001 USDC per item for 2+ items
    price_tag_2 = tiders_x402_server.PriceTag(
        pay_to="0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7",
        amount_per_item="0.001",
        token=usdc,
        min_total_amount=None,
        min_items=2,
        max_items=None,
        description=None,
        is_default=False
    )
    
    # Create DuckDB database instance
    db = tiders_x402_server.DuckDbDatabase("../data/duckdb.db")

    # Get schema and create table payment offers
    swap_schema = db.get_table_schema("uniswap_v3_pool_swap")
    swaps_offer = tiders_x402_server.TablePaymentOffers("uniswap_v3_pool_swap", [price_tag_1], swap_schema)
    swaps_offer.with_payment_offer(price_tag_2)

    server_base_url = "http://0.0.0.0:4021"

    global_payment_config = tiders_x402_server.GlobalPaymentConfig(
        facilitator,
    )

    global_payment_config.add_offers_table(swaps_offer)

    state = tiders_x402_server.AppState(
        db,
        payment_config=global_payment_config,
        server_base_url=server_base_url,
    )

    print("Starting server")
    print("Database: data/duckdb.db")
    print(f"Facilitator: {facilitator_url}")
    print("Table 'uniswap_v3_pool_swap' requires payment")

    # Start the server (blocking call)
    tiders_x402_server.start_server_py(state)

if __name__ == "__main__":
    main()