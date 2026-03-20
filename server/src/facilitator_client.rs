//! A [`x402_rs::facilitator::Facilitator`] implementation that interacts with a _remote_ x402 Facilitator over HTTP.
//!
//! This [`FacilitatorClient`] handles the `/verify` and `/settle` endpoints of a remote facilitator,
//! and implements the [`x402_rs::facilitator::Facilitator`] trait for compatibility
//! with x402-based middleware and logic.
//!
//! ## Example
//!
//! ```rust
//! use x402_axum::facilitator_client::FacilitatorClient;
//!
//! let facilitator = FacilitatorClient::try_from("https://facilitator.ukstv.me/").unwrap();
//! ```
//! This client is cheap to clone and internally shares a connection pool via `reqwest::Client`,
//! making it safe and efficient to reuse across multiple Axum routes or concurrent tasks.
//!
//! ## Features
//!
//! - Uses `reqwest` for async HTTP requests
//! - Supports optional timeout and headers
//! - Integrates with `tracing` if the `telemetry` feature is enabled
//!
//! ## Error Handling
//!
//! Custom error types capture detailed failure contexts, including
//! - URL construction
//! - HTTP transport failures
//! - JSON deserialization errors
//! - Unexpected HTTP status responses

use http::{HeaderMap, StatusCode};
use reqwest::Client;
use std::fmt::Display;
use std::time::Duration;
use url::Url;
use x402_rs::facilitator::Facilitator;
use x402_rs::proto;
use tracing::{Span, Instrument};

/// A client for communicating with a remote x402 facilitator.
///
/// Handles `/verify`, `/settle`, and `/supported` endpoints via JSON HTTP POST/GET.
#[derive(Clone, Debug)]
pub struct FacilitatorClient {
    /// Base URL of the facilitator (e.g. `https://facilitator.example/`)
    #[allow(dead_code)] // Public for consumption by downstream crates.
    base_url: Url,
    /// Full URL to `POST /verify` requests
    verify_url: Url,
    /// Full URL to `POST /settle` requests
    settle_url: Url,
    /// Full URL to `GET /supported` requests
    supported_url: Url,
    /// Shared Reqwest HTTP client
    client: Client,
    /// Optional custom headers sent with each request
    headers: HeaderMap,
    /// Optional request timeout
    timeout: Option<Duration>,
}

impl Facilitator for FacilitatorClient {
    type Error = FacilitatorClientError;

    /// Verifies a payment payload with the facilitator.
    async fn verify(
        &self,
        request: &proto::VerifyRequest,
    ) -> Result<proto::VerifyResponse, FacilitatorClientError> {
        with_span(
            FacilitatorClient::verify(self, request),
            tracing::info_span!("x402.facilitator_client.verify", timeout = ?self.timeout),
        )
        .await
    }

    /// Attempts to settle a verified payment with the facilitator.
    async fn settle(
        &self,
        request: &proto::SettleRequest,
    ) -> Result<proto::SettleResponse, FacilitatorClientError> {
        with_span(
            FacilitatorClient::settle(self, request),
            tracing::info_span!("x402.facilitator_client.settle", timeout = ?self.timeout),
        )
        .await
    }

    /// Queries the facilitator for supported payment kinds.
    async fn supported(
        &self,
    ) -> Result<proto::SupportedResponse, FacilitatorClientError> {
        with_span(
            FacilitatorClient::supported(self),
            tracing::info_span!("x402.facilitator_client.supported", timeout = ?self.timeout),
        )
        .await
    }
}

/// Errors that can occur while interacting with a remote facilitator.
#[derive(Debug, thiserror::Error)]
pub enum FacilitatorClientError {
    #[error("URL parse error: {context}: {source}")]
    UrlParse {
        context: &'static str,
        #[source]
        source: url::ParseError,
    },
    #[error("HTTP error: {context}: {source}")]
    Http {
        context: &'static str,
        #[source]
        source: reqwest::Error,
    },
    #[error("Failed to deserialize JSON: {context}: {source}")]
    JsonDeserialization {
        context: &'static str,
        #[source]
        source: reqwest::Error,
    },
    #[error("Unexpected HTTP status {status}: {context}: {body}")]
    HttpStatus {
        context: &'static str,
        status: StatusCode,
        body: String,
    },
    #[error("Failed to read response body as text: {context}: {source}")]
    ResponseBodyRead {
        context: &'static str,
        #[source]
        source: reqwest::Error,
    },
}

impl FacilitatorClient {
    /// Returns the base URL used by this client.
    #[allow(dead_code)] // Public for consumption by downstream crates.
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    /// Returns the computed `./verify` URL relative to [`FacilitatorClient::base_url`].
    #[allow(dead_code)] // Public for consumption by downstream crates.
    pub fn verify_url(&self) -> &Url {
        &self.verify_url
    }

    /// Returns the computed `./settle` URL relative to [`FacilitatorClient::base_url`]
    #[allow(dead_code)] // Public for consumption by downstream crates.
    pub fn settle_url(&self) -> &Url {
        &self.settle_url
    }

    /// Returns any custom headers configured on the client.
    #[allow(dead_code)] // Public for consumption by downstream crates.
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Returns the configured timeout, if any.
    #[allow(dead_code)] // Public for consumption by downstream crates.
    pub fn timeout(&self) -> &Option<Duration> {
        &self.timeout
    }

