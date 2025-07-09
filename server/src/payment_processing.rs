//! Payment processing module for facilitator interactions.
//! 
//! This module handles the low-level communication with the x402 facilitator
//! for payment verification and settlement. The payment configuration and
//! requirements creation are handled by the `payment_config` module.

use std::sync::Arc;
use x402_rs::types::{
    SettleRequest, VerifyRequest, VerifyResponse
};
use crate::facilitator_client::{FacilitatorClient, FacilitatorClientError};

/// Helper function to verify payment with the facilitator
pub async fn verify_payment(
    facilitator: &Arc<FacilitatorClient>,
    verify_request: &VerifyRequest,
) -> Result<VerifyResponse, FacilitatorClientError> {
    let verify_response = facilitator.verify(verify_request).await?;
    Ok(verify_response)
}

/// Helper function to settle payment with the facilitator
pub async fn settle_payment(
    verify_response: VerifyResponse,
    facilitator: &Arc<FacilitatorClient>,
    verify_request: VerifyRequest
) -> Result<(), Box<dyn std::error::Error>> {
    match verify_response {
        VerifyResponse::Valid { .. } => {
            // Settle payment
            let settle_request = SettleRequest {
                x402_version: verify_request.x402_version,
                payment_payload: verify_request.payment_payload,
                payment_requirements: verify_request.payment_requirements,
            };
            
            let settle_response = facilitator.settle(&settle_request).await?;
            
            if settle_response.success {
                Ok(())
            } else {
                Err("Payment settlement failed".into())
            }
        }
        VerifyResponse::Invalid { reason, .. } => {
            Err(format!("Payment verification failed: {}", reason).into())
        }
    }
} 