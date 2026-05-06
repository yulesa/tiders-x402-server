# Payment Protocol

The server implements the [x402 payment protocol](https://www.x402.org/), which standardizes micropayments over HTTP using the `402 Payment Required` status code.

## Protocol Overview

tiders-x402-server speaks **x402 V2**. The protocol extends HTTP with a payment negotiation layer:

1. Server returns **402** with payment options in the JSON body and a base64-encoded `Payment-Required` header.
2. Client signs a payment and resubmits the request with the signed payload in the `Payment-Signature` header.
3. Server verifies the payment via a facilitator, executes the work (or has already done so for per-row pricing), and delivers the response.

This is analogous to HTTP authentication (401 / `Authorization` header) but for payments.

## `Payment-Signature` Header

The `Payment-Signature` header contains a base64-encoded JSON `PaymentPayload`:

```json
{
  "x402Version": 2,
  "accepted": {
    "scheme": "exact",
    "network": "eip155:84532",
    "amount": "4000",
    "payTo": "0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7",
    "maxTimeoutSeconds": 300,
    "asset": "0x036CbD53842c5426634e7929541eC2318f3dCF7e",
    "extra": { "name": "USDC", "version": "2" }
  },
  "payload": {
    "signature": "0x...",
    "authorization": {
      "from": "0x<sender>",
      "to": "0x<recipient>",
      "value": "4000",
      "validAfter": "0",
      "validBefore": "1735689600",
      "nonce": "0x..."
    }
  },
  "resource": {
    "url": "http://localhost:4021/api/query",
    "description": "Uniswap v3 pool swaps - 2 rows",
    "mimeType": "application/vnd.apache.arrow.stream"
  }
}
```

The `accepted` field must exactly match one of the `accepts` entries the server returned in the previous 402 response — the server uses direct equality to choose which payment requirement to verify against.

## x402 Payment Schemes

x402 payment schemes are distinct from the server's pricing models. The only scheme currently supported is `"exact"`, which requires the client to pay exactly the amount specified in the 402 response.

The server, on the other hand, supports three pricing models: per-row, fixed, and metadata. The amount may be calculated from the row count (per-row pricing), charged as a flat fee per query (fixed pricing), or charged as a one-time fee for metadata access. From the client's perspective these are indistinguishable — in all cases the client simply pays the amount quoted in the 402 response. Only the server-side calculation that produces that quote differs.

If x402 adds new schemes (e.g. `"upto"`, where the server can settle a smaller amount than the user signed for), the server logic can be extended.

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

For **per-row** queries the server executes the database query *before* verifying (it needs the actual row count to pick the right requirement). For **fixed** and **metadata** pricing it verifies *before* doing the work, so bogus payment headers don't cost anything.

## Supported Tokens

Payment is made in ERC-20 tokens. Currently supported:

- **USDC** on Base, Base Sepolia, Avalanche, Avalanche Fuji, Polygon, Polygon Amoy

The token's EIP-712 domain info (`name`, `version`) is included in the `extra` field of payment requirements, so clients can construct the correct typed data for signing.

| Network | Rust | Python |
|---------|------|--------|
| Base Sepolia (testnet) | `USDC::base_sepolia()` | `USDC("base_sepolia")` |
| Base | `USDC::base()` | `USDC("base")` |
| Avalanche Fuji (testnet) | `USDC::avalanche_fuji()` | `USDC("avalanche_fuji")` |
| Avalanche | `USDC::avalanche()` | `USDC("avalanche")` |
| Polygon | `USDC::polygon()` | `USDC("polygon")` |
| Polygon Amoy (testnet) | `USDC::polygon_amoy()` | `USDC("polygon_amoy")` |

In YAML configs, the same set is reachable as `usdc/base_sepolia`, `usdc/base`, `usdc/avalanche_fuji`, etc.

See the [SDK Reference](../sdk-library/sdk-configuration.md) and [YAML Reference](../cli/yaml-reference.md) for full pricing and payment configuration details.
