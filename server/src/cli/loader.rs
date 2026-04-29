//! Config file loading pipeline: read file -> expand env vars -> parse YAML -> validate.

use std::path::{Path, PathBuf};
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
    let mut config: Config = serde_yaml::from_str(&expanded).map_err(|e| {
        // serde_yaml errors include line/column info which is helpful
        anyhow::anyhow!("Failed to parse config file \"{}\": {e}", path.display())
    })?;

    // Resolve relative paths to absolute, anchored at the config file's directory.
    if let Some(duck) = &mut config.database.duckdb {
        duck.path = resolve_against_config(path, &duck.path.to_string_lossy());
    }
    let default_root = resolve_against_config(path, "./dashboards");
    let root = config.dashboards.root.take().map(|r| resolve_against_config(path, &r.to_string_lossy())).unwrap_or(default_root);
    config.dashboards.root = Some(root.clone());
    for d in &mut config.dashboards.entries {
        let folder = match d.folder_path.take() {
            Some(p) => resolve_against_config(path, &p.to_string_lossy()),
            None => root.join(&d.slug),
        };
        let build = match d.build_path.take() {
            Some(p) => resolve_against_config(path, &p.to_string_lossy()),
            None => folder.join("build"),
        };
        d.folder_path = Some(folder);
        d.build_path = Some(build);
    }

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


/// Resolves `target` to an absolute path against the config file's directory.
/// Always returns an absolute path: canonicalized when the target exists,
/// otherwise lexically-normalized against the absolute config dir.
fn resolve_against_config(config_path: &Path, target: &str) -> PathBuf {
    let p = Path::new(target);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    let base = config_path.parent().unwrap_or_else(|| Path::new("."));
    let joined = base.join(p);
    if let Ok(c) = joined.canonicalize() {
        return c;
    }
    // The target doesn't exist yet. Make sure the result is still absolute by
    // anchoring against current_dir, then lexically resolve `..` components.
    let abs = if joined.is_absolute() {
        joined
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(joined)
    };
    let mut out = PathBuf::new();
    for c in abs.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => { out.pop(); }
            c => out.push(c),
        }
    }
    out
}
