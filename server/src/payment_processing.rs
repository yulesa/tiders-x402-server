//! Payment verification and settlement via the x402 facilitator.
//!
//! Translates between the server's V2 types and the facilitator's wire format.
//! This module is stateless — pricing logic lives in [`crate::payment_config`],
//! and HTTP transport lives in [`crate::facilitator_client`].

use std::sync::Arc;
use x402_types::proto;
use x402_types::proto::v2;
use crate::facilitator_client::FacilitatorClient;

/// Verifies a payment with the facilitator.
///
/// Builds a V2 verify request, converts it to the wire format, and sends it.
/// Returns both the wire-format request (needed later for settlement) and the
/// typed V2 response so the caller can inspect the result.
pub async fn verify_payment(
    facilitator: &Arc<FacilitatorClient>,
    payment_payload: &v2::PaymentPayload<v2::PaymentRequirements, serde_json::Value>,
    payment_requirements: &v2::PaymentRequirements,
) -> Result<(proto::VerifyRequest, v2::VerifyResponse), Box<dyn std::error::Error>> {
    let v2_verify_request = v2::VerifyRequest {
        x402_version: v2::X402Version2,
        payment_payload: payment_payload.clone(),
        payment_requirements: payment_requirements.clone(),
    };
    let proto_verify_request: proto::VerifyRequest = (&v2_verify_request).try_into()?;
    let proto_response = facilitator.verify(&proto_verify_request).await?;
    let v2_response: v2::VerifyResponse = proto_response.try_into()?;
    Ok((proto_verify_request, v2_response))
}

/// Settles a verified payment with the facilitator.
///
/// Only proceeds if the verification was valid. Reuses the wire-format verify
/// request as the settle request. Returns an error if the facilitator rejects
/// the settlement or if the verification was invalid.
pub async fn settle_payment(
    verify_response: v2::VerifyResponse,
    facilitator: &Arc<FacilitatorClient>,
    verify_request: proto::VerifyRequest,
) -> Result<(), Box<dyn std::error::Error>> {
    match verify_response {
        v2::VerifyResponse::Valid { .. } => {
            let settle_response_proto = facilitator.settle(&verify_request).await?;
            let settle_response: v2::SettleResponse = serde_json::from_value(settle_response_proto.0)?;

            match settle_response {
                v2::SettleResponse::Success { .. } => Ok(()),
                v2::SettleResponse::Error { reason, .. } => {
                    Err(format!("Payment settlement failed: {}", reason).into())
                }
            }
        }
        v2::VerifyResponse::Invalid { reason, .. } => {
            Err(format!("Payment verification failed: {}", reason).into())
        }
    }
}
