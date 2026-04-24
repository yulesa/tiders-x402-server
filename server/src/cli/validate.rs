//! Configuration validation with user-friendly error messages.
//!
//! Validates a parsed [`Config`] for semantic correctness beyond what serde
//! can enforce: exactly one database backend, valid token identifiers, valid
//! URLs, etc.

use std::path::{Path, PathBuf};

use super::config::{ChartConfig, Config, PriceTagConfig};

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
    validate_dashboard(config, &mut errors);

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

fn validate_database(config: &Config, errors: &mut Vec<ValidationError>) {
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

    // DuckDB path validation
    if let Some(duck) = &db.duckdb {
        if duck.path.is_empty() {
            errors.push(ValidationError {
                message: "database.duckdb.path is empty.".into(),
                hint: Some("Provide a path to a DuckDB file (e.g., \"./data/my.db\").".into()),
            });
        } else if duck.path != ":memory:" && !std::path::Path::new(&duck.path).exists() {
            errors.push(ValidationError {
                message: format!(
                    "database.duckdb.path: file \"{}\" does not exist.",
                    duck.path
                ),
                hint: Some("Create the database first, then start the server.".into()),
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

/// Returns the dashboard charts directory derived from the config file path
/// (`<config_parent>/charts`). Errors if it does not exist or is not a
/// directory.
pub fn resolve_charts_dir(config: &Config) -> std::io::Result<PathBuf> {
    let dir = config
        .config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("charts");

    if !dir.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "charts directory \"{}\" does not exist — create it next to the config file",
                dir.display()
            ),
        ));
    }
    if !dir.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotADirectory,
            format!("\"{}\" exists but is not a directory", dir.display()),
        ));
    }
    Ok(dir)
}

fn validate_dashboard(config: &Config, errors: &mut Vec<ValidationError>) {
    let Some(dashboard) = &config.dashboard else {
        return;
    };

    if dashboard.title.is_empty() {
        errors.push(ValidationError {
            message: "dashboard.title is empty.".into(),
            hint: Some("Provide a human-readable dashboard title.".into()),
        });
    }

    if dashboard.default_cache_ttl_minutes == 0 {
        errors.push(ValidationError {
            message: "dashboard.default_cache_ttl_minutes must be > 0.".into(),
            hint: Some("Use a positive number of minutes, e.g. 60.".into()),
        });
    }

    if let Some(0) = dashboard.query_timeout_seconds {
        errors.push(ValidationError {
            message: "dashboard.query_timeout_seconds must be > 0.".into(),
            hint: Some("Omit the field to use the default (60s), or use a positive value.".into()),
        });
    }

    // Resolve the charts directory so downstream existence checks work.
    let charts_dir = match resolve_charts_dir(config) {
        Ok(dir) => Some(dir),
        Err(e) => {
            errors.push(ValidationError {
                message: format!("dashboard charts directory: {e}"),
                hint: Some(
                    "Create a `charts/` directory next to the config file.".into(),
                ),
            });
            None
        }
    };

    let id_re = regex::Regex::new(r"^[a-z0-9][a-z0-9_-]*$").expect("static regex");
    let mut seen_ids = std::collections::HashSet::new();

    for (i, chart) in dashboard.charts.iter().enumerate() {
        validate_chart(
            i,
            chart,
            &id_re,
            &mut seen_ids,
            charts_dir.as_deref(),
            errors,
        );
    }
}

fn validate_chart(
    idx: usize,
    chart: &ChartConfig,
    id_re: &regex::Regex,
    seen_ids: &mut std::collections::HashSet<String>,
    charts_dir: Option<&Path>,
    errors: &mut Vec<ValidationError>,
) {
    let prefix = format!("dashboard.charts[{idx}]");

    if chart.id.is_empty() {
        errors.push(ValidationError {
            message: format!("{prefix}.id is empty."),
            hint: Some("Use a short lowercase slug like \"daily_volume\".".into()),
        });
    } else if !id_re.is_match(&chart.id) {
        errors.push(ValidationError {
            message: format!(
                "{prefix}.id: \"{}\" must match ^[a-z0-9][a-z0-9_-]*$.",
                chart.id
            ),
            hint: Some("Use lowercase letters, digits, underscores, and hyphens; start with a letter or digit.".into()),
        });
    } else if !seen_ids.insert(chart.id.clone()) {
        errors.push(ValidationError {
            message: format!("{prefix}.id: duplicate chart id \"{}\".", chart.id),
            hint: Some("Every chart id must be unique within the catalog.".into()),
        });
    }

    if chart.title.is_empty() {
        errors.push(ValidationError {
            message: format!("{prefix}.title is empty."),
            hint: Some("Provide a human-readable chart title.".into()),
        });
    }

    if chart.sql.trim().is_empty() {
        errors.push(ValidationError {
            message: format!("{prefix}.sql is empty."),
            hint: Some("Provide a SQL query that produces the chart data.".into()),
        });
    }

    if let Some(0) = chart.cache_ttl_minutes {
        errors.push(ValidationError {
            message: format!("{prefix}.cache_ttl_minutes must be > 0."),
            hint: Some("Omit to inherit default_cache_ttl_minutes, or use a positive value.".into()),
        });
    }

    if chart.module_file.is_empty() {
        errors.push(ValidationError {
            message: format!("{prefix}.module_file is empty."),
            hint: Some("Provide a path to a .js or .mjs ECharts build module.".into()),
        });
        return;
    }

    // If charts_dir couldn't be resolved earlier, skip path-dependent checks.
    let Some(charts_dir) = charts_dir else {
        return;
    };
    let module_path = resolve_module_path(&chart.module_file, charts_dir);

    let ext_ok = module_path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e == "js" || e == "mjs");
    if !ext_ok {
        errors.push(ValidationError {
            message: format!(
                "{prefix}.module_file: \"{}\" must have extension .js or .mjs.",
                chart.module_file
            ),
            hint: None,
        });
    }

    if !module_path.exists() {
        errors.push(ValidationError {
            message: format!(
                "{prefix}.module_file: resolved path \"{}\" does not exist.",
                module_path.display()
            ),
            hint: Some(
                "Create the module file, or set `dashboard.charts_dir` to the directory that contains it."
                    .into(),
            ),
        });
    } else if !module_path.is_file() {
        errors.push(ValidationError {
            message: format!(
                "{prefix}.module_file: resolved path \"{}\" is not a regular file.",
                module_path.display()
            ),
            hint: None,
        });
    }
}

/// Resolves a `module_file` entry to an on-disk path. Absolute paths are
/// returned unchanged; relative paths join with `charts_dir`.
pub fn resolve_module_path(module_file: &str, charts_dir: &Path) -> PathBuf {
    let p = Path::new(module_file);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        charts_dir.join(p)
    }
}
