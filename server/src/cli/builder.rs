//! Converts a validated [`Config`] into the runtime [`AppState`].
//!
//! This is the bridge between the YAML config world and the library's
//! programmatic API. It constructs database connections, payment configs,
//! facilitator clients, and fetches table schemas.

use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use url::Url;
use x402_chain_eip155::KnownNetworkEip155;
use x402_chain_eip155::chain::{ChecksummedAddress, Eip155TokenDeployment};
use x402_types::networks::USDC;

use crate::dashboard::{DashboardChart, DashboardState};
use crate::facilitator_client::FacilitatorClient;
use crate::payment_config::GlobalPaymentConfig;
use crate::price::{PriceTag, PricingModel, TablePaymentOffers, TokenAmount};
use crate::{AppState, Database};

use super::config::{ChartConfig, Config, DashboardConfig, PriceTagConfig};
use super::validate::{resolve_charts_dir, resolve_module_path};

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

    let dashboard = build_dashboard_state_from_config(config)?;

    Ok(AppState::new(
        db,
        payment_config,
        server_base_url,
        config.server.bind_address.clone(),
        dashboard,
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

async fn build_database(db_config: &super::config::DatabaseConfig) -> Result<Arc<dyn Database>> {
    #[cfg(feature = "duckdb")]
    if let Some(duck) = &db_config.duckdb {
        let db = crate::database_duckdb::DuckDbDatabase::from_path(&duck.path)?;
        return Ok(Arc::new(db));
    }
    #[cfg(not(feature = "duckdb"))]
    if db_config.duckdb.is_some() {
        bail!(
            "Config specifies a DuckDB backend but this build was compiled without the \"duckdb\" feature."
        );
    }

    #[cfg(feature = "postgresql")]
    if let Some(pg) = &db_config.postgresql {
        let db = crate::database_postgresql::PostgresqlDatabase::from_connection_string(
            &pg.connection_string,
        )
        .await?;
        return Ok(Arc::new(db));
    }
    #[cfg(not(feature = "postgresql"))]
    if db_config.postgresql.is_some() {
        bail!(
            "Config specifies a PostgreSQL backend but this build was compiled without the \"postgresql\" feature."
        );
    }

    #[cfg(feature = "clickhouse")]
    if let Some(ch) = &db_config.clickhouse {
        let db = crate::database_clickhouse::ClickHouseDatabase::from_params(
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
    #[cfg(not(feature = "clickhouse"))]
    if db_config.clickhouse.is_some() {
        bail!(
            "Config specifies a ClickHouse backend but this build was compiled without the \"clickhouse\" feature."
        );
    }

    bail!("No database backend configured.")
}

pub fn build_facilitator(
    fac_config: &super::config::FacilitatorConfig,
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
    table_cfg: &super::config::TableConfig,
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
        PriceTagConfig::MetadataPrice {
            pay_to,
            token,
            amount,
            description,
            is_default,
        } => {
            let token_deployment = resolve_token(token)?;
            let pay_to_addr = ChecksummedAddress::from_str(pay_to)
                .map_err(|e| anyhow!("Invalid pay_to address \"{pay_to}\": {e}"))?;

            let metadata_amount = TokenAmount(
                token_deployment
                    .parse(amount.as_str())
                    .map_err(|e| anyhow!("Invalid amount \"{amount}\": {e}"))?
                    .amount,
            );

            Ok(PriceTag {
                pay_to: pay_to_addr,
                pricing: PricingModel::MetadataPrice {
                    amount: metadata_amount,
                },
                token: token_deployment,
                description: description.clone(),
                is_default: *is_default,
            })
        }
    }
}

/// Converts a YAML [`ChartConfig`] into its runtime [`DashboardChart`] shape.
/// Pure: path + default resolution only, no filesystem or network I/O.
pub fn resolve_chart(
    chart: &ChartConfig,
    dashboard: &DashboardConfig,
    charts_dir: &Path,
) -> DashboardChart {
    let ttl_minutes = chart
        .cache_ttl_minutes
        .unwrap_or(dashboard.default_cache_ttl_minutes);

    DashboardChart {
        id: chart.id.clone(),
        title: chart.title.clone(),
        sql: chart.sql.clone(),
        module_path: resolve_module_path(&chart.module_file, charts_dir),
        cache_ttl: Duration::from_secs(ttl_minutes.saturating_mul(60)),
    }
}

/// Builds a [`DashboardState`] from a validated [`Config`], or `None` when the
/// config has no `dashboard:` section or the dashboard is disabled. Also
/// ensures `charts_dir` exists on disk.
///
/// Symmetric to [`build_payment_config_from_tables`], usable both during
/// initial startup and by the hot-reload path (commit 5).
pub fn build_dashboard_state_from_config(config: &Config) -> Result<Option<DashboardState>> {
    let Some(dashboard) = &config.dashboard else {
        return Ok(None);
    };
    if !dashboard.enabled {
        tracing::info!("Dashboard present in config but disabled; not mounting.");
        return Ok(None);
    }

    let charts_dir = resolve_charts_dir(config)
        .map_err(|e| anyhow!("Failed to resolve dashboard charts directory: {e}"))?;

    let charts: Vec<DashboardChart> = dashboard
        .charts
        .iter()
        .map(|c| resolve_chart(c, dashboard, &charts_dir))
        .collect();

    let query_timeout = dashboard.query_timeout_seconds.map(Duration::from_secs);
    let state = DashboardState::new(dashboard.title.clone(), query_timeout, charts);

    tracing::info!(
        "Dashboard enabled with {} chart(s); query timeout {}s.",
        state.charts.len(),
        state.query_timeout.as_secs()
    );

    Ok(Some(state))
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
