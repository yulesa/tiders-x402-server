# Facilitator Client

The facilitator client (`server/src/facilitator_client.rs`) communicates with a remote x402 facilitator service to verify and settle payments.

## What is a Facilitator?

An x402 facilitator is a third-party service that handles the blockchain-side payment operations:

- **Verify** -- Confirms that a payment payload is valid and the transaction is properly signed and funded.
- **Settle** -- Executes the on-chain payment transfer.
- **Supported** -- Reports which payment schemes and networks are supported.

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

### Construction

From a URL string:

```rust
let facilitator = FacilitatorClient::try_from("https://facilitator.x402.rs")
    .expect("Failed to create facilitator client");
```

Or from a `Url`:

```rust
let facilitator = FacilitatorClient::try_new(url)?;
```

### Configuration

```rust
// Add custom headers
let client = facilitator.with_headers(headers);

// Set request timeout
let client = facilitator.with_timeout(Duration::from_secs(30));
```

### Facilitator Trait

The client implements the `x402_rs::facilitator::Facilitator` trait:

```rust
impl Facilitator for FacilitatorClient {
    async fn verify(&self, request: &VerifyRequest) -> Result<VerifyResponse, Error>;
    async fn settle(&self, request: &SettleRequest) -> Result<SettleResponse, Error>;
    async fn supported(&self) -> Result<SupportedResponse, Error>;
}
```

### Error Types

```rust
pub enum FacilitatorClientError {
    UrlParse { context, source },           // Invalid URL
    Http { context, source },               // Network/transport error
    JsonDeserialization { context, source }, // Response parsing failed
    HttpStatus { context, status, body },   // Non-200 response
    ResponseBodyRead { context, source },   // Could not read body
}
```

### Telemetry

All facilitator requests are instrumented with OpenTelemetry tracing spans. Each request records:
- `otel.status_code` -- "OK" or "ERROR"
- `error.message` -- Error details on failure
