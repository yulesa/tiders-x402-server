//! Axum handler for the `GET /api/table/:name` endpoint.
//!
//! Returns full schema and payment offer details for a specific table as JSON.
//! If the table has a `MetadataPrice` price tag, the endpoint requires x402
//! payment before returning the data.

use crate::AppState;
use crate::payment_config::GlobalPaymentConfig;
use crate::payment_processing::{settle_payment, verify_payment};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use std::sync::Arc;
use tracing::instrument;
use x402_types::proto::v2::{PaymentPayload, PaymentRequirements, VerifyResponse};
use x402_types::util::Base64Bytes;

/// Handles `GET /api/table/:name` — returns full schema and payment offers as JSON.
///
/// If the table has a `MetadataPrice` tag, the caller must provide a valid
/// `Payment-Signature` header following the x402 protocol. Otherwise the
/// data is returned freely.
#[axum::debug_handler]
#[instrument(skip_all, fields(table = %name))]
#[allow(dead_code)]
pub async fn table_detail_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<axum::response::Response, TableDetailError> {
    let payment_config = state.payment_config.read().await.clone();
    let path = format!("/api/table/{name}");

    let offer = payment_config
        .offers_tables
        .get(&name)
        .ok_or_else(|| TableDetailError::NotFound(name.clone()))?;

    // If no metadata price tag, return the data freely
    if !offer.has_metadata_price() {
        return Ok(Json(offer.clone()).into_response());
    }

    // Metadata is paid — check for payment header
    match headers.get("Payment-Signature") {
        None => Err(TableDetailError::payment(
            &payment_config,
            "No crypto payment found. Implement x402 protocol (https://www.x402.org/) to pay for metadata access.".to_string(),
            &name,
            &path,
            &state.server_base_url,
        )),
        Some(payment_header) => {
            let payment_payload = decode_payment_payload(payment_header)?;

            // Match payment requirements
            let payment_requirement = payment_config
                .find_matching_metadata_payment_requirements(&name, &payment_payload.accepted)
                .ok_or_else(|| {
                    TableDetailError::Internal(
                        "No payment offer was found matching the provided payment payload"
                            .to_string(),
                    )
                })?;

            // Verify payment with the facilitator BEFORE returning data
            let (verify_request, verify_response) = verify_payment(
                &payment_config.facilitator,
                &payment_payload,
                &payment_requirement,
            )
            .await
            .map_err(|e| {
                TableDetailError::Internal(format!(
                    "Payment verification failed due to facilitator error: {e}"
                ))
            })?;

            if let VerifyResponse::Invalid { reason, .. } = &verify_response {
                return Err(TableDetailError::payment(
                    &payment_config,
                    format!("Payment provided is invalid, verification failed: {reason}"),
                    &name,
                    &path,
                    &state.server_base_url,
                ));
            }

            // Settle payment
            settle_payment(verify_response, &payment_config.facilitator, verify_request)
                .await
                .map_err(|e| {
                    TableDetailError::payment(
                        &payment_config,
                        format!("Settlement of the provided payment failed: {e}"),
                        &name,
                        &path,
                        &state.server_base_url,
                    )
                })?;

            Ok(Json(offer.clone()).into_response())
        }
    }
}

fn decode_payment_payload(
    payment_header: &HeaderValue,
) -> Result<PaymentPayload<PaymentRequirements, serde_json::Value>, TableDetailError> {
    let base64 = Base64Bytes::from(payment_header.as_bytes());
    let decoded = base64.decode().map_err(|e| {
        TableDetailError::BadRequest(format!("Failed to decode payment header: {e}"))
    })?;
    serde_json::from_slice(&decoded)
        .map_err(|e| TableDetailError::BadRequest(format!("Failed to parse payment payload: {e}")))
}

#[derive(Debug)]
pub enum TableDetailError {
    NotFound(String),
    BadRequest(String),
    Internal(String),
    PaymentRequired {
        header_value: String,
        json_body: Vec<u8>,
    },
}

impl TableDetailError {
    fn payment(
        payment_config: &GlobalPaymentConfig,
        message: String,
        table_name: &str,
        path: &str,
        server_base_url: &url::Url,
    ) -> Self {
        match payment_config.create_metadata_payment_required_response(
            &message,
            table_name,
            path,
            server_base_url,
        ) {
            Some(payment_response) => {
                let json_bytes = serde_json::to_vec(&payment_response)
                    .expect("Failed to serialize payment response");
                let encoded = Base64Bytes::encode(&json_bytes);
                let header_value =
                    String::from_utf8(encoded.0.into_owned()).expect("Base64 is valid UTF-8");
                Self::PaymentRequired {
                    header_value,
                    json_body: json_bytes,
                }
            }
            None => {
                Self::Internal("Failed to find payment options for metadata request".to_string())
            }
        }
    }
}

impl IntoResponse for TableDetailError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::NotFound(name) => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": format!("Table '{name}' not found")})),
            )
                .into_response(),
            Self::BadRequest(msg) => {
                tracing::info!("Table detail request failed: {msg}");
                (
                    StatusCode::BAD_REQUEST,
                    [("content-type", "text/plain")],
                    Bytes::from(msg),
                )
                    .into_response()
            }
            Self::Internal(msg) => {
                tracing::error!("Table detail request failed: {msg}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [("content-type", "text/plain")],
                    Bytes::from(msg),
                )
                    .into_response()
            }
            Self::PaymentRequired {
                header_value,
                json_body,
            } => (
                StatusCode::PAYMENT_REQUIRED,
                [
                    ("Payment-Required", header_value),
                    ("content-type", "application/json".to_string()),
                ],
                Bytes::from(json_body),
            )
                .into_response(),
        }
    }
}
