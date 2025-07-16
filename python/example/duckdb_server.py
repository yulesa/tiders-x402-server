import cherry_402_python

def main():
    
    # Create facilitator client
    facilitator = cherry_402_python.FacilitatorClient("http://localhost:4022")
    
    # Create price tags similar to the Rust example
    # First price tag: 0.002 USDC per item (default)
    price_tag_1 = cherry_402_python.PriceTag(
        pay_to="0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7",
        amount_per_item="0.002",
        token="0x0000000000000000000000000000000000000000",  # Will use USDC
        min_total_amount=None,
        min_items=None,
        max_items=None,
        description=None,
        is_default=True
    )
    
    # Second price tag: 0.001 USDC per item for 2+ items
    price_tag_2 = cherry_402_python.PriceTag(
        pay_to="0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7",
        amount_per_item="0.001",
        token="0x0000000000000000000000000000000000000000",  # Will use USDC
        min_total_amount=None,
        min_items=2,
        max_items=None,
        description=None,
        is_default=False
    )
    
    # Create table payment offers
    swaps_offer = cherry_402_python.TablePaymentOffers("swaps_df", [price_tag_1])
    swaps_offer.with_payment_offer(price_tag_2)
    
    # Setup the server with database and payment configuration
    # server.setup_server(
    #     facilitator_url="http://localhost:4022",
    #     base_url="http://localhost:4021",
    #     db_path="../data/uni_v2_swaps.db",
    #     table_offers=[swaps_offer]
    # )

    global_payment_config = cherry_402_python.GlobalPaymentConfig(
        facilitator,
        base_url="http://localhost:4021",
    )

    global_payment_config.add_table_offer(swaps_offer)

    state = cherry_402_python.AppState(
        db_path="../data/uni_v2_swaps.db",
        payment_config=global_payment_config,
    )

    server = cherry_402_python.Server(
        state,
    )
    
    print("Starting server on http://localhost:4021")
    print("Database: data/uni_v2_swaps.db")
    print("Facilitator: http://localhost:4022")
    print("Table 'swaps_df' requires payment")
    
    # Start the server (this will block)
    server.start_server()

if __name__ == "__main__":
    main()