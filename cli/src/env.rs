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
/// Returns `Ok(expanded_string)` on success, or a list of missing variable names.
pub fn expand_env_vars(input: &str) -> Result<String, Vec<String>> {
    let mut missing = Vec::new();
    let result = ENV_VAR_PATTERN.replace_all(input, |caps: &regex::Captures| {
        let var_name = &caps[1];
        if let Ok(val) = std::env::var(var_name) {
            val
        } else {
            missing.push(var_name.to_string());
            // Leave the original placeholder so the error message is clear
            format!("${{{var_name}}}")
        }
    });

    if missing.is_empty() {
        Ok(result.into_owned())
    } else {
        Err(missing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_set_vars() {
        std::env::set_var("TEST_CLI_HOST", "localhost");
        let result = expand_env_vars("host=${TEST_CLI_HOST}:8080");
        assert_eq!(result, Ok("host=localhost:8080".to_string()));
        std::env::remove_var("TEST_CLI_HOST");
    }

    #[test]
    fn reports_missing_vars() {
        std::env::remove_var("DEFINITELY_NOT_SET_12345");
        let result = expand_env_vars("addr=${DEFINITELY_NOT_SET_12345}");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            vec!["DEFINITELY_NOT_SET_12345".to_string()]
        );
    }

    #[test]
    fn no_substitution_needed() {
        let result = expand_env_vars("plain string without vars");
        assert_eq!(result, Ok("plain string without vars".to_string()));
    }
}
