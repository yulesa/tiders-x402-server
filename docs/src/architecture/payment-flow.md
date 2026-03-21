# Payment Flow

The server implements a two-phase HTTP payment flow based on the [x402 protocol](https://www.x402.org/).

## Sequence Diagram

```
Client                        Server                       Facilitator
  |                              |                              |
  |  POST /query (no payment)    |                              |
  |----------------------------->|                              |
  |                              | Parse & validate SQL         |
  |                              | Estimate row count           |
  |                              | Calculate pricing            |
  |  402 Payment Required        |                              |
  |<-----------------------------|                              |
  |  { accepts: [...] }          |                              |
  |                              |                              |
  |  Sign payment with wallet    |                              |
  |                              |                              |
  |  POST /query + X-Payment     |                              |
  |----------------------------->|                              |
  |                              | Decode X-Payment header      |
  |                              | Execute query (actual rows)  |
  |                              | Find matching payment offer  |
  |                              |                              |
  |                              |  POST /verify                |
  |                              |----------------------------->|
  |                              |  VerifyResponse              |
  |                              |<-----------------------------|
  |                              |                              |
  |                              |  POST /settle                |
  |                              |----------------------------->|
  |                              |  SettleResponse              |
  |                              |<-----------------------------|
  |                              |                              |
  |  200 OK (Arrow IPC)          |                              |
  |<-----------------------------|                              |
```

## Phase 1: Price Discovery

When a client sends a query without an `X-Payment` header:

1. The server parses and validates the SQL.
2. It wraps the query in `SELECT COUNT(*) FROM (...)` to estimate the row count.
3. It calculates applicable pricing tiers based on the estimated row count.
4. It returns an HTTP **402 Payment Required** response with a JSON body:

```json
{
  "x402Version": 1,
  "error": "No crypto payment found. Implement x402 protocol...",
  "accepts": [
    {
      "scheme": "exact",
      "network": "base-sepolia",
      "max_amount_required": "4000",
      "resource": "http://server:4021/query",
      "description": "Uniswap v2 swaps - 2 rows",
      "mime_type": "application/vnd.apache.arrow.stream",
      "pay_to": "0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7",
      "max_timeout_seconds": 300,
      "asset": "0x036CbD53842c5426634e7929541eC2318f3dCF7e",
      "extra": { "name": "USDC", "version": "2" }
    }
  ]
}
```

Each entry in `accepts` represents a valid payment option. If multiple pricing tiers apply (e.g., default and bulk), multiple options are returned.

## Phase 2: Payment and Data Delivery

When the client resends with an `X-Payment` header (base64-encoded payment payload):

1. The server decodes and parses the payment payload.
2. It executes the actual query to get the real row count.
3. It finds payment requirements matching the payload (by scheme, network, pay_to, and amount).
4. It sends a **verify** request to the facilitator to validate the payment on-chain.
5. If verified, it sends a **settle** request to complete the payment.
6. It serializes the query results to Arrow IPC and returns them with HTTP 200.

## Payment Matching

The server matches the client's payment to a specific pricing tier by comparing:
- `scheme` -- Payment scheme (currently `"exact"`)
- `network` -- Blockchain network
- `pay_to` -- Recipient address (from the payment's `authorization.to` field)
- `max_amount_required` -- Payment amount (from `authorization.value`)

## Error Cases

| Scenario | Response |
|----------|----------|
| Table not found | 400 Bad Request |
| Invalid SQL | 400 Bad Request |
| No matching payment offer | 500 Internal Server Error |
| Payment verification fails | 402 Payment Required (with reason) |
| Payment settlement fails | 402 Payment Required (with reason) |
| Facilitator unreachable | 500 Internal Server Error |
