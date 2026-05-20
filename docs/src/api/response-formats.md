# Response Formats

## Arrow IPC (Success)

Successful queries to `GET /api/query` return data in [Apache Arrow IPC streaming format](https://arrow.apache.org/docs/format/Columnar.html#ipc-streaming-format).

```
Content-Type: application/vnd.apache.arrow.stream
```

Arrow IPC is a binary columnar format that is significantly more efficient than JSON for structured data. It preserves type information and supports zero-copy reads.

### Reading in TypeScript

```typescript
import * as arrow from 'apache-arrow';

const response = await fetch("http://localhost:4021/api/query?query=" + encodeURIComponent(sql));
const arrayBuffer = await response.arrayBuffer();
const table = arrow.tableFromIPC(arrayBuffer);

for (const row of table) {
  console.log(row.toJSON());
}
```

### Reading in Python

```python
import pyarrow as pa

reader = pa.ipc.open_stream(response_bytes)
table = reader.read_all()
print(table.to_pandas())
```

### Reading in Rust

```rust
use arrow::ipc::reader::StreamReader;
use std::io::Cursor;

let reader = StreamReader::try_new(Cursor::new(bytes), None)?;
let batches: Vec<RecordBatch> = reader.collect::<Result<_, _>>()?;
```

## Payment Required (402)

When payment is needed, the response is JSON following the x402 V2 specification, with the same payload duplicated as a base64-encoded `Payment-Required` HTTP header so SDKs that read headers can pick it up directly.

The [x402 foundation](https://github.com/x402-foundation/x402) maintains official client implementations that handle the full payment flow automatically — including TypeScript (`x402-fetch`, `x402-axios`) and Python clients. Using one of these is the recommended way to interact with any x402-enabled server without writing payment logic by hand.

For example, with `x402-fetch`:

```
Content-Type: application/json
Payment-Required: <base64>
```

```json
{
  "x402Version": 2,
  "error": "No crypto payment found...",
  "resource": {
    "url": "http://localhost:4021/api/query?query=SELECT%20*%20FROM%20uniswap_v3_pool_swap%20LIMIT%202",
    "description": "Uniswap v3 pool swaps - 2 rows",
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

### Top-level fields

| Field | Description |
|-------|-------------|
| `x402Version` | Protocol version (`2`) |
| `error` | Human-readable explanation of why payment is required |
| `resource.url` | URL of the resource being paid for |
| `resource.description` | Human-readable description (often includes the row count for per-row pricing) |
| `resource.mimeType` | Content type of the successful response (`application/vnd.apache.arrow.stream` for queries, `application/json` for metadata) |
| `accepts` | List of `PaymentRequirements`. The client picks one and signs it |

### `PaymentRequirements` fields

| Field | Description |
|-------|-------------|
| `scheme` | Payment scheme. Always `"exact"` today |
| `network` | EIP-155 chain identifier (e.g. `"eip155:84532"` for Base Sepolia) |
| `amount` | Total price in the token's smallest unit (USDC has 6 decimals — `"4000"` = $0.004) |
| `payTo` | Recipient wallet address |
| `maxTimeoutSeconds` | How long this offer is valid for |
| `asset` | ERC-20 token contract address |
| `extra` | Token metadata (`name`, `version`) used by the client to construct the EIP-712 domain when signing |

For per-row pricing, multiple `accepts` entries may be returned (e.g., a default tier plus a bulk-discount tier whose `min_items` is satisfied by the estimated row count). The client is free to pay any of them.

## Error Responses

Errors are returned as plain text:

```
Content-Type: text/plain
```

| Status | Cause |
|--------|-------|
| 400 | Invalid SQL, unsupported table, or malformed payment header |
| 404 | Table not found (only on `GET /api/table/{name}`) |
| 500 | Database errors, facilitator communication failures, serialization errors |
| 503 | Dashboard configured but no `index.html` built yet (only on `GET /` and `/<slug>/`) |
