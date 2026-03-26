# Facilitator Client

The facilitator client (`server/src/facilitator_client.rs`) is responsible for communicating with a remote x402 facilitator service. It handles the HTTP details so the rest of the server can verify and settle payments through simple function calls.

## What is a Facilitator?

An x402 facilitator is a third-party service that handles the blockchain side of payments. The server never interacts with the blockchain directly — instead, it delegates to the facilitator for three operations:

- **Verify** — confirms that a payment payload is valid, properly signed, and funded.
- **Settle** — executes the on-chain payment transfer.
- **Supported** — reports which payment schemes and networks the facilitator can handle.

The default public facilitator is at `https://facilitator.x402.rs`.

## FacilitatorClient

```rust
pub struct FacilitatorClient {
    base_url: Url,
    verify_url: Url,      // base_url + "./verify"
    settle_url: Url,       // base_url + "./settle"
    supported_url: Url,    // base_url + "./supported"
    client: Client,        // reqwest HTTP client (shared connection pool)
    headers: HeaderMap,    // optional custom headers
    timeout: Option<Duration>,
}
```

`FacilitatorClient` wraps an HTTP client pointed at a facilitator's base URL. On construction, it derives the `/verify`, `/settle`, and `/supported` endpoint URLs automatically.

The client can be safely reused across concurrent requests.

**Construction**

The client can be created from a URL string or a parsed `Url`:

```rust
let facilitator = FacilitatorClient::try_from("https://facilitator.x402.rs")
    .expect("Failed to create facilitator client");
```

**Configuration**

The client supports optional customization after creation:

```rust
// Rust
let facilitator = facilitator.with_headers(header_map);
let facilitator = facilitator.with_timeout(Duration::from_millis(5000));

// Read back
println!("{}", facilitator.base_url());
println!("{}", facilitator.verify_url());
println!("{}", facilitator.settle_url());
println!("{:?}", facilitator.timeout());
```

```python
# Python
facilitator.set_headers({"Authorization": "Bearer token123"})
facilitator.set_timeout(5000)  # milliseconds

# Getters (properties)
print(facilitator.base_url)
print(facilitator.verify_url)
print(facilitator.settle_url)
print(facilitator.timeout_ms)  # returns int or None
```

See the [Configuration Reference](./configuration.md#facilitatorclient) for the full API.

## Facilitator Trait

The client implements the `x402_types::facilitator::Facilitator` trait, which defines the `verify`, `settle`, and `supported` methods. This allows it to be used interchangeably with other facilitator implementations (e.g., a local one for testing).

## Error Handling

Errors are captured with context about where the failure occurred:

| Error | Meaning |
|-------|---------|
| `UrlParse` | The facilitator URL or an endpoint path could not be parsed |
| `Http` | A network or transport error occurred (connection refused, DNS failure, timeout) |
| `JsonDeserialization` | The facilitator returned a response that could not be parsed as JSON |
| `HttpStatus` | The facilitator returned a non-200 status code |
| `ResponseBodyRead` | The response body could not be read as text |

## Telemetry

All facilitator requests are wrapped in OpenTelemetry tracing spans. Each span records the outcome (`otel.status_code` as `"OK"` or `"ERROR"`) and, on failure, the error details. This makes facilitator latency and errors visible in the server's observability pipeline.