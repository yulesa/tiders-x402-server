use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::body::Bytes;
use std::sync::Arc;
use serde_json::json;
use url::Url;

use x402_rs::network::{Network, USDCDeployment};
use x402_rs::types::{
    PaymentRequiredResponse, 
    PaymentRequirements, Scheme, SettleRequest,
    VerifyRequest, VerifyResponse, X402Version
};
use crate::facilitator_client::{FacilitatorClient, FacilitatorClientError};
use crate::price::IntoPriceTag;

// Helper function to create payment requirements
pub fn create_payment_requirements(
    total_price: f64,
    table_name: &str,
    estimated_rows: usize,
    path: &str,
) -> PaymentRequirements {
    let usdc = USDCDeployment::by_network(Network::BaseSepolia);
    let pay_to_address = "0xE7a820f9E05e4a456A7567B79e433cc64A058Ae7";
    
    // Create USDC amount using the builder pattern
    let price_tag = usdc.pay_to(pay_to_address).amount(total_price).unwrap();
    
    PaymentRequirements {
        scheme: Scheme::Exact,
        network:  price_tag.token.network(),
        max_amount_required: price_tag.amount,
        resource: Url::parse(&format!("http://localhost:4021{}", path)).unwrap(),
        description: format!("Query on table '{}' returning {} rows", table_name, estimated_rows),
        mime_type: "application/vnd.apache.arrow.stream".to_string(),
        pay_to: price_tag.pay_to.into(),
        max_timeout_seconds: 300,
        asset: price_tag.token.asset.address.into(),
        extra: Some(json!({
            "name": price_tag.token.eip712.name,
            "version": price_tag.token.eip712.version
        })),
        output_schema: None,
    }
}

// Helper function to verify and settle payment
pub async fn verify_payment(
    facilitator: &Arc<FacilitatorClient>,
    verify_request: &VerifyRequest,
) -> Result<VerifyResponse, FacilitatorClientError> {
    let verify_response = facilitator.verify(verify_request).await?;
    Ok(verify_response)
}

// Helper function to settle payment
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

// Helper function to create payment required response
pub fn create_payment_required_response(
    error: &str,
    total_price: f64,
    table_name: &str,
    estimated_rows: usize,
    path: &str,
) -> Response {
    let payment_requirements = create_payment_requirements(
        total_price,
        table_name,
        estimated_rows,
        path,
    );
    
    let payment_required_response = PaymentRequiredResponse {
        error: error.to_string(),
        accepts: vec![payment_requirements],
        payer: None,
        x402_version: X402Version::V1,
    };

    let response_body = serde_json::to_vec(&payment_required_response)
        .expect("Failed to serialize payment response");
    let response = (
            StatusCode::PAYMENT_REQUIRED,
            [("content-type", "application/json")],
            Bytes::from(response_body),
        );
    tracing::info!("Request failed: {:?}", response);
    return response.into_response();
} 