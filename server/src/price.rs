//! Pricing model for paid tables.
//!
//! Defines the data structures that describe how much a query costs
//! (`PriceTag`) and how tables are configured with pricing tiers
//! (`TablePaymentOffers`). Used by [`crate::payment_config`] to
//! generate x402 payment requirements.

use alloy_primitives::U256;
use arrow::datatypes::Schema;
use std::fmt::Debug;
use x402_chain_eip155::chain::{ChecksummedAddress, Eip155TokenDeployment};

/// A token amount in the token's smallest unit (e.g., wei for ETH, 10^-6 for USDC).
#[derive(Clone, Debug, PartialEq)]
pub struct TokenAmount(pub U256);

/// How the total price for a query is calculated.
#[derive(Clone, Debug, PartialEq)]
pub enum PricingModel {
    /// Price scales linearly with the number of rows returned.
    PerRow {
        /// Price per row in the token's smallest unit.
        amount_per_item: TokenAmount,
        /// Minimum row count for this tier to apply (inclusive). `None` means no lower bound.
        min_items: Option<usize>,
        /// Maximum row count for this tier to apply (inclusive). `None` means no upper bound.
        max_items: Option<usize>,
        /// Optional minimum charge, enforced even if the per-row calculation is lower.
        min_total_amount: Option<TokenAmount>,
    },
    /// A flat fee regardless of how many rows are returned.
    Fixed {
        /// The fixed amount charged for any query against this table.
        amount: TokenAmount,
    },
}

/// A single pricing tier for a table, describing who gets paid, how much,
/// and in which token.
///
/// A table can have multiple price tags (e.g., different tokens or tiers for
/// small vs. large queries). The [`crate::payment_config`] module selects which
/// ones apply for a given row count.
#[derive(Clone, Debug, PartialEq)]
pub struct PriceTag {
    /// Recipient wallet address.
    pub pay_to: ChecksummedAddress,
    /// The pricing model and its parameters.
    pub pricing: PricingModel,
    /// The ERC-20 token used for payment (chain, contract address, transfer method).
    pub token: Eip155TokenDeployment,
    /// Optional human-readable label for this tier.
    pub description: Option<String>,
    /// Whether this is the default pricing tier for the table.
    pub is_default: bool,
}

impl PriceTag {
    /// Returns `true` if `item_count` falls within this tier's range.
    /// Fixed-price tiers always return `true` (row count is irrelevant).
    pub fn is_in_range(&self, item_count: usize) -> bool {
        match &self.pricing {
            PricingModel::Fixed { .. } => true,
            PricingModel::PerRow {
                min_items,
                max_items,
                ..
            } => {
                if let Some(min) = min_items
                    && item_count < *min
                {
                    return false;
                }
                if let Some(max) = max_items
                    && item_count > *max
                {
                    return false;
                }
                true
            }
        }
    }

    /// Calculates the total price for the given item count.
    ///
    /// For [`PricingModel::PerRow`], returns `amount_per_item * item_count`.
    /// Does **not** apply `min_total_amount` — that enforcement happens in
    /// [`crate::payment_config`].
    ///
    /// For [`PricingModel::Fixed`], returns the flat amount (ignores `item_count`).
    pub fn calculate_total_price(&self, item_count: usize) -> TokenAmount {
        match &self.pricing {
            PricingModel::PerRow {
                amount_per_item, ..
            } => {
                let items_u256 = U256::from(item_count);
                let total = amount_per_item.0 * items_u256;
                TokenAmount(total)
            }
            PricingModel::Fixed { amount } => amount.clone(),
        }
    }

    /// Returns `true` if this price tag uses fixed pricing.
    pub fn is_fixed(&self) -> bool {
        matches!(self.pricing, PricingModel::Fixed { .. })
    }
}

impl From<PriceTag> for Vec<PriceTag> {
    fn from(value: PriceTag) -> Self {
        vec![value]
    }
}

/// Groups the payment configuration for a single table: its pricing tiers,
/// whether payment is required, and metadata shown to clients.
#[derive(Clone, Debug)]
pub struct TablePaymentOffers {
    /// The table this configuration applies to.
    pub table_name: String,
    /// Available pricing tiers for this table.
    pub price_tags: Vec<PriceTag>,
    /// Whether queries against this table require payment (derived from whether price tags exist).
    pub requires_payment: bool,
    /// Optional description shown in the root endpoint and 402 responses.
    pub description: Option<String>,
    /// Optional Arrow schema, displayed in the root endpoint to help clients discover columns.
    pub schema: Option<Schema>,
}

#[allow(dead_code)]
impl TablePaymentOffers {
    /// Creates a paid table with the given pricing tiers. Sets `requires_payment`
    /// based on whether any price tags are provided.
    pub fn new(table_name: String, payment_offers: Vec<PriceTag>, schema: Option<Schema>) -> Self {
        let requires_payment = !payment_offers.is_empty();
        Self {
            table_name,
            price_tags: payment_offers,
            requires_payment,
            description: None,
            schema,
        }
    }

    /// Creates a free table (no payment required, no price tags).
    pub fn new_free_table(table_name: String, schema: Option<Schema>) -> Self {
        Self {
            table_name,
            price_tags: vec![],
            requires_payment: false,
            description: None,
            schema,
        }
    }

    /// Sets a human-readable description for this table.
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Adds a pricing tier and marks the table as requiring payment.
    pub fn add_payment_offer(mut self, offer: PriceTag) -> Self {
        self.price_tags.push(offer);
        self.requires_payment = true;
        self
    }

    /// Removes a price tag by index. Returns `true` if the index was valid and the tag was removed.
    /// Updates `requires_payment` based on whether any price tags remain.
    pub fn remove_price_tag(&mut self, index: usize) -> bool {
        if index >= self.price_tags.len() {
            return false;
        }
        self.price_tags.remove(index);
        self.requires_payment = !self.price_tags.is_empty();
        true
    }

    /// Removes all price tags and marks the table as free (no payment required).
    pub fn make_free(&mut self) {
        self.price_tags.clear();
        self.requires_payment = false;
    }

    /// Returns `true` if all price tags use fixed pricing.
    /// Returns `false` if there are no price tags or any use per-row pricing.
    pub fn is_all_fixed_price(&self) -> bool {
        !self.price_tags.is_empty() && self.price_tags.iter().all(|tag| tag.is_fixed())
    }
}
