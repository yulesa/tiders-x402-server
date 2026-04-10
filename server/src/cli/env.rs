//! Environment variable substitution for YAML config strings.
//!
//! Replaces `${VAR_NAME}` patterns with the corresponding environment variable
//! value before the YAML is parsed. Produces clear errors when referenced
//! variables are not set.

use regex::Regex;
use std::sync::LazyLock;

static ENV_VAR_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").unwrap_or_else(|_| {
        // This pattern is a compile-time constant so it will never fail.
        unreachable!()
    })
});

/// Expands all `${VAR_NAME}` references in `input` using environment variables.
///
/// Skips YAML comment lines (lines whose first non-whitespace character is `#`).
/// Returns `Ok(expanded_string)` on success, or a list of missing variable names.
pub fn expand_env_vars(input: &str) -> Result<String, Vec<String>> {
    let mut missing = Vec::new();
    let expanded: Vec<String> = input
        .lines()
        .map(|line| {
            if line.trim_start().starts_with('#') {
                return line.to_string();
            }
            ENV_VAR_PATTERN
                .replace_all(line, |caps: &regex::Captures| {
                    let var_name = &caps[1];
                    if let Ok(val) = std::env::var(var_name) {
                        val
                    } else {
                        missing.push(var_name.to_string());
                        format!("${{{var_name}}}")
                    }
                })
                .into_owned()
        })
        .collect();

    if missing.is_empty() {
        Ok(expanded.join("\n"))
    } else {
        Err(missing)
    }
}
