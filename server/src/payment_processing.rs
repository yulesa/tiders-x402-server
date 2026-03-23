//! Payment processing module for facilitator interactions.
//!
//! This module handles the low-level communication with the x402 facilitator
//! for payment verification and settlement. The payment configuration and
//! requirements creation are handled by the `payment_config` module.

use std::sync::Arc;
use x402_types::proto;
use x402_types::proto::v2;
use crate::facilitator_client::FacilitatorClient;

/// Helper function to verify payment with the facilitator.
///
/// Builds a V2 verify request, converts it to the proto wire format,
/// and sends it to the facilitator. Returns both the proto request
/// (for reuse in settlement) and the typed V2 response.
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

/// Helper function to settle payment with the facilitator.
///
/// Reuses the proto verify request as the settle request (same wire format).
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
