# Query Handler

The query handler (`server/src/query_handler.rs`) is the main entry point for all HTTP requests. It implements two endpoints:

## Root Handler -- `GET /`

Returns a plain text response with:
- Usage instructions
- List of available tables with their schemas
- SQL parser rules and restrictions

## Query Handler -- `POST /query`

Accepts a JSON body:

```json
{ "query": "SELECT * FROM my_table LIMIT 10" }
```

### Processing Steps

1. **Parse and validate** the SQL using `sqp_parser::analyze_query`.
2. **Convert** the analyzed query to DuckDB SQL via `duckdb_reader::create_duckdb_query`.
3. **Check table existence** -- returns 400 if the table is not in the configuration.
4. **Check payment requirement** -- if the table is free, execute immediately and return Arrow IPC.
5. **Phase 1** (no `X-Payment` header):
   - Estimate row count with a `COUNT(*)` wrapper query.
   - Return 402 with payment options.
6. **Phase 2** (with `X-Payment` header):
   - Decode and parse the payment payload.
   - Execute the query to get actual results.
   - Find matching payment requirements.
   - Verify payment with the facilitator.
   - Settle payment with the facilitator.
   - Return Arrow IPC data.

### QueryResponse

Responses use a `QueryResponse` struct with these variants:

| Method | Status | Content-Type | Body |
|--------|--------|-------------|------|
| `success` | 200 | `application/vnd.apache.arrow.stream` | Arrow IPC binary |
| `payment_required` | 402 | `application/json` | Payment requirements JSON |
| `bad_request` | 400 | `text/plain` | Error message |
| `internal_error` | 500 | `text/plain` | Error message |

### AppState

```rust
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub payment_config: Arc<GlobalPaymentConfig>,
}
```

The DuckDB connection is behind a `Mutex` because DuckDB connections are not `Send` + `Sync` by default. This means queries are serialized, but DuckDB is fast enough for typical workloads.
