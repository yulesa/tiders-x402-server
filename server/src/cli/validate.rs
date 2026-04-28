//! Configuration validation with user-friendly error messages.
//!
//! Validates a parsed [`Config`] for semantic correctness beyond what serde
//! can enforce: exactly one database backend, valid token identifiers, valid
//! URLs, etc.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use super::config::{Config, PriceTagConfig};

/// Names that conflict with reserved server route prefixes.
const RESERVED_DASHBOARD_NAMES: &[&str] = &["api", "assets", "static"];

/// Slug pattern from the plan: lowercase alphanumeric, hyphens and
/// underscores allowed, must start with a letter or digit.
static DASHBOARD_NAME_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-z0-9][a-z0-9_-]*$").unwrap_or_else(|_| unreachable!())
});

/// A validation error with an optional hint for the user.
#[derive(Debug)]
pub struct ValidationError {
    pub message: String,
    pub hint: Option<String>,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error: {}", self.message)?;
        if let Some(hint) = &self.hint {
            write!(f, "\n  Hint: {hint}")?;
        }
        Ok(())
    }
}

/// Known token identifiers and their display names.
const KNOWN_TOKENS: &[&str] = &[
    "usdc/base_sepolia",
    "usdc/base",
    "usdc/avalanche_fuji",
    "usdc/avalanche",
    "usdc/polygon",
    "usdc/polygon_amoy",
];

/// Validates the config and returns all errors found (not just the first).
pub fn validate_config(config: &Config) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    validate_server(config, &mut errors);
    validate_facilitator(config, &mut errors);
    validate_database(config, &mut errors);
    validate_tables(config, &mut errors);
    validate_dashboards(config, &mut errors);

    errors
}

fn validate_server(config: &Config, errors: &mut Vec<ValidationError>) {
    if config.server.bind_address.is_empty() {
        errors.push(ValidationError {
            message: "server.bind_address is empty.".into(),
            hint: Some("Use a host:port format like \"0.0.0.0:4021\".".into()),
        });
    }

    if url::Url::parse(&config.server.base_url).is_err() {
        errors.push(ValidationError {
            message: format!(
                "server.base_url: \"{}\" is not a valid URL.",
                config.server.base_url
            ),
            hint: Some("Use a full URL like \"http://localhost:4021\".".into()),
        });
    }
}

fn validate_facilitator(config: &Config, errors: &mut Vec<ValidationError>) {
    if url::Url::parse(&config.facilitator.url).is_err() {
        errors.push(ValidationError {
            message: format!(
                "facilitator.url: \"{}\" is not a valid URL.",
                config.facilitator.url
            ),
            hint: Some("Use a full URL like \"https://facilitator.x402.rs\".".into()),
        });
    }
}

fn validate_database(
    config: &Config,
    errors: &mut Vec<ValidationError>,
) {
    let db = &config.database;
    let count = usize::from(db.duckdb.is_some())
        + usize::from(db.postgresql.is_some())
        + usize::from(db.clickhouse.is_some());

    if count == 0 {
        errors.push(ValidationError {
            message: "database: no backend specified.".into(),
            hint: Some("Specify exactly one of: duckdb, postgresql, clickhouse.".into()),
        });
    } else if count > 1 {
        let mut active = Vec::new();
        if db.duckdb.is_some() {
            active.push("duckdb");
        }
        if db.postgresql.is_some() {
            active.push("postgresql");
        }
        if db.clickhouse.is_some() {
            active.push("clickhouse");
        }
        errors.push(ValidationError {
            message: format!(
                "database: multiple backends specified ({}). Only one is allowed.",
                active.join(", ")
            ),
            hint: Some("Comment out or remove the backends you don't need.".into()),
        });
    }

    if let Some(duck) = &db.duckdb {
        if duck.path.as_os_str().is_empty() {
            errors.push(ValidationError {
                message: "database.duckdb.path is empty.".into(),
                hint: Some("Provide a path to a DuckDB file (e.g., \"./data/my.db\").".into()),
            });
        } else if !duck.path.exists() {
            errors.push(ValidationError {
                message: format!(
                    "database.duckdb.path: file \"{}\" does not exist.",
                    duck.path.display()
                ),
                hint: Some(
                    "Create the database first, then start the server. Relative paths \
                     are resolved against the config file's directory."
                        .into(),
                ),
            });
        }
    }

    // PostgreSQL validation
    if let Some(pg) = &db.postgresql
        && pg.connection_string.is_empty()
    {
        errors.push(ValidationError {
                message: "database.postgresql.connection_string is empty.".into(),
                hint: Some(
                    "Use a format like \"host=localhost port=5432 user=postgres password=secret dbname=mydb\"."
                        .into(),
                ),
            });
    }

    // ClickHouse validation
    if let Some(ch) = &db.clickhouse {
        if url::Url::parse(&ch.url).is_err() {
            errors.push(ValidationError {
                message: format!(
                    "database.clickhouse.url: \"{}\" is not a valid URL.",
                    ch.url
                ),
                hint: Some("Use a URL like \"http://localhost:8123\".".into()),
            });
        }
        if let Some(comp) = &ch.compression
            && comp != "none"
            && comp != "lz4"
        {
            errors.push(ValidationError {
                message: format!("database.clickhouse.compression: unknown value \"{comp}\".",),
                hint: Some("Use \"none\" or \"lz4\".".into()),
            });
        }
    }
}

