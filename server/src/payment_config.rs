use std::collections::HashMap;
use std::sync::Arc;
use url::Url;
use serde_json::json;
use x402_rs::proto::v1::{
    PaymentPayload, PaymentRequired, PaymentRequirements, X402Version1,
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

    pub fn find_matching_payment_requirements(
        &self,
        table_name: &str,
        item_count: usize,
        path: &str,
        payment_payload: &PaymentPayload,
    ) ->  Vec<PaymentRequirements> {

        let payment_requirements = self.get_all_payment_requirements(table_name, item_count, path);
        // Parse the raw payload to extract authorization fields
        let payload_json: serde_json::Value = serde_json::from_str(payment_payload.payload.get())
            .unwrap_or(serde_json::Value::Null);

        let matching_payment_requirements = payment_requirements.iter().filter(|requirement| {
            requirement.scheme == payment_payload.scheme
                && requirement.network == payment_payload.network
                && requirement.pay_to == payload_json.get("authorization")
                    .and_then(|auth| auth.get("to"))
                    .and_then(|to| to.as_str())
                    .unwrap_or("")
                && requirement.max_amount_required == payload_json.get("authorization")
                    .and_then(|auth| auth.get("value"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")

        }).cloned().collect::<Vec<_>>();

        matching_payment_requirements
    }

    /// Creates a payment required response using the new configuration system
    pub fn create_payment_required_response(
        &self,
        error: &str,
        table_name: &str,
        estimated_items: usize,
        path: &str,
    ) -> Option<PaymentRequired> {
        let payment_requirements = self.get_all_payment_requirements(table_name, estimated_items, path);
        if payment_requirements.is_empty() {
            return None;
        }
        Some(PaymentRequired {
            error: Some(error.to_string()),
            accepts: payment_requirements,
            x402_version: X402Version1,
        })
    }

    /// Gets all available payment requirements for a table (for 402 responses)
    pub fn get_all_payment_requirements(
        &self,
        table_name: &str,
        estimated_items: usize,
        path: &str,
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
                    table_name,
                    estimated_items,
                    path,
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
        table_name: &str,
        item_count: usize,
        path: &str,
        offer: &PriceTag,
    ) -> Option<PaymentRequirements> {
        let calculated_price = offer.calculate_total_price(item_count);
        let total_price = match offer.min_total_amount {
            Some(min_total_amount) => {
                if calculated_price.0 > min_total_amount.0 {
                    calculated_price
                } else {
                    min_total_amount
                }
            }
            None => calculated_price,
        };

        let resource_url = self.base_url.join(path).ok()?;

        let table_offer = self.get_table_offer(table_name)?;
        let description = table_offer.description
            .as_ref()
            .unwrap_or(&self.default_description)
            .clone();

        let chain_id: x402_rs::chain::ChainId = offer.token.chain_reference.into();
        let network = chain_id.as_network_name()
            .unwrap_or_else(|| panic!("Unknown network for chain reference: {}", offer.token.chain_reference))
            .to_string();

        Some(PaymentRequirements {
            scheme: "exact".to_string(),
            network,
            max_amount_required: total_price.0.to_string(),
            resource: resource_url.to_string(),
            description: format!("{} - {} rows", description, item_count),
            mime_type: self.mime_type.clone(),
            pay_to: offer.pay_to.to_string(),
            max_timeout_seconds: self.max_timeout_seconds,
            asset: format!("{}", offer.token.address),
            extra: offer.token.eip712.as_ref().map(|eip712| json!({
                "name": eip712.name,
                "version": eip712.version,
            })),
            output_schema: None,
        })
    }
}
