import tiders_x402_server

def main():
    
    # Create facilitator client
    facilitator = tiders_x402_server.FacilitatorClient("http://localhost:4022")

    usdc = tiders_x402_server.USDCDeployment.by_network(tiders_x402_server.Network.BASE_SEPOLIA)
    # Create price tags similar to the Rust example
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
    
    swap_schema = tiders_x402_server.get_duckdb_table_schema_py("../data/uni_v2_swaps.db", "swaps_df")

    # Create table payment offers
    swaps_offer = tiders_x402_server.TablePaymentOffers("swaps_df", [price_tag_1], swap_schema)
    swaps_offer.with_payment_offer(price_tag_2)
    
    base_url = "http://0.0.0.0:4021"

    global_payment_config = tiders_x402_server.GlobalPaymentConfig(
        facilitator,
        base_url=base_url,
    )

    global_payment_config.add_table_offer(swaps_offer)

    state = tiders_x402_server.AppState(
        db_path="../data/uni_v2_swaps.db",
        payment_config=global_payment_config,
    )

    server = tiders_x402_server.Server(
        state,
    )
    
    print("Starting server on http://localhost:4021")
    print("Database: data/uni_v2_swaps.db")
    print("Facilitator: http://localhost:4022")
    print("Table 'swaps_df' requires payment")
    
    # Start the server (this will block)
    server.start_server(base_url)

if __name__ == "__main__":
    main()