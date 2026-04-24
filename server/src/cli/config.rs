//! YAML configuration types for the tiders-x402-server CLI.
//!
//! These types are deserialized from the user's YAML config file and then
//! converted into the runtime types (`AppState`, `GlobalPaymentConfig`, etc.)
//! by the [`super::builder`] module.

use std::path::PathBuf;

use serde::Deserialize;

/// Top-level configuration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Server bind address and public URL.
    pub server: ServerConfig,
    /// x402 facilitator service configuration.
    pub facilitator: FacilitatorConfig,
    /// Database backend configuration (exactly one must be specified).
    pub database: DatabaseConfig,
    /// Global payment settings (optional, has defaults).
    #[serde(default)]
    pub payment: PaymentConfig,
    /// Table definitions with pricing.
    #[serde(default)]
    pub tables: Vec<TableConfig>,
    /// Dashboard configuration (optional). Omit to disable the dashboard subsystem.
    #[serde(default)]
    pub dashboard: Option<DashboardConfig>,
    /// Absolute path to the YAML config file. Populated by the loader, not
    /// parsed from YAML. Used to resolve paths relative to the config file
    /// (e.g., the dashboard charts directory).
    #[serde(skip, default)]
    pub config_path: PathBuf,
}

/// Server network configuration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    /// Address to bind the server to (e.g., "0.0.0.0:4021").
    pub bind_address: String,
    /// Public URL of the server, used in x402 payment responses (e.g., "http://localhost:4021").
    pub base_url: String,
}

/// Facilitator service configuration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FacilitatorConfig {
    /// URL of the x402 facilitator (e.g., "https://facilitator.x402.rs").
    pub url: String,
    /// Optional request timeout in seconds.
    pub timeout: Option<u64>,
    /// Optional custom headers sent with every facilitator request.
    pub headers: Option<std::collections::HashMap<String, String>>,
}

/// Database backend configuration. Exactly one variant must be specified.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    /// DuckDB configuration.
    pub duckdb: Option<DuckDbConfig>,
    /// PostgreSQL configuration.
    pub postgresql: Option<PostgresqlConfig>,
    /// ClickHouse configuration.
    pub clickhouse: Option<ClickHouseConfig>,
}

/// DuckDB-specific configuration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DuckDbConfig {
    /// Path to the DuckDB database file.
    pub path: String,
}

/// PostgreSQL-specific configuration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostgresqlConfig {
    /// PostgreSQL connection string (e.g., "host=localhost port=5432 user=postgres ...").
    pub connection_string: String,
}

/// ClickHouse-specific configuration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClickHouseConfig {
    /// ClickHouse HTTP endpoint (e.g., "http://localhost:8123").
    pub url: String,
    /// Database user.
    pub user: Option<String>,
    /// Database password.
    pub password: Option<String>,
    /// Database name.
    pub database: Option<String>,
    /// Access token for authentication.
    pub access_token: Option<String>,
    /// Compression mode: "none" or "lz4".
    pub compression: Option<String>,
}

/// Global payment settings.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentConfig {
    /// How long a payment offer remains valid, in seconds (default: 300).
    pub max_timeout_seconds: Option<u64>,
    /// Fallback description for tables without their own.
    pub default_description: Option<String>,
}

/// Dashboard subsystem configuration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DashboardConfig {
    /// Whether the dashboard is enabled.
    pub enabled: bool,
    /// Human-readable dashboard title.
    pub title: String,
    /// Default cache TTL in minutes for chart query results.
    pub default_cache_ttl_minutes: u64,
    /// Timeout in seconds for chart SQL queries (default: 60).
    pub query_timeout_seconds: Option<u64>,
    /// Chart catalog.
    #[serde(default)]
    pub charts: Vec<ChartConfig>,
}

/// A single chart in the dashboard catalog.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChartConfig {
    /// Chart identifier (must match `^[a-z0-9][a-z0-9_-]*$`, unique within the catalog).
    pub id: String,
    /// Human-readable chart title.
    pub title: String,
    /// SQL query producing the chart data.
    pub sql: String,
    /// Path to the ECharts build module (`.js` or `.mjs`). Relative paths are
    /// resolved against the `charts/` directory next to the config file.
    pub module_file: String,
    /// Per-chart cache TTL override in minutes.
    pub cache_ttl_minutes: Option<u64>,
}

/// A table exposed by the server.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableConfig {
    /// Table name in the database.
    pub name: String,
    /// Human-readable description.
    pub description: Option<String>,
    /// Pricing tiers. Empty or absent means the table is free.
    #[serde(default)]
    pub price_tags: Vec<PriceTagConfig>,
}

/// A single pricing tier for a table.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, tag = "type")]
pub enum PriceTagConfig {
    /// Price scales linearly with the number of rows returned.
    #[serde(rename = "per_row")]
    PerRow {
        /// Recipient wallet address.
        pay_to: String,
        /// Token identifier (e.g., "usdc/base_sepolia").
        token: String,
        /// Price per row as a human-readable decimal string (e.g., "0.002").
        amount_per_item: String,
        /// Minimum row count for this tier to apply.
        min_items: Option<usize>,
        /// Maximum row count for this tier to apply.
        max_items: Option<usize>,
        /// Minimum total charge, even if per-row calculation is lower.
        min_total_amount: Option<String>,
        /// Optional label for this tier.
        description: Option<String>,
        /// Whether this is the default tier (default: false).
        #[serde(default)]
        is_default: bool,
    },
    /// A flat fee regardless of row count.
    #[serde(rename = "fixed")]
    Fixed {
        /// Recipient wallet address.
        pay_to: String,
        /// Token identifier (e.g., "usdc/base_sepolia").
        token: String,
        /// Fixed amount as a human-readable decimal string (e.g., "1.00").
        amount: String,
        /// Optional label for this tier.
        description: Option<String>,
        /// Whether this is the default tier (default: false).
        #[serde(default)]
        is_default: bool,
    },
    /// A flat fee for accessing table metadata (schema + payment offers) via GET /api/table/:name.
    #[serde(rename = "metadata_price")]
    MetadataPrice {
        /// Recipient wallet address.
        pay_to: String,
        /// Token identifier (e.g., "usdc/base_sepolia").
        token: String,
        /// Fixed amount as a human-readable decimal string (e.g., "0.01").
        amount: String,
        /// Optional label for this tier.
        description: Option<String>,
        /// Whether this is the default tier (default: false).
        #[serde(default)]
        is_default: bool,
    },
}
