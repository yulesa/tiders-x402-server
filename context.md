# Tiders x402 Server - Migration Context

## What Was Done

Migrated from the deprecated monolithic `x402-rs` (v0.12.5) crate to `x402-types` (v1.4.4) + `x402-chain-eip155` (v1.4.4). Switched from V1 to V2 x402 payment protocol, targeting Base chain with `v2-eip155-exact` scheme.

## Protocol Changes (V1 to V2)

- **Request header**: `X-Payment` replaced by `Payment-Signature`
- **402 response**: `Payment-Required` header with base64-encoded JSON + empty body (was JSON body)
- **Exact matching**: `PaymentPayload.accepted` contains full `PaymentRequirements`, enabling direct `PartialEq` check instead of V1's partial JSON field matching
- **Chain IDs**: Network names (`"base-sepolia"`) replaced by CAIP-2 chain IDs (`"eip155:84532"`) via `ChainId` type
- **Resource info**: Moved from per-requirement fields to top-level `ResourceInfo` struct in `PaymentRequired`
- **Verify/Settle**: `v2::VerifyRequest` serializes to `proto::VerifyRequest` via `TryFrom`; same proto request reused for settlement
- **Facilitator trait**: Now uses RPITIT (Rust 1.88+) instead of `async-trait`

## Dependency Changes

| Old | New |
|-----|-----|
| `x402-rs = "0.12.5"` | `x402-types = { version = "1.4.4", features = ["telemetry"] }` |
| | `x402-chain-eip155 = { version = "1.4.4", features = ["server"] }` |

Additional workspace deps added (previously transitive via x402-rs):
- `tracing-subscriber` with `env-filter`, `fmt` features
- `tokio` features: `signal`, `rt-multi-thread`, `macros`
- `tower-http` feature: `trace`

## Files Modified

| File | Change |
|------|--------|
| `Cargo.toml` | Replaced `x402-rs` dep, added `tracing-subscriber`, tokio/tower-http features |
| `server/Cargo.toml` | Updated workspace dep references |
| `python/Cargo.toml` | Updated workspace dep references |
| `server/src/facilitator_client.rs` | Import update only |
| `server/src/price.rs` | Import update + added local `TokenAmount` newtype |
| `server/src/payment_config.rs` | Full V2 rewrite: exact matching, `ResourceInfo`, `ChainId`, `transfer_method` serialization |
| `server/src/payment_processing.rs` | Full V2 rewrite: builds `v2::VerifyRequest`, proto conversion via `TryFrom` |
| `server/src/query_handler.rs` | V2 header/payload types, base64 `Payment-Required` header, single verify/settle cycle |
| `server/src/main.rs` | Import updates |
| `server/src/lib.rs` | Replaced `Telemetry` OTLP setup with `tracing_subscriber::fmt()` |
| `python/src/lib.rs` | Import updates, fixed `AppState` path bug |

## Notable Design Decisions

1. **TokenAmount**: New crates use `DeployedTokenAmount<U256, Eip155TokenDeployment>` instead of standalone `TokenAmount`. Kept a local `TokenAmount(pub U256)` newtype in `price.rs` for simplicity.
2. **Telemetry**: `x402_rs::util::Telemetry` (full OTLP setup) replaced with basic `tracing_subscriber::fmt()`. Full OTLP exporter can be re-added independently.
3. **Transfer method**: `AssetTransferMethod` enum (Eip3009/Permit2) replaces old `eip712` field, serialized into `extra` on `PaymentRequirements`.

## Build

Full workspace builds successfully:
```bash
PYO3_PYTHON=/usr/bin/python3 cargo build
```
(`PYO3_PYTHON` needed because system has `python3` but not `python` at `/usr/bin/python`)

## Current State

Migration is complete. No pending tasks. Possible next steps:
- Run manual/integration tests
- Commit the changes
- Re-add full OTLP telemetry if needed
