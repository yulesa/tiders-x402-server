# Query Handler

The query handler (`server/src/query_handler.rs`) is the Axum handler for the `POST /query` API endpoint. It is the core of the server logic: it receives SQL queries from clients, validates them, checks whether payment is required, and orchestrates the x402 V2 payment flow when needed.

The handler accepts a JSON body:

```json
{ "query": "SELECT * FROM my_table LIMIT 10" }
```

For paid tables, a successful request typically involves two steps. First, the client submits a query without payment to discover the price. The server responds with a 402 containing the payment conditions — most importantly, the cost. Then the client resubmits the same query with a `Payment-Signature` header attached.

## Processing Flow

Every request goes through the same initial validation:

1. **Parse and validate** the SQL query using `sqp_parser::analyze_query`.
2. **Convert** the parsed query into executable DuckDB SQL via `duckdb_reader::create_duckdb_query`.
3. **Check table existence** — return status 400 if the table is not in the configuration.
4. **Check payment requirement** — if the table is free, execute immediately and return the data (Arrow IPC format).

For paid tables, the flow branches depending on whether the client included a payment and the table's pricing model:

5. **Estimation** (no `Payment-Signature` header):
   - For **per-row** tables: estimate the row count using a `COUNT(*)` wrapper query.
   - For **fixed-price** tables: skip the estimation (the price doesn't depend on row count).
   - Return status 402 with x402 V2 payment requirements in both the `Payment-Required` header (base64-encoded) and the JSON response body.

6. **Execution and Settlement** (with `Payment-Signature` header):

   The server uses two different flows depending on the pricing model:

   **Per-row flow** (`process_payment`):
   - Decode and deserialize the payment header into a V2 `PaymentPayload`.
   - Execute the query to get the actual results and compute the actual number of rows (to verify the cost).
   - Match the payload against the generated payment requirements.
   - Verify the payment with the facilitator.
   - If verification fails, return 402 with updated payment options.
   - Settle the payment with the facilitator.
   - Return the query results as Arrow IPC data.

   **Fixed-price flow** (`process_fixed_price_payment`):
   - Decode and deserialize the payment header into a V2 `PaymentPayload`.
   - Match the payload against the generated payment requirements.
   - **Verify the payment with the facilitator BEFORE executing the query.** This prevents bogus payment headers from triggering expensive queries.
   - Execute the query.
   - Settle the payment with the facilitator.
   - Return the query results as Arrow IPC data.

## Responses

The query handler can return four types of responses, each signaling a different outcome to the client:

| Outcome | Status | Content-Type | Description |
|---------|--------|-------------|-------------|
| Success | 200 | `application/vnd.apache.arrow.stream` | The query executed successfully. The body contains the result data in Arrow IPC format. |
| Bad Request | 400 | `text/plain` | The client sent something invalid — a malformed query, an unsupported table, or a bad payment header. The body explains what went wrong. |
| Payment Required | 402 | `application/json` | The query is valid but requires payment. The body contains the x402 V2 payment requirements (price, accepted networks, etc.). The same information is also available base64-encoded in the `Payment-Required` header. |
| Internal Error | 500 | `text/plain` | Something unexpected failed on the server side (database error, serialization failure, facilitator unreachable). |

## Helper Functions

The handler delegates to several private helpers to keep the main function readable:

- **`run_query_to_ipc`** — executes a query and serializes the results to Arrow IPC bytes.
- **`estimate_row_count`** — wraps a query in `COUNT(*)` to estimate the number of rows (skipped for fixed-price tables).
- **`execute_db_query`** — acquires the database lock and runs a query, returning Arrow record batches.
- **`decode_payment_payload`** — base64-decodes and deserializes the `Payment-Signature` header into a V2 `PaymentPayload`.
- **`process_payment`** — orchestrates the per-row verify/settle cycle (execute first, then verify).
- **`process_fixed_price_payment`** — orchestrates the fixed-price verify/settle cycle (verify first, then execute).

## Server Overload Vector

For **per-row** pricing, the server runs the full query before verifying the payment. So an attacker can repeatedly submit expensive queries with bogus Payment-Signature headers and the server will execute every one of them.

**Fixed-price tables are not affected** — the `process_fixed_price_payment` flow verifies payment with the facilitator *before* executing the query. Bogus payment headers are rejected without touching the database.

For per-row tables, the attack surface has two layers:

1. Estimation abuse (Step 5) — `COUNT(*)` queries are cheap but still hit the database. High volume could saturate the mutex.
2. Execution abuse (Step 6) — full queries run before payment is verified. Needs more computation, can be arbitrarily expensive.

**Possible mitigations for per-row tables**:

Verify before executing — move payment verification before execute_db_query. Use an estimated row count (like in Step 5, recomputed cheaply) instead of actual rows for matching. This eliminates the expensive work for invalid payments. The tradeoff: the actual row count might differ from the estimate. (Minor improvement)

Proof-of-intent deposit — require a payment signature at the estimation step, not just at execution. A successful request would then involve two payments signatures: one for the estimate, one for the data. The server would need additional logic to decide when to charge the estimation fee (e.g., always, or only after repeated requests). This shifts the cost of abuse to the attacker but adds protocol complexity.

Query cost cap — reject queries above a certain estimated cost (row count, complexity). This bounds the damage per request.

Rate limiting — add a middleware layer (by IP, by wallet address from the payment header, etc.) to cap requests per time window. Cheap to implement, but doesn't prevent slow, sustained abuse.
