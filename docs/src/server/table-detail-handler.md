# Table Detail Handler

The table detail handler (`server/src/table_detail_handler.rs`) serves the `GET /api/table/:name` endpoint. It returns full schema and payment offer details for a specific table as JSON.

## Endpoint

```
GET /api/table/:name
```

Returns the `TablePaymentOffers` for the requested table, including its schema, pricing tiers, and payment requirements.

## Payment Flow

If the table has a `MetadataPrice` price tag, the endpoint requires payment before returning the data. The flow follows the x402 protocol:

1. **No `Payment-Signature` header** -- returns HTTP 402 with payment requirements in the JSON body and a `Payment-Required` header (base64-encoded).
2. **With `Payment-Signature` header** -- the handler:
   1. Decodes the base64 payment payload.
   2. Matches it against the table's metadata payment requirements.
   3. Sends a **verify** request to the facilitator.
   4. If valid, sends a **settle** request.
   5. Returns the table metadata as JSON with HTTP 200.

If the table has no `MetadataPrice` tag, the metadata is returned freely (no payment needed).

## Responses

### 200 OK

Returns the `TablePaymentOffers` as JSON:

```json
{
  "table_name": "uniswap_v3_pool_swap",
  "price_tags": [
    {
      "pay_to": "0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7",
      "pricing": { "model": "PerRow", "amount_per_item": "2000000000000000", ... },
      "token": { "chain": "84532", "address": "0x036CbD53842c5426634e7929541eC2318f3dCF7e", ... },
      "is_default": true
    }
  ],
  "requires_payment": true,
  "description": "Uniswap V3 pool swaps",
  "schema": { ... }
}
```

### 402 Payment Required

Returned when the table has a `MetadataPrice` tag and no valid payment is provided.

```json
{
  "x402Version": 2,
  "error": "No crypto payment found. Implement x402 protocol...",
  "resource": {
    "url": "http://localhost:4021/api/table/uniswap_v3_pool_swap",
    "description": "Uniswap V3 pool swaps - metadata access",
    "mime_type": "application/json"
  },
  "accepts": [ ... ]
}
```

Includes a `Payment-Required` header with the same data, base64-encoded.

### 404 Not Found

```json
{
  "error": "Table 'unknown_table' not found"
}
```

### 400 Bad Request

Returned when the `Payment-Signature` header cannot be decoded or parsed.

### 500 Internal Server Error

Returned on facilitator communication failures or when no matching payment offer is found.

## Headers

| Header | Direction | Description |
|--------|-----------|-------------|
| `Payment-Signature` | Request | Base64-encoded payment payload (required for paid metadata) |
| `Payment-Required` | Response | Base64-encoded payment requirements (on 402) |
| `Content-Type: application/json` | Response | All responses are JSON |

## Related

- [MetadataPrice](./price.md#metadataprice) -- the pricing model that controls metadata access
- [Payment Configuration](./payment-config.md) -- how metadata payment requirements are generated
- [YAML Configuration](../cli/yaml-reference.md#metadata_price) -- configuring metadata pricing in the CLI
