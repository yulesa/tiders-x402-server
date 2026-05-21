//! x402 payment plumbing.
//!
//! - [`price`] — pricing model types (`PricingModel`, `PriceTag`, `TablePaymentOffers`).
//! - [`config`] — `GlobalPaymentConfig`: matches a request to a price tier and builds
//!   x402 `PaymentRequirements` for 402 responses.
//! - [`processing`] — orchestrates `verify` + `settle` calls against the facilitator.
//! - [`facilitator_client`] — thin HTTP client for the remote x402 facilitator.

pub mod cdp_jwt;
pub mod config;
pub mod facilitator_client;
pub mod price;
pub mod processing;
