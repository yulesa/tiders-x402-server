# Payment Flow

The server implements a two-step HTTP payment flow based on the [x402 protocol](https://www.x402.org/) (V2). The flow differs slightly depending on whether the table uses **per-row**, **fixed**, or **metadata** pricing.

## Pricing Flow

![Tiders-x402-server Payment Flow](../resources/payment_flow.png)

## Step 1: Estimation

When a client sends a query to `POST /api/query` (or `GET /api/table/{name}` for metadata) without a `Payment-Signature` header:

1. The server parses and validates the SQL.
2. For **per-row** tables: it wraps the query in `SELECT COUNT(*) FROM (...)` to estimate the row count, then computes the applicable pricing tiers.
   For **fixed-price** and **metadata-price** tables: this step is skipped (the price doesn't depend on row count).
3. It returns HTTP **402 Payment Required** with:
   - A JSON body containing the error message, `resource` info, and all applicable payment options under `accepts`.
   - A `Payment-Required` header carrying the same payload, base64-encoded (so SDKs that read headers, e.g. `x402-fetch`, can pick it up directly).

```json
{
  "x402Version": 2,
  "error": "No crypto payment found. Implement x402 protocol...",
  "resource": {
    "url": "http://server:4021/api/query",
    "description": "Uniswap v3 swaps - 2 rows",
    "mimeType": "application/vnd.apache.arrow.stream"
  },
  "accepts": [
    {
      "scheme": "exact",
      "network": "eip155:84532",
      "amount": "4000",
      "payTo": "0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7",
      "maxTimeoutSeconds": 300,
      "asset": "0x036CbD53842c5426634e7929541eC2318f3dCF7e",
      "extra": { "name": "USDC", "version": "2" }
    }
  ]
}
```

Each entry in `accepts` represents a valid payment option. If multiple pricing tiers apply (different tokens, networks, or bulk tiers), multiple options are returned and the client picks one.

## Step 2: Execution and Settlement

The client resubmits the same request with a `Payment-Signature` header (base64-encoded `PaymentPayload` JSON). The server's behaviour depends on the pricing model:

### Per-Row Tables

1. Decode and deserialize the payment payload into a V2 `PaymentPayload`.
2. Execute the actual query to get the real row count.
3. Match the payload's `accepted` field against the generated payment requirements.
4. Send a **verify** request to the facilitator to confirm the payment is valid and funded.
5. If verified, send a **settle** request to execute the on-chain transfer.
6. Return the query results as Arrow IPC with HTTP 200.

### Fixed-Price Tables

1. Decode and deserialize the payment payload into a V2 `PaymentPayload`.
2. Match the payload's `accepted` field against the generated payment requirements (row count is irrelevant).
3. Send a **verify** request to the facilitator.
4. **Only after verification succeeds**, execute the actual query.
5. Send a **settle** request to execute the on-chain transfer.
6. Return the query results as Arrow IPC with HTTP 200.

This ordering is intentional: it prevents an attacker from triggering expensive queries with bogus payment headers, since the database is never touched until the facilitator has approved the payment.

### Metadata-Priced Tables (`GET /api/table/{name}`)

The same verify-before-act flow as fixed-price tables. The "work" being protected is just serializing the table's `TablePaymentOffers` to JSON, but the structure is identical: match → verify → return data → settle.

## Error Cases

| Scenario | Response |
|----------|----------|
| Table not found | 400 Bad Request (`/api/query`) or 404 Not Found (`/api/table/{name}`) |
| Invalid SQL | 400 Bad Request |
| Malformed payment header | 400 Bad Request |
| No matching payment offer | 500 Internal Server Error |
| Payment verification fails | 402 Payment Required (with reason and updated options) |
| Payment settlement fails | 402 Payment Required (with reason and updated options) |
| Facilitator unreachable | 500 Internal Server Error |
