//! Payment configuration and x402 V2 payment requirements generation.
//!
//! This module is the central place where pricing rules are defined. It
//! determines how much each query costs based on the target table and row
//! count, and generates the x402 V2 [`PaymentRequirements`] that clients
//! need to fulfill before receiving data.

use std::collections::HashMap;
use std::sync::Arc;
use url::Url;
use x402_types::proto::v2::{PaymentRequired, PaymentRequirements, ResourceInfo, X402Version2};

use crate::facilitator_client::FacilitatorClient;
use crate::price::{PriceTag, TablePaymentOffers};

/// Central payment configuration shared across all request handlers.
///
/// Holds the facilitator client, server identity, and per-table pricing
/// rules. The query handler consults this to decide whether a table is
/// free or paid, to build 402 responses, and to validate incoming payments.
#[derive(Clone, Debug)]
pub struct GlobalPaymentConfig {
    /// Client for verifying and settling payments with the x402 facilitator.
    pub facilitator: Arc<FacilitatorClient>,
    /// Response format advertised to clients (defaults to `"application/vnd.apache.arrow.stream"`).
    pub mime_type: String,
    /// How long a payment offer remains valid, in seconds (defaults to 300).
    pub max_timeout_seconds: u64,
    /// Fallback description used when a table has no description of its own.
    pub default_description: String,
    /// Per-table pricing and payment configuration, keyed by table name.
    pub offers_tables: HashMap<String, TablePaymentOffers>,
}

#[allow(dead_code)]
impl GlobalPaymentConfig {
    /// Creates a configuration with sensible defaults (Arrow IPC mime type, 300s timeout).
    pub fn default(facilitator: Arc<FacilitatorClient>) -> Self {
        Self {
            facilitator,
            mime_type: "application/vnd.apache.arrow.stream".to_string(),
            max_timeout_seconds: 300,
            default_description: "Query execution payment".to_string(),
            offers_tables: HashMap::new(),
        }
    }

    /// Registers a table and its pricing rules.
    pub fn add_offers_table(&mut self, offer: TablePaymentOffers) {
        self.offers_tables.insert(offer.table_name.clone(), offer);
    }

    /// Looks up the payment offers for a table, or `None` if the table is not configured.
    pub fn get_offers_table(&self, table_name: &str) -> Option<&TablePaymentOffers> {
        self.offers_tables.get(table_name)
    }

    /// Returns whether a table is free (`Some(false)`), paid (`Some(true)`),
    /// or not configured at all (`None`).
    pub fn table_requires_payment(&self, table_name: &str) -> Option<bool> {
        self.offers_tables
            .get(table_name)
            .map(|offer| offer.requires_payment)
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

    /// Assembles a full 402 response body with the error message, resource info,
    /// and all applicable payment options for the given table and row count.
    ///
    /// Returns `None` if no price tags apply or the table is not configured.
    pub fn create_payment_required_response(
        &self,
        error: &str,
        table_name: &str,
        estimated_items: usize,
        path: &str,
        server_base_url: &Url,
    ) -> Option<PaymentRequired> {
        let payment_requirements = self.get_all_payment_requirements(table_name, estimated_items);
        if payment_requirements.is_empty() {
            return None;
        }

        let resource_url = server_base_url.join(path).ok()?;
        let offers_table = self.get_offers_table(table_name)?;
        let description = offers_table
            .description
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

    /// Returns all payment requirements whose price tag range covers `estimated_items`.
    ///
    /// Each price tag with a matching `min_items`/`max_items` range produces one
    /// [`PaymentRequirements`] entry with the calculated price.
    pub fn get_all_payment_requirements(
        &self,
        table_name: &str,
        estimated_items: usize,
    ) -> Vec<PaymentRequirements> {
        let mut requirements = Vec::new();
        let offers_table = self.get_offers_table(table_name);
        if offers_table.is_none() {
            return requirements;
        }
        let offers_table = offers_table.unwrap();

        for offer in &offers_table.price_tags {
            if offer.is_in_range(estimated_items)
                && let Some(req) =
                    self.create_payment_requirements_for_offer(estimated_items, offer)
            {
                requirements.push(req);
            }
        }

        requirements
    }

    /// Converts a single price tag into a [`PaymentRequirements`] for the given row count.
    ///
    /// Calculates the total price (applying `min_total_amount` if set) and fills in
    /// the blockchain-specific fields (network, asset, pay_to, etc.).
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
