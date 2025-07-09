use std::collections::HashMap;
use std::sync::Arc;
use url::Url;
use serde_json::json;
use x402_rs::types::{
    MixedAddress, PaymentPayload, PaymentRequiredResponse, PaymentRequirements, Scheme, X402Version
};

use crate::facilitator_client::FacilitatorClient;
use crate::price::PriceTag;

/// Payment offer configuration for a specific table
#[derive(Clone, Debug)]
pub struct TablePaymentOffers {
    /// Table name
    pub table_name: String,
    /// Available payment options for this table
    pub price_tags: Vec<PriceTag>,
    /// Whether this table requires payment
    pub requires_payment: bool,
    /// Custom description for this table's payment requirements
    pub description: Option<String>,
}

impl TablePaymentOffers {
    /// Creates a new TablePaymentOffer
    pub fn new(table_name: String, payment_offers: Vec<PriceTag>) -> Self {
        let requires_payment = !payment_offers.is_empty();
        Self {
            table_name,
            price_tags: payment_offers,
            requires_payment,
            description: None,
        }
    }

    /// Adds a payment offer to this table
    pub fn with_payment_offer(mut self, offer: PriceTag) -> Self {
        self.price_tags.push(offer);
        self.requires_payment = true;
        self
    }

    /// Sets a custom description for this table
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }
}

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
        let matching_payment_requirements = payment_requirements.iter().filter(|requirement| {
            requirement.scheme == payment_payload.scheme
                && requirement.network == payment_payload.network
                && match &requirement.pay_to {
                    MixedAddress::Evm(address) => address == &payment_payload.payload.authorization.to,
                    MixedAddress::Offchain(_address) => true // assume valid if offchain
                }
                && requirement.max_amount_required == payment_payload.payload.authorization.value

        }).cloned().collect::<Vec<_>>();
        
        matching_payment_requirements
    }

    /// Creates a payment required response using the new configuration system
    pub fn create_payment_required_response(
        &self,
        error: &str,
        table_name: &str,
        estimated_rows: usize,
        path: &str,
    ) -> Option<PaymentRequiredResponse> {
        let payment_requirements = self.get_all_payment_requirements(table_name, estimated_rows, path);
        if payment_requirements.is_empty() {
            return None;
        }
        Some(PaymentRequiredResponse {
            error: error.to_string(),
            accepts: payment_requirements,
            payer: None,
            x402_version: X402Version::V1,
        })
    }
    
    /// Gets all available payment requirements for a table (for 402 responses)
    pub fn get_all_payment_requirements(
        &self,
        table_name: &str,
        estimated_rows: usize,
        path: &str,
    ) -> Vec<PaymentRequirements> {
        let mut requirements = Vec::new();
        let table_offer = self.get_table_offer(table_name);
        if table_offer.is_none() {
            return requirements;
        }
        let table_offer = table_offer.unwrap();
        
        for offer in &table_offer.price_tags {
            if offer.is_in_range(estimated_rows) {
                if let Some(req) = self.create_payment_requirements_for_offer(
                    table_name,
                    estimated_rows,
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
                std::cmp::max(calculated_price, min_total_amount)
            }
            None => calculated_price,
        };
        
        let resource_url = self.base_url.join(path).ok()?;
        
        let table_offer = self.get_table_offer(table_name)?;
        let description = table_offer.description
            .as_ref()
            .unwrap_or(&self.default_description)
            .clone();
        
        Some(PaymentRequirements {
            scheme: Scheme::Exact,
            network: offer.token.network(),
            max_amount_required: total_price,
            resource: resource_url,
            description: format!("{} - {} rows", description, item_count),
            mime_type: self.mime_type.clone(),
            pay_to: offer.pay_to.into(),
            max_timeout_seconds: self.max_timeout_seconds,
            asset: offer.token.address().into(),
            extra: Some(json!({
                "name": offer.token.eip712.name,
                "version": offer.token.eip712.version,
            })),
            output_schema: None,
        })
    }

    // /// Calculates the total price for a table and item count
    // pub fn calculate_total_price(&self, table_name: &str, item_count: usize) -> Option<TokenAmount> {
    //     let table_offer = self.get_table_offer(table_name)?;
    //     let offer = table_offer.find_applicable_offer(item_count)?;
    //     Some(offer.calculate_total_price(item_count))
    // }
}