fn validate_tables(config: &Config, errors: &mut Vec<ValidationError>) {
    for (i, table) in config.tables.iter().enumerate() {
        if table.name.is_empty() {
            errors.push(ValidationError {
                message: format!("tables[{i}].name is empty."),
                hint: Some(
                    "Every table must have a name matching a table in your database.".into(),
                ),
            });
        }

        for (j, tag) in table.price_tags.iter().enumerate() {
            validate_price_tag(i, j, tag, errors);
        }
    }
}

fn validate_price_tag(
    table_idx: usize,
    tag_idx: usize,
    tag: &PriceTagConfig,
    errors: &mut Vec<ValidationError>,
) {
    let prefix = format!("tables[{table_idx}].price_tags[{tag_idx}]");

    let (pay_to, token) = match tag {
        PriceTagConfig::PerRow {
            pay_to,
            token,
            amount_per_item,
            ..
        } => {
            if amount_per_item.is_empty() {
                errors.push(ValidationError {
                    message: format!("{prefix}: amount_per_item is empty."),
                    hint: Some("Use a decimal string like \"0.002\".".into()),
                });
            }
            (pay_to, token)
        }
        PriceTagConfig::Fixed {
            pay_to,
            token,
            amount,
            ..
        }
        | PriceTagConfig::MetadataPrice {
            pay_to,
            token,
            amount,
            ..
        } => {
            if amount.is_empty() {
                errors.push(ValidationError {
                    message: format!("{prefix}: amount is empty."),
                    hint: Some("Use a decimal string like \"1.00\".".into()),
                });
            }
            (pay_to, token)
        }
    };

    // Validate pay_to is a valid Ethereum address
    if pay_to
        .parse::<x402_chain_eip155::chain::ChecksummedAddress>()
        .is_err()
    {
        errors.push(ValidationError {
            message: format!("{prefix}.pay_to: \"{pay_to}\" is not a valid Ethereum address."),
            hint: Some("Use a checksummed 0x-prefixed address (42 characters).".into()),
        });
    }

    // Validate token identifier
    if !KNOWN_TOKENS.contains(&token.as_str()) {
        let available = KNOWN_TOKENS.join(", ");
        errors.push(ValidationError {
            message: format!("{prefix}.token: unknown token \"{token}\"."),
            hint: Some(format!("Available tokens: {available}")),
        });
    }
}

fn validate_dashboards(config: &Config, errors: &mut Vec<ValidationError>) {
    let mut seen_names: HashSet<&str> = HashSet::new();

    for (i, d) in config.dashboards.entries.iter().enumerate() {
        let prefix = format!("dashboards[{i}]");

        if d.name.is_empty() {
            errors.push(ValidationError {
                message: format!("{prefix}.name is empty."),
                hint: Some("Provide a URL slug like \"uniswap_v3\".".into()),
            });
            continue;
        }

        if !DASHBOARD_NAME_PATTERN.is_match(&d.name) {
            errors.push(ValidationError {
                message: format!(
                    "{prefix}.name: \"{}\" is not a valid slug.",
                    d.name
                ),
                hint: Some(
                    "Use lowercase letters, digits, hyphens, or underscores. \
                     Must start with a letter or digit."
                        .into(),
                ),
            });
            continue;
        }

        if RESERVED_DASHBOARD_NAMES.contains(&d.name.as_str()) {
            errors.push(ValidationError {
                message: format!(
                    "{prefix}.name: \"{}\" is reserved (would shadow a server route).",
                    d.name
                ),
                hint: Some(format!(
                    "Reserved names: {}. Pick a different slug.",
                    RESERVED_DASHBOARD_NAMES.join(", ")
                )),
            });
            continue;
        }

        if !seen_names.insert(d.name.as_str()) {
            errors.push(ValidationError {
                message: format!(
                    "{prefix}.name: \"{}\" is used by more than one dashboard.",
                    d.name
                ),
                hint: Some("Each dashboard must have a unique name.".into()),
            });
        }

    }
}
