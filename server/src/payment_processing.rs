//! Payment processing module for facilitator interactions.
//!
//! This module handles the low-level communication with the x402 facilitator
//! for payment verification and settlement. The payment configuration and
//! requirements creation are handled by the `payment_config` module.

use std::sync::Arc;
use x402_rs::proto;
use x402_rs::proto::v1;
use crate::facilitator_client::FacilitatorClient;

/// Helper function to verify payment with the facilitator
pub async fn verify_payment(
    facilitator: &Arc<FacilitatorClient>,
    verify_request: &proto::VerifyRequest,
) -> Result<v1::VerifyResponse, Box<dyn std::error::Error>> {
    let proto_response = facilitator.verify(verify_request).await?;
    let v1_response: v1::VerifyResponse = proto_response.try_into()?;
    Ok(v1_response)
}

/// Helper function to settle payment with the facilitator
pub async fn settle_payment(
    verify_response: v1::VerifyResponse,
    facilitator: &Arc<FacilitatorClient>,
    verify_request: proto::VerifyRequest,
) -> Result<(), Box<dyn std::error::Error>> {
    match verify_response {
        v1::VerifyResponse::Valid { .. } => {
            // The settle request is the same JSON as verify request
            let settle_response_proto = facilitator.settle(&verify_request).await?;
            let settle_response: v1::SettleResponse = serde_json::from_value(settle_response_proto.0)?;

            match settle_response {
                v1::SettleResponse::Success { .. } => Ok(()),
                v1::SettleResponse::Error { reason, .. } => {
                    Err(format!("Payment settlement failed: {}", reason).into())
                }
            }
        }
        v1::VerifyResponse::Invalid { reason, .. } => {
            Err(format!("Payment verification failed: {}", reason).into())
        }
    }
}