    /// Constructs a new [`FacilitatorClient`] from a base URL.
    ///
    /// This sets up `./verify`, `./settle`, and `./supported` endpoint URLs relative to the base.
    pub fn try_new(base_url: Url) -> Result<Self, FacilitatorClientError> {
        let client = Client::new();
        let verify_url =
            base_url
                .join("./verify")
                .map_err(|e| FacilitatorClientError::UrlParse {
                    context: "Failed to construct ./verify URL",
                    source: e,
                })?;
        let settle_url =
            base_url
                .join("./settle")
                .map_err(|e| FacilitatorClientError::UrlParse {
                    context: "Failed to construct ./settle URL",
                    source: e,
                })?;
        let supported_url =
            base_url
                .join("./supported")
                .map_err(|e| FacilitatorClientError::UrlParse {
                    context: "Failed to construct ./supported URL",
                    source: e,
                })?;
        Ok(Self {
            client,
            base_url,
            verify_url,
            settle_url,
            supported_url,
            headers: HeaderMap::new(),
            timeout: None,
        })
    }

    /// Attaches custom headers to all future requests.
    #[allow(dead_code)] // Public for consumption by downstream crates.
    pub fn with_headers(&self, headers: HeaderMap) -> Self {
        let mut this = self.clone();
        this.headers = headers;
        this
    }

    /// Sets a timeout for all future requests.
    #[allow(dead_code)] // Public for consumption by downstream crates.
    pub fn with_timeout(&self, timeout: Duration) -> Self {
        let mut this = self.clone();
        this.timeout = Some(timeout);
        this
    }

    /// Sends a `POST /verify` request to the facilitator.
    pub async fn verify(
        &self,
        request: &proto::VerifyRequest,
    ) -> Result<proto::VerifyResponse, FacilitatorClientError> {
        self.post_json(&self.verify_url, "POST /verify", request)
            .await
    }

    /// Sends a `POST /settle` request to the facilitator.
    pub async fn settle(
        &self,
        request: &proto::SettleRequest,
    ) -> Result<proto::SettleResponse, FacilitatorClientError> {
        self.post_json(&self.settle_url, "POST /settle", request)
            .await
    }

    /// Sends a `GET /supported` request to the facilitator.
    pub async fn supported(
        &self,
    ) -> Result<proto::SupportedResponse, FacilitatorClientError> {
        let mut req = self.client.get(self.supported_url.clone());
        for (key, value) in self.headers.iter() {
            req = req.header(key, value);
        }
        if let Some(timeout) = self.timeout {
            req = req.timeout(timeout);
        }
        let http_response = req
            .send()
            .await
            .map_err(|e| FacilitatorClientError::Http { context: "GET /supported", source: e })?;

        let result = if http_response.status() == StatusCode::OK {
            http_response
                .json::<proto::SupportedResponse>()
                .await
                .map_err(|e| FacilitatorClientError::JsonDeserialization { context: "GET /supported", source: e })
        } else {
            let status = http_response.status();
            let body = http_response
                .text()
                .await
                .map_err(|e| FacilitatorClientError::ResponseBodyRead { context: "GET /supported", source: e })?;
            Err(FacilitatorClientError::HttpStatus {
                context: "GET /supported",
                status,
                body,
            })
        };

        record_result_on_span(&result);

        result
    }

    /// Generic POST helper that handles JSON serialization, error mapping,
    /// timeout application, and telemetry integration.
    async fn post_json<T, R>(
        &self,
        url: &Url,
        context: &'static str,
        payload: &T,
    ) -> Result<R, FacilitatorClientError>
    where
        T: serde::Serialize + ?Sized,
        R: serde::de::DeserializeOwned,
    {
        let mut req = self.client.post(url.clone()).json(payload);
        for (key, value) in self.headers.iter() {
            req = req.header(key, value);
        }
        if let Some(timeout) = self.timeout {
            req = req.timeout(timeout);
        }
        let http_response = req
            .send()
            .await
            .map_err(|e| FacilitatorClientError::Http { context, source: e })?;

        let result = if http_response.status() == StatusCode::OK {
            http_response
                .json::<R>()
                .await
                .map_err(|e| FacilitatorClientError::JsonDeserialization { context, source: e })
        } else {
            let status = http_response.status();
            let body = http_response
                .text()
                .await
                .map_err(|e| FacilitatorClientError::ResponseBodyRead { context, source: e })?;
            Err(FacilitatorClientError::HttpStatus {
                context,
                status,
                body,
            })
        };

        record_result_on_span(&result);

        result
    }
}

/// Converts a string URL into a `FacilitatorClient`, parsing the URL and calling `try_new`.
impl TryFrom<&str> for FacilitatorClient {
    type Error = FacilitatorClientError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        // Normalize: strip trailing slashes and add a single trailing slash
        let mut normalized = value.trim_end_matches('/').to_string();
        normalized.push('/');
        let url = Url::parse(&normalized).map_err(|e| FacilitatorClientError::UrlParse {
            context: "Failed to parse base url",
            source: e,
        })?;
        FacilitatorClient::try_new(url)
    }
}

/// Records the outcome of a request on a tracing span, including status and errors.
fn record_result_on_span<R, E: Display>(result: &Result<R, E>) {
    let span = Span::current();
    match result {
        Ok(_) => {
            span.record("otel.status_code", "OK");
        }
        Err(err) => {
            span.record("otel.status_code", "ERROR");
            span.record("error.message", tracing::field::display(err));
            tracing::event!(tracing::Level::ERROR, error = %err, "Request to facilitator failed");
        }
    }
}

/// Instruments a future with a given tracing span.
fn with_span<F: Future>(fut: F, span: Span) -> impl Future<Output = F::Output> {
    fut.instrument(span)
}
