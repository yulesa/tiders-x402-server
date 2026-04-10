//! Config file loading pipeline: read file -> expand env vars -> parse YAML -> validate.

use std::path::Path;

use anyhow::{Result, bail};

use super::config::Config;
use super::env::expand_env_vars;
use super::validate::validate_config;

/// Loads, expands, parses, and validates a config file.
///
/// Returns a fully validated [`Config`] or a user-friendly error.
pub fn load_config(path: &Path) -> Result<Config> {
    // Read the file
    let raw = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read config file \"{}\": {e}", path.display()))?;

    // Expand environment variables
    let expanded = match expand_env_vars(&raw) {
        Ok(s) => s,
        Err(missing) => {
            let vars = missing
                .iter()
                .map(|v| format!("  - ${{{v}}}"))
                .collect::<Vec<_>>()
                .join("\n");
            bail!(
                "Config references environment variables that are not set:\n{vars}\n\nSet them in your shell or .env file before starting the server."
            );
        }
    };

    // Parse YAML
    let config: Config = serde_yaml::from_str(&expanded).map_err(|e| {
        // serde_yaml errors include line/column info which is helpful
        anyhow::anyhow!("Failed to parse config file \"{}\": {e}", path.display())
    })?;

    // Validate
    let errors = validate_config(&config);
    if !errors.is_empty() {
        let messages = errors
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n\n");
        bail!(
            "Config validation failed with {} error(s):\n\n{messages}",
            errors.len()
        );
    }

    Ok(config)
}
