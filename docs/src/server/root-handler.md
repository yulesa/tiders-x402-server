# API Root Handler

The API root handler (`server/src/handler_api_root.rs`) serves the `GET /api/` endpoint. It returns a JSON discovery document describing the server, its endpoints, and every configured table — intended as a machine-readable view for clients, AI agents, and `jq`-style inspection.

## Response Shape

```json
{
  "server": {
    "url": "https://api.example.com/",
    "version": "0.3.0",
    "facilitator_url": "https://facilitator.x402.rs/"
  },
  "endpoints": {
    "GET /api/":             { "description": "..." },
    "GET /api/table/{name}": { "description": "...", "response_format": "application/json" },
    "POST /api/query":       { "description": "...", "response_format": "application/vnd.apache.arrow.stream" }
  },
  "tables": [
    {
      "name": "uniswap_v3_pool_swap",
      "description": "Uniswap V3 pool swaps",
      "requires_payment": true,
      "details": "/api/table/uniswap_v3_pool_swap",
      "pricing": [
        {
          "model": "PerRow",
          "amount_per_item": "2000",
          "min_items": null,
          "max_items": null,
          "min_total_amount": null,
          "token_address": "0x036CbD53842c5426634e7929541eC2318f3dCF7e",
          "chain": "84532",
          "pay_to": "0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7"
        }
      ]
    }
  ]
}
```

The `tables` array is sorted alphabetically by name.

## Pricing Summaries

Each table's `pricing` array contains a serialized `PriceSummary` for every `PerRow` and `Fixed` price tag — `MetadataPrice` tags are intentionally omitted. To discover metadata pricing, clients should follow the `details` link to `GET /api/table/{name}`.

| Variant | Fields included |
|---------|-----------------|
| `PerRow` | `amount_per_item`, optional `min_items`/`max_items`/`min_total_amount`, `token_address`, `chain`, `pay_to` |
| `Fixed` | `amount`, `token_address`, `chain`, `pay_to` |

Token amounts are in the smallest unit (e.g. `"2000"` for $0.002 USDC).

## Use Cases

- **Discovery for clients.** Hit `/api/` once at startup to enumerate tables, build a UI, and decide which queries to run.
- **AI agents.** A model can read this single document to understand the server's capabilities and pricing without parsing free-form prose.
- **Health check.** A 200 response with the expected `version` confirms the binary, config, and database are wired up.

## Endpoint Mounting

This handler is registered on the `/api/` path of the API sub-router. The legacy plain-text `GET /` from earlier versions has been removed — `/` is now reserved for the dashboard landing page (or unmounted entirely when no dashboards are configured).
