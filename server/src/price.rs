use std::fmt::Debug;
use x402_rs::types::{EvmAddress, TokenDeployment};
use x402_rs::types::TokenAmount;
use alloy::primitives::U256;

/// A complete x402-compatible price tag, describing a required payment.
///
/// A `PriceTag` specifies a target recipient (`pay_to`), a token-denominated amount per item (row, cell, size, etc.),
/// and an associated ERC-20 asset. It can be used by sellers to declare required payments
/// or by facilitators to verify compliance.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PriceTag {
    pub pay_to: EvmAddress,
    pub amount_per_item: TokenAmount,
    pub token: TokenDeployment,
    pub min_total_amount: Option<TokenAmount>,
    pub min_items: Option<usize>,
    pub max_items: Option<usize>,
    pub description: Option<String>,
    pub is_default: bool,
}

impl PriceTag {
    /// Constructs a new `PriceTag` from raw inputs.
    pub fn new<P: Into<EvmAddress>, T: Into<TokenAmount>, A: Into<TokenDeployment>>(
        pay_to: P,
        amount_per_item: T,
        token: A,
        min_total_amount: Option<T>,
        min_items: Option<usize>,
        max_items: Option<usize>,
        description: Option<String>,
        is_default: bool,
    ) -> Self {
        Self {
            pay_to: pay_to.into(),
            amount_per_item: amount_per_item.into(),
            token: token.into(),
            min_total_amount: min_total_amount.map(|t| t.into()),
            min_items,
            max_items,
            description: description,
            is_default: is_default,
        }
    }

    /// Checks if this pricing tier is in range for the given item count
    pub fn is_in_range(&self, item_count: usize) -> bool {
        if let Some(min) = self.min_items {
            if item_count < min {
                return false;
            }
        }
        if let Some(max) = self.max_items {
            if item_count > max {
                return false;
            }
        }
        true
    }

    /// Calculates the total price for the given item count
    pub fn calculate_total_price(&self, item_count: usize) -> TokenAmount {
        let items_u256 = U256::from(item_count);
        let total = self.amount_per_item * items_u256;
        total
    }
}

impl From<PriceTag> for Vec<PriceTag> {
    fn from(value: PriceTag) -> Self {
        vec![value]
    }
}

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

    pub fn new_free_table(table_name: String) -> Self {
        Self {
            table_name,
            price_tags: vec![],
            requires_payment: false,
            description: None,
        }
    }

    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Adds a payment offer to this table
    pub fn with_payment_offer(mut self, offer: PriceTag) -> Self {
        self.price_tags.push(offer);
        self.requires_payment = true;
        self
    }
}
