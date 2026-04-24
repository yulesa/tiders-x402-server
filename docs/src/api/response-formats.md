# Response Formats

## Arrow IPC (Success)

Successful queries return data in [Apache Arrow IPC streaming format](https://arrow.apache.org/docs/format/Columnar.html#ipc-streaming-format).

```
Content-Type: application/vnd.apache.arrow.stream
```

Arrow IPC is a binary columnar format that is significantly more efficient than JSON for structured data. It preserves type information and supports zero-copy reads.

### Reading in TypeScript

```typescript
import * as arrow from 'apache-arrow';

const response = await fetch("http://localhost:4021/api/query", { ... });
const arrayBuffer = await response.arrayBuffer();
const table = arrow.tableFromIPC(arrayBuffer);

for (const row of table) {
  console.log(row.toJSON());
}
```

### Reading in Python

```python
import pyarrow as pa

# From bytes
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

When payment is needed, the response is JSON following the x402 specification:

```json
{
  "x402Version": 1,
  "error": "No crypto payment found...",
  "accepts": [
    {
      "scheme": "exact",
      "network": "base-sepolia",
      "max_amount_required": "4000",
      "resource": "http://localhost:4021/api/query",
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

### Fields

| Field | Description |
|-------|-------------|
| `scheme` | Payment scheme (`"exact"`) |
| `network` | Blockchain network name |
| `max_amount_required` | Total price in the token's smallest unit (e.g., USDC has 6 decimals, so `"4000"` = $0.004) |
| `resource` | URL of the resource being paid for |
| `description` | Human-readable description with row count |
| `mime_type` | Content type of the successful response |
| `pay_to` | Recipient wallet address |
| `max_timeout_seconds` | How long the payment offer is valid |
| `asset` | ERC-20 token contract address |
| `extra` | Token EIP-712 domain info for signing |

## Error Responses

Errors are returned as plain text:

```
Content-Type: text/plain
```

- **400**: Invalid SQL, unsupported table, or malformed payment header
- **500**: Database errors, facilitator communication failures, or serialization errors
