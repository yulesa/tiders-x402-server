//! Converts a validated [`Config`] into the runtime [`AppState`].
//!
//! This is the bridge between the YAML config world and the library's
//! programmatic API. It constructs database connections, payment configs,
//! facilitator clients, and fetches table schemas.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use url::Url;
use x402_chain_eip155::KnownNetworkEip155;
use x402_chain_eip155::chain::{ChecksummedAddress, Eip155TokenDeployment};
use x402_types::networks::USDC;

use tiders_x402_server::facilitator_client::FacilitatorClient;
use tiders_x402_server::payment_config::GlobalPaymentConfig;
use tiders_x402_server::price::{PriceTag, PricingModel, TablePaymentOffers, TokenAmount};
use tiders_x402_server::{AppState, Database};

use crate::config::{Config, PriceTagConfig};

/// Builds an [`AppState`] from a validated config.
///
/// This connects to the database, fetches schemas for each table (warning and
/// skipping tables that don't exist), and assembles the payment configuration.
pub async fn build_app_state(config: &Config) -> Result<AppState> {
    let db = build_database(&config.database).await?;
    let facilitator = build_facilitator(&config.facilitator)?;

    let mut payment_config = if config.payment.max_timeout_seconds.is_some()
        || config.payment.default_description.is_some()
    {
        GlobalPaymentConfig::new(
            facilitator,
            None,
            config.payment.max_timeout_seconds,
            config.payment.default_description.clone(),
            None,
        )
    } else {
        GlobalPaymentConfig::default(facilitator)
    };

    // Register tables, fetching schemas from the DB
    for table_cfg in &config.tables {
        match build_table_offer(table_cfg, db.as_ref()).await {
            Ok(offer) => {
                tracing::info!("Registered table \"{}\"", table_cfg.name);
                payment_config.add_offers_table(offer);
            }
            Err(e) => {
                tracing::warn!(
                    "Skipping table \"{}\": {}. The server will start without this table.",
                    table_cfg.name,
                    e
                );
            }
        }
    }

    let server_base_url =
        Url::parse(&config.server.base_url).map_err(|e| anyhow!("Invalid server.base_url: {e}"))?;

    Ok(AppState::new(
        db,
        payment_config,
        server_base_url,
        config.server.bind_address.clone(),
    ))
}

/// Builds the payment configuration from config (without tables).
/// Used by hot reload to swap just the payment config.
pub async fn build_payment_config_from_tables(
    config: &Config,
    db: &dyn Database,
    facilitator: Arc<FacilitatorClient>,
) -> Result<GlobalPaymentConfig> {
    let mut payment_config = if config.payment.max_timeout_seconds.is_some()
        || config.payment.default_description.is_some()
    {
        GlobalPaymentConfig::new(
            facilitator,
            None,
            config.payment.max_timeout_seconds,
            config.payment.default_description.clone(),
            None,
        )
    } else {
        GlobalPaymentConfig::default(facilitator)
    };

    for table_cfg in &config.tables {
        match build_table_offer(table_cfg, db).await {
            Ok(offer) => {
                tracing::info!("Registered table \"{}\"", table_cfg.name);
                payment_config.add_offers_table(offer);
            }
            Err(e) => {
                tracing::warn!("Skipping table \"{}\": {}", table_cfg.name, e);
            }
        }
    }

    Ok(payment_config)
}

async fn build_database(db_config: &crate::config::DatabaseConfig) -> Result<Arc<dyn Database>> {
    if let Some(duck) = &db_config.duckdb {
        let db = tiders_x402_server::database_duckdb::DuckDbDatabase::from_path(&duck.path)?;
        return Ok(Arc::new(db));
    }

    if let Some(pg) = &db_config.postgresql {
        let db =
            tiders_x402_server::database_postgresql::PostgresqlDatabase::from_connection_string(
                &pg.connection_string,
            )
            .await?;
        return Ok(Arc::new(db));
    }

    if let Some(ch) = &db_config.clickhouse {
        let db = tiders_x402_server::database_clickhouse::ClickHouseDatabase::from_params(
            &ch.url,
            ch.user.as_deref(),
            ch.password.as_deref(),
            ch.database.as_deref(),
            ch.access_token.as_deref(),
            ch.compression.as_deref(),
            None,
            None,
        )?;
        return Ok(Arc::new(db));
    }

    bail!("No database backend configured.")
}

