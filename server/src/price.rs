use std::fmt::Debug;
use x402_rs::network::USDCDeployment;
use x402_rs::types::{EvmAddress, TokenDeployment};
use x402_rs::types::{MoneyAmount, TokenAmount};

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
}

impl PriceTag {
    /// Constructs a new `PriceTag` from raw inputs.
    pub fn new<P: Into<EvmAddress>, T: Into<TokenAmount>, A: Into<TokenDeployment>>(
        pay_to: P,
        amount_per_item: T,
        token: A,
    ) -> Self {
        Self {
            pay_to: pay_to.into(),
            amount_per_item: amount_per_item.into(),
            token: token.into(),
        }
    }
}

impl From<PriceTag> for Vec<PriceTag> {
    fn from(value: PriceTag) -> Self {
        vec![value]
    }
}

/// Intermediate builder struct for constructing a [`PriceTag`] using fluent chaining.
///
/// Allows creation of price tags using either token amounts or human-readable values
/// (e.g., `"1.5"` USDC per item). Generic over the amount and payee representations.
#[derive(Clone, Debug)]
pub struct PriceTagBuilder<A, P> {
    token: TokenDeployment,
    amount_per_item: Option<A>,
    pay_to: Option<P>,
}

/// Wrapper type used to distinguish [`PriceTagBuilder::amount_per_item`] created with human-friendly money values.
///
/// These must be converted to token amounts using the associated asset's decimal precision.
#[derive(Clone, Debug)]
pub struct PriceTagMoneyAmountPerItem<A>(A);

/// Wrapper type used to distinguish [`PriceTagBuilder::amount_per_item`] created with exact token-denominated values.
#[derive(Clone, Debug)]
pub struct PriceTagTokenAmountPerItem<A>(A);

/// Converts the wrapped value into a [`TokenAmount`] using [`TryInto`].
///
/// Used internally by [`PriceTagBuilder`] when the user provided a raw token value.
impl<A> TryInto<TokenAmount> for PriceTagTokenAmountPerItem<A>
where
    A: TryInto<TokenAmount>,
{
    type Error = A::Error;

    fn try_into(self) -> Result<TokenAmount, Self::Error> {
        self.0.try_into()
    }
}

/// Converts the wrapped value into a [`MoneyAmount`] using [`TryInto`].
///
/// Used by [`PriceTagBuilder`] to interpret human-readable values like `"1.5"` USDC per item.
impl<A> TryInto<MoneyAmount> for PriceTagMoneyAmountPerItem<A>
where
    A: TryInto<MoneyAmount>,
{
    type Error = A::Error;
    fn try_into(self) -> Result<MoneyAmount, Self::Error> {
        self.0.try_into()
    }
}

/// Trait for initiating a [`PriceTagBuilder`] from a known token deployment.
///
/// All methods clone the asset, so the trait is intended for ergonomic one-liners like:
///
/// ```rust
/// use x402_axum::price::IntoPriceTag;
/// use x402_rs::network::{Network, USDCDeployment};
///
/// let price_tag = USDCDeployment::by_network(Network::Base)
///     .amount("1.50")
///     .pay_to("0x036CbD53842c5426634e7929541eC2318f3dCF7e")
///     .build()
///     .unwrap();
/// ```
pub trait IntoPriceTag {
    fn token_amount<A: TryInto<TokenAmount>>(
        &self,
        token_amount: A,
    ) -> PriceTagBuilder<PriceTagTokenAmountPerItem<A>, ()>;
    fn amount<A: TryInto<MoneyAmount>>(
        &self,
        amount: A,
    ) -> PriceTagBuilder<PriceTagMoneyAmountPerItem<A>, ()>;
    fn pay_to<P: TryInto<EvmAddress>>(&self, address: P) -> PriceTagBuilder<(), P>;
}

/// Errors that may occur when building a [`PriceTag`] using a [`PriceTagBuilder`].
#[derive(Clone, Debug, thiserror::Error)]
pub enum PriceTagBuilderError {
    #[error("No amount per item provided")]
    NoAmountPerItem,
    #[error("Invalid amount per item value")]
    InvalidAmountPerItem,
    #[error("No pay_to address provided")]
    NoPayTo,
    #[error("Invalid pay_to address")]
    InvalidPayTo,
}

impl<A, P> PriceTagBuilder<PriceTagTokenAmountPerItem<A>, P>
where
    A: TryInto<TokenAmount>,
    P: TryInto<EvmAddress>,
{
    /// Builds a [`PriceTag`] using a token-denominated amount per item.
    ///
    /// Returns an error if the amount per item or payee are missing or invalid.
    #[allow(dead_code)] // Public for consumption by downstream crates.
    pub fn build(self) -> Result<PriceTag, PriceTagBuilderError> {
        let token = self.token;
        let amount_per_item = self.amount_per_item.ok_or(PriceTagBuilderError::NoAmountPerItem)?;
        let amount_per_item = amount_per_item
            .try_into()
            .ok()
            .ok_or(PriceTagBuilderError::InvalidAmountPerItem)?;
        let pay_to = self.pay_to.ok_or(PriceTagBuilderError::NoPayTo)?;
        let pay_to = pay_to
            .try_into()
            .ok()
            .ok_or(PriceTagBuilderError::InvalidPayTo)?;
        let price_tag = PriceTag {
            token,
            amount_per_item,
            pay_to,
        };
        Ok(price_tag)
    }

    /// Convenience: like `build` but panics on error. Should only be used when failure is impossible or intended.
    #[allow(dead_code)] // Public for consumption by downstream crates.
    pub fn unwrap(self) -> PriceTag {
        self.build().unwrap()
    }
}

