# Payment Protocol

The server implements the [x402 payment protocol](https://www.x402.org/), which standardizes micropayments over HTTP using the `402 Payment Required` status code.

## Protocol Overview

x402 extends HTTP with a payment negotiation layer:

1. Server returns **402** with payment options in the response body.
2. Client signs a payment and attaches it as the `X-Payment` header.
3. Server verifies the payment via a facilitator and delivers the content.

This is analogous to HTTP authentication (401 / `Authorization` header) but for payments.

## X-Payment Header

The `X-Payment` header contains a base64-encoded JSON `PaymentPayload`:

```json
{
  "x402Version": 1,
  "scheme": "exact",
  "network": "base-sepolia",
  "payload": {
    "signature": "0x...",
    "authorization": {
      "from": "0x<sender>",
      "to": "0x<recipient>",
      "value": "4000",
      "validAfter": "0",
      "validBefore": "...",
      "nonce": "0x..."
    }
  }
}
```

## x402 Payment Schemes

Currently the only supported scheme is `"exact"`, which requires the client to pay the exact amount specified in the 402 response. The amount is either calculated from the row count (per-row pricing) or a flat fee (fixed pricing). From the client's perspective, the protocol is identical — only the server-side calculation differs.

## Verification Flow

```
Server                            Facilitator
  |                                    |
  |  POST /verify                      |
  |  { payment_payload,                |
  |    payment_requirements }          |
  |----------------------------------->|
  |                                    | Validates signature
  |                                    | Checks on-chain balance
  |                                    | Verifies authorization
  |  VerifyResponse::Valid             |
  |  or VerifyResponse::Invalid        |
  |<-----------------------------------|
```

If valid, the server proceeds to settle:

```
Server                            Facilitator
  |                                    |
  |  POST /settle                      |
  |  { verify_response,                |
  |    verify_request }                |
  |----------------------------------->|
  |                                    | Executes on-chain transfer
  |  SettleResponse                    |
  |<-----------------------------------|
```

## Supported Tokens

Payment is made in ERC-20 tokens. Currently supported:

- **USDC** on Base, Base Sepolia, Avalanche, Avalanche Fuji, Polygon, Polygon Amoy

The token's EIP-712 domain info (`name`, `version`) is included in the `extra` field of payment requirements, enabling clients to construct the correct typed data for signing.

| Network | Rust | Python |
|---------|------|--------|
| Base Sepolia (testnet) | `USDC::base_sepolia()` | `USDC("base_sepolia")` |
| Base | `USDC::base()` | `USDC("base")` |
| Avalanche Fuji (testnet) | `USDC::avalanche_fuji()` | `USDC("avalanche_fuji")` |
| Avalanche | `USDC::avalanche()` | `USDC("avalanche")` |
| Polygon | `USDC::polygon()` | `USDC("polygon")` |
| Polygon Amoy (testnet) | `USDC::polygon_amoy()` | `USDC("polygon_amoy")` |

See the [Configuration Reference](../getting-started/configuration.md) for full pricing and payment configuration details.

## Client Libraries

The x402 protocol has client libraries that handle the payment flow automatically:

- **TypeScript/JavaScript**: [`x402-fetch`](https://www.npmjs.com/package/x402-fetch) -- wraps `fetch()` to automatically handle 402 responses
- **Python**: See the [Python example](../../examples/python/duckdb_server.py) for server-side usage
