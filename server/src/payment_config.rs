use std::collections::HashMap;
use std::sync::Arc;
use url::Url;
use x402_types::proto::v2::{
    PaymentRequired, PaymentRequirements, ResourceInfo, X402Version2,
};

use crate::facilitator_client::FacilitatorClient;
use crate::price::{PriceTag, TablePaymentOffers};

/// Global configuration for the payment system
#[derive(Clone, Debug)]
pub struct GlobalPaymentConfig {
    /// The facilitator client for payment verification and settlement
    pub facilitator: Arc<FacilitatorClient>,
    /// Base URL for constructing resource URLs
    pub base_url: Url,
    /// MIME type for responses
    pub mime_type: String,
    /// Maximum timeout for payment settlement in seconds
    pub max_timeout_seconds: u64,
    /// Default description for payment requirements. This is used if no description is provided for a table.
    pub default_description: String,
    /// Table-specific payment offers
    pub table_offers: HashMap<String, TablePaymentOffers>,
}

#[allow(dead_code)]
impl GlobalPaymentConfig {
    /// Creates a default configuration with common values
    pub fn default(facilitator: Arc<FacilitatorClient>, base_url: Url) -> Self {
        Self {
            facilitator,
            base_url,
            mime_type: "application/vnd.apache.arrow.stream".to_string(),
            max_timeout_seconds: 300,
            default_description: "Query execution payment".to_string(),
            table_offers: HashMap::new(),
        }
    }

    /// Adds a table payment offer to the configuration
    pub fn add_table_offer(&mut self, offer: TablePaymentOffers) {
        self.table_offers.insert(offer.table_name.clone(), offer);
    }

    /// Gets a table payment offers by name
    pub fn get_table_offer(&self, table_name: &str) -> Option<&TablePaymentOffers> {
        self.table_offers.get(table_name)
    }

    /// Checks if a table requires payment
    pub fn table_requires_payment(&self, table_name: &str) -> Option<bool> {
        match self.table_offers.get(table_name) {
            Some(offer) => Some(offer.requires_payment),
            None => None
        }
    }

    /// Finds the matching payment requirement for the provided V2 payment payload.
    ///
    /// V2 exact matching: the client echoes back the complete `PaymentRequirements`
    /// in `PaymentPayload.accepted`, so we just do a direct equality check.
    pub fn find_matching_payment_requirements(
        &self,
        table_name: &str,
        item_count: usize,
        accepted: &PaymentRequirements,
    ) -> Option<PaymentRequirements> {
        let requirements = self.get_all_payment_requirements(table_name, item_count);
        requirements.into_iter().find(|req| req == accepted)
    }

    /// Creates a payment required response using the new configuration system
    pub fn create_payment_required_response(
        &self,
        error: &str,
        table_name: &str,
        estimated_items: usize,
        path: &str,
    ) -> Option<PaymentRequired> {
        let payment_requirements = self.get_all_payment_requirements(table_name, estimated_items);
        if payment_requirements.is_empty() {
            return None;
        }

        let resource_url = self.base_url.join(path).ok()?;
        let table_offer = self.get_table_offer(table_name)?;
        let description = table_offer.description
            .as_ref()
            .unwrap_or(&self.default_description)
            .clone();

        Some(PaymentRequired {
            x402_version: X402Version2,
            error: Some(error.to_string()),
            resource: Some(ResourceInfo {
                url: resource_url.to_string(),
                description: Some(format!("{} - {} rows", description, estimated_items)),
                mime_type: Some(self.mime_type.clone()),
            }),
            accepts: payment_requirements,
        })
    }

    /// Gets all available payment requirements for a table (for 402 responses)
    pub fn get_all_payment_requirements(
        &self,
        table_name: &str,
        estimated_items: usize,
    ) -> Vec<PaymentRequirements> {
        let mut requirements = Vec::new();
        let table_offer = self.get_table_offer(table_name);
        if table_offer.is_none() {
            return requirements;
        }
        let table_offer = table_offer.unwrap();

        for offer in &table_offer.price_tags {
            if offer.is_in_range(estimated_items) {
                if let Some(req) = self.create_payment_requirements_for_offer(
                    estimated_items,
                    offer,
                ) {
                    requirements.push(req);
                }
            }
        }

        requirements
    }

    /// Creates payment requirements for a specific offer
    fn create_payment_requirements_for_offer(
        &self,
        item_count: usize,
        offer: &PriceTag,
    ) -> Option<PaymentRequirements> {
        let calculated_price = offer.calculate_total_price(item_count);
        let total_price = match offer.min_total_amount {
            Some(ref min_total_amount) => {
                if calculated_price.0 > min_total_amount.0 {
                    calculated_price
                } else {
                    min_total_amount.clone()
                }
            }
            None => calculated_price,
        };

        let chain_id: x402_types::chain::ChainId = offer.token.chain_reference.into();
        let extra = serde_json::to_value(&offer.token.transfer_method).ok();

        Some(PaymentRequirements {
            scheme: "exact".to_string(),
            network: chain_id,
            amount: total_price.0.to_string(),
            pay_to: offer.pay_to.to_string(),
            max_timeout_seconds: self.max_timeout_seconds,
            asset: format!("{}", offer.token.address),
            extra,
        })
    }
}
