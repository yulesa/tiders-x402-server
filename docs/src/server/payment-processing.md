# Payment Processing

The payment processing module (`server/src/payment_processing.rs`) handles the communication with the x402 facilitator for payment verification and settlement. It sits between the query handler and the facilitator client, translating between the server's V2 types and the facilitator's wire format.

## Role

The query handler delegates to this module once it has a payment payload and a matching payment requirement. The module provides two functions that map directly to the two steps of the payment lifecycle:

1. **`verify_payment`** — builds a V2 verify request, converts it to the facilitator's wire format, and sends it. Returns both the wire-format request (needed later for settlement) and the typed V2 response so the query handler can inspect the result.

2. **`settle_payment`** — takes a successful verification response and the original wire-format request, and asks the facilitator to execute the on-chain transfer. Returns an error if the facilitator reports a settlement failure.

## How It Fits Together

```
query_handler
  │
  ├── verify_payment(facilitator, payload, requirements)
  │       └── facilitator_client.verify(request)
  │
  └── settle_payment(verify_response, facilitator, verify_request)
          └── facilitator_client.settle(request)
```

The module intentionally keeps no state — it converts types and forwards calls. The payment configuration logic (which requirements to use, pricing) lives in `payment_config`, and the HTTP transport lives in `facilitator_client`.