fn build_facilitator(
    fac_config: &crate::config::FacilitatorConfig,
) -> Result<Arc<FacilitatorClient>> {
    let mut client = FacilitatorClient::try_from(fac_config.url.as_str())
        .map_err(|e| anyhow!("Failed to create facilitator client: {e}"))?;

    if let Some(timeout_secs) = fac_config.timeout {
        client = client.with_timeout(Duration::from_secs(timeout_secs));
    }

    if let Some(headers_map) = &fac_config.headers {
        let mut header_map = http::HeaderMap::new();
        for (k, v) in headers_map {
            let name = http::HeaderName::from_str(k)
                .map_err(|e| anyhow!("Invalid facilitator header name \"{k}\": {e}"))?;
            let value = http::HeaderValue::from_str(v)
                .map_err(|e| anyhow!("Invalid facilitator header value for \"{k}\": {e}"))?;
            header_map.insert(name, value);
        }
        client = client.with_headers(header_map);
    }

    Ok(Arc::new(client))
}

async fn build_table_offer(
    table_cfg: &crate::config::TableConfig,
    db: &dyn Database,
) -> Result<TablePaymentOffers> {
    // Fetch schema from DB (this also validates the table exists)
    let schema = db
        .get_table_schema(&table_cfg.name)
        .await
        .map_err(|e| anyhow!("Failed to get schema for table \"{}\": {e}", table_cfg.name))?;

    let mut price_tags = Vec::new();
    for (i, tag_cfg) in table_cfg.price_tags.iter().enumerate() {
        let tag = build_price_tag(tag_cfg).map_err(|e| {
            anyhow!(
                "Invalid price_tags[{i}] for table \"{}\": {e}",
                table_cfg.name
            )
        })?;
        price_tags.push(tag);
    }

    let mut offer = if price_tags.is_empty() {
        TablePaymentOffers::new_free_table(table_cfg.name.clone(), Some(schema))
    } else {
        TablePaymentOffers::new(table_cfg.name.clone(), price_tags, Some(schema))
    };

    if let Some(desc) = &table_cfg.description {
        offer = offer.with_description(desc.clone());
    }

    Ok(offer)
}

fn build_price_tag(tag_cfg: &PriceTagConfig) -> Result<PriceTag> {
    match tag_cfg {
        PriceTagConfig::PerRow {
            pay_to,
            token,
            amount_per_item,
            min_items,
            max_items,
            min_total_amount,
            description,
            is_default,
        } => {
            let token_deployment = resolve_token(token)?;
            let pay_to_addr = ChecksummedAddress::from_str(pay_to)
                .map_err(|e| anyhow!("Invalid pay_to address \"{pay_to}\": {e}"))?;

            let amount = TokenAmount(
                token_deployment
                    .parse(amount_per_item.as_str())
                    .map_err(|e| anyhow!("Invalid amount_per_item \"{amount_per_item}\": {e}"))?
                    .amount,
            );

            let min_total = if let Some(mta) = min_total_amount {
                Some(TokenAmount(
                    token_deployment
                        .parse(mta.as_str())
                        .map_err(|e| anyhow!("Invalid min_total_amount \"{mta}\": {e}"))?
                        .amount,
                ))
            } else {
                None
            };

            Ok(PriceTag {
                pay_to: pay_to_addr,
                pricing: PricingModel::PerRow {
                    amount_per_item: amount,
                    min_items: *min_items,
                    max_items: *max_items,
                    min_total_amount: min_total,
                },
                token: token_deployment,
                description: description.clone(),
                is_default: *is_default,
            })
        }
        PriceTagConfig::Fixed {
            pay_to,
            token,
            amount,
            description,
            is_default,
        } => {
            let token_deployment = resolve_token(token)?;
            let pay_to_addr = ChecksummedAddress::from_str(pay_to)
                .map_err(|e| anyhow!("Invalid pay_to address \"{pay_to}\": {e}"))?;

            let fixed_amount = TokenAmount(
                token_deployment
                    .parse(amount.as_str())
                    .map_err(|e| anyhow!("Invalid amount \"{amount}\": {e}"))?
                    .amount,
            );

            Ok(PriceTag {
                pay_to: pay_to_addr,
                pricing: PricingModel::Fixed {
                    amount: fixed_amount,
                },
                token: token_deployment,
                description: description.clone(),
                is_default: *is_default,
            })
        }
    }
}

/// Resolves a token identifier like "usdc/base_sepolia" to a token deployment.
fn resolve_token(token_str: &str) -> Result<Eip155TokenDeployment> {
    match token_str {
        "usdc/base_sepolia" => Ok(USDC::base_sepolia()),
        "usdc/base" => Ok(USDC::base()),
        "usdc/avalanche_fuji" => Ok(USDC::avalanche_fuji()),
        "usdc/avalanche" => Ok(USDC::avalanche()),
        "usdc/polygon" => Ok(USDC::polygon()),
        "usdc/polygon_amoy" => Ok(USDC::polygon_amoy()),
        other => bail!(
            "Unknown token \"{other}\". Available: usdc/base_sepolia, usdc/base, usdc/avalanche_fuji, usdc/avalanche, usdc/polygon, usdc/polygon_amoy"
        ),
    }
}