impl<A, P> PriceTagBuilder<PriceTagMoneyAmountPerItem<A>, P>
where
    A: TryInto<MoneyAmount>,
    P: TryInto<EvmAddress>,
{
    /// Builds a [`PriceTag`] from a human-readable money amount per item (e.g., `"1.50"`).
    ///
    /// Converts the money amount to a [`TokenAmount`] using the asset's decimal precision.
    pub fn build(self) -> Result<PriceTag, PriceTagBuilderError> {
        let token = self.token;
        let amount_per_item = self.amount_per_item.ok_or(PriceTagBuilderError::NoAmountPerItem)?;
        let money_amount: MoneyAmount = amount_per_item
            .try_into()
            .ok()
            .ok_or(PriceTagBuilderError::InvalidAmountPerItem)?;
        let amount_per_item = money_amount
            .as_token_amount(token.decimals as u32)
            .ok()
            .ok_or(PriceTagBuilderError::InvalidAmountPerItem)?;
        let pay_to = self.pay_to.ok_or(PriceTagBuilderError::NoPayTo)?;
        let pay_to = pay_to
            .try_into()
            .ok()
            .ok_or(PriceTagBuilderError::InvalidPayTo)?;
        let price_tag = PriceTag {
            token,
            amount_per_item,
            pay_to,
        };
        Ok(price_tag)
    }

    /// Convenience: like `build` but panics on error. Should only be used when failure is impossible.
    pub fn unwrap(self) -> PriceTag {
        self.build().unwrap()
    }
}

impl<A, P> PriceTagBuilder<A, P>
where
    A: Clone,
{
    /// Adds or replaces the `pay_to` address in [`PriceTagBuilder`].
    #[allow(dead_code)] // Public for consumption by downstream crates.
    pub fn pay_to<P1: TryInto<EvmAddress>>(&self, address: P1) -> PriceTagBuilder<A, P1> {
        PriceTagBuilder {
            token: self.token.clone(),
            amount_per_item: self.amount_per_item.clone(),
            pay_to: Some(address),
        }
    }
}

impl<A, P> PriceTagBuilder<A, P>
where
    P: Clone,
{
    /// Sets the human-readable money amount per item in the builder.
    pub fn amount<A1: TryInto<MoneyAmount>>(
        &self,
        amount: A1,
    ) -> PriceTagBuilder<PriceTagMoneyAmountPerItem<A1>, P> {
        PriceTagBuilder {
            token: self.token.clone(),
            amount_per_item: Some(PriceTagMoneyAmountPerItem(amount)),
            pay_to: self.pay_to.clone(),
        }
    }

    /// Sets the token-denominated amount per item in the builder (e.g., `1500000` for 1.5 USDC with 6 decimals).
    #[allow(dead_code)] // Public for consumption by downstream crates.
    pub fn token_amount<A1: TryInto<TokenAmount>>(
        &self,
        token_amount: A1,
    ) -> PriceTagBuilder<PriceTagTokenAmountPerItem<A1>, P> {
        PriceTagBuilder {
            token: self.token.clone(),
            amount_per_item: Some(PriceTagTokenAmountPerItem(token_amount)),
            pay_to: self.pay_to.clone(),
        }
    }
}

impl IntoPriceTag for TokenDeployment {
    fn token_amount<A: TryInto<TokenAmount>>(
        &self,
        token_amount: A,
    ) -> PriceTagBuilder<PriceTagTokenAmountPerItem<A>, ()> {
        let token = self.clone();
        PriceTagBuilder {
            token,
            amount_per_item: Some(PriceTagTokenAmountPerItem(token_amount)),
            pay_to: None,
        }
    }

    fn amount<A: TryInto<MoneyAmount>>(
        &self,
        amount: A,
    ) -> PriceTagBuilder<PriceTagMoneyAmountPerItem<A>, ()> {
        let token = self.clone();
        PriceTagBuilder {
            token,
            amount_per_item: Some(PriceTagMoneyAmountPerItem(amount)),
            pay_to: None,
        }
    }

    fn pay_to<P: TryInto<EvmAddress>>(&self, address: P) -> PriceTagBuilder<(), P> {
        let token = self.clone();
        PriceTagBuilder {
            token,
            amount_per_item: None,
            pay_to: Some(address),
        }
    }
}

impl IntoPriceTag for USDCDeployment {
    /// Sets the exact token-denominated amount per item in the builder.
    fn token_amount<A: TryInto<TokenAmount>>(
        &self,
        token_amount: A,
    ) -> PriceTagBuilder<PriceTagTokenAmountPerItem<A>, ()> {
        self.0.token_amount(token_amount)
    }

    /// Sets the human-readable money amount per item in the builder.
    fn amount<A: TryInto<MoneyAmount>>(
        &self,
        amount: A,
    ) -> PriceTagBuilder<PriceTagMoneyAmountPerItem<A>, ()> {
        self.0.amount(amount)
    }

    /// Adds or replaces the `pay_to` address in the builder.
    fn pay_to<P: TryInto<EvmAddress>>(&self, address: P) -> PriceTagBuilder<(), P> {
        self.0.pay_to(address)
    }
}
