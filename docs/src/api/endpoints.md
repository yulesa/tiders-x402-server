# API Endpoints

The server exposes three endpoints:

## `GET /`

Returns server information as plain text.

**Request**

```bash
curl http://[Server_URL]/
```

**Response**

```
Welcome to the Tiders-x402 API!

Usage:
- Send a POST request to /query with a JSON body: { "query": "SELECT ... FROM ..." }
- You must implement the x402 payment protocol to access paid tables.
- See x402 protocol docs: https://x402.gitbook.io/x402

Supported tables:
- Table: swaps_df
  Description: Uniswap v2 swaps
  Payment required: true
  Details: GET /table/uniswap_v3_pool_swap

SQL parser rules:
- Only SELECT statements are supported.
- Only one statement per request.
- Only one table in the FROM clause.
- No GROUP BY, HAVING, JOIN, or subqueries.
- Only simple field names in SELECT, no expressions.
- WHERE, ORDER BY, and LIMIT are supported with restrictions.
```

## `POST /query`

Executes a SQL query against the database.

Queries must conform to a restricted SQL dialect ("Simplified SQL") whose AST permits only `SELECT` statements against a single table, with a limited set of `WHERE`, `ORDER BY`, and `LIMIT` expressions. JOINs, subqueries, GROUP BY, CTEs, window functions, and aggregates are rejected. See the [SQL Parser](../server/sql-parser.md) page for the full grammar and list of supported features.

**Request**

```bash
curl -X POST http://[Server_URL]/query \
  -H "Content-Type: application/json" \
  -d '{"query": "SELECT * FROM my_table LIMIT 10"}'
```
**Request Body**

```json
{
  "query": "SELECT * FROM my_table WHERE col1 = 'value' LIMIT 10"
}
```

**Response: 200 OK (Success)**

Binary Arrow IPC stream.

```
Content-Type: application/vnd.apache.arrow.stream
```

Parse with any Arrow library (PyArrow, `apache-arrow` in JS, `arrow` crate in Rust).

**Response: 402 Payment Required**

Returned when the table requires payment and no valid `X-Payment` header is present.

```json
{
  "x402Version": 1,
  "error": "No crypto payment found. Implement x402 protocol...",
  "accepts": [
    {
      "scheme": "exact",
      "network": "base-sepolia",
      "max_amount_required": "4000",
      "resource": "http://[Server_URL]/query",
      "description": "Uniswap v2 swaps - 2 rows",
      "mime_type": "application/vnd.apache.arrow.stream",
      "pay_to": "0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7",
      "max_timeout_seconds": 300,
      "asset": "0x036CbD53842c5426634e7929541eC2318f3dCF7e",
      "extra": {
        "name": "USDC",
        "version": "2"
      }
    }
  ]
}
```

**Response: 400 Bad Request**

```
Content-Type: text/plain

Invalid query: Simplified SQL does not support the use of 'GroupByExpr'
```

**Response: 500 Internal Server Error**

```
Content-Type: text/plain

Failed to execute query: ...
```

**Headers**

| Header | Direction | Description |
|--------|-----------|-------------|
| `Content-Type: application/json` | Request | Required for POST body |
| `X-Payment` | Request | Base64-encoded payment payload (Phase 2) |
| `Content-Type: application/vnd.apache.arrow.stream` | Response | Arrow IPC data on success |
| `Content-Type: application/json` | Response | Payment requirements on 402 |

---

## `GET /table/:name`

Returns full schema and payment offer details for a specific table as JSON.

If the table has a [`MetadataPrice`](../server/price.md#metadataprice) price tag, this endpoint requires payment via the x402 protocol before returning data. Otherwise the metadata is returned freely.

**Request**

```bash
curl http://[Server_URL]/table/my_table
```

**Response: 200 OK (Success)**

Returns the table's payment configuration as JSON:

```json
{
  "table_name": "my_table",
  "price_tags": [
    {
      "pay_to": "0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7",
      "pricing": { "model": "PerRow", "amount_per_item": "2000000000000000" },
      "token": { "chain": "84532", "address": "0x036CbD53842c5426634e7929541eC2318f3dCF7e" },
      "is_default": true
    }
  ],
  "requires_payment": true,
  "description": "My dataset",
  "schema": { "fields": [ ... ] }
}
```

**Response: 402 Payment Required**

Returned when the table has a `MetadataPrice` tag and no valid `Payment-Signature` header is provided.

```json
{
  "x402Version": 2,
  "error": "No crypto payment found. Implement x402 protocol...",
  "resource": {
    "url": "http://[Server_URL]/table/my_table",
    "description": "My dataset - metadata access",
    "mime_type": "application/json"
  },
  "accepts": [ ... ]
}
```

**Response: 404 Not Found**

```json
{
  "error": "Table 'unknown_table' not found"
}
```

**Headers**

| Header | Direction | Description |
|--------|-----------|-------------|
| `Payment-Signature` | Request | Base64-encoded payment payload (required for paid metadata) |
| `Payment-Required` | Response | Base64-encoded payment requirements (on 402) |
| `Content-Type: application/json` | Response | All responses are JSON |
