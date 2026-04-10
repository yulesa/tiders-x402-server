//! CLI entry point for tiders-x402-server.
//!
//! Reads a YAML config file, builds the server state, and starts the
//! HTTP server. Optionally watches the config file for hot-reloading
//! payment configuration changes.

mod builder;
mod config;
mod env;
mod loader;
mod validate;
mod watcher;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Run a payment-enabled database API server from a YAML config file.
#[derive(Parser, Debug)]
#[command(name = "tiders-x402-server", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// Start the server.
    Start {
        /// Path to the YAML configuration file.
        /// If omitted, auto-discovers a single .yaml/.yml file in the current directory.
        config: Option<PathBuf>,

        /// Disable automatic config file watching (hot reload is enabled by default).
        #[arg(long)]
        no_watch: bool,

        /// Path to a .env file to load before reading the config.
        /// If omitted, looks for `.env` in the current directory (and parents).
        #[arg(long)]
        env_file: Option<PathBuf>,
    },

    /// Validate the config file and test database connectivity, then exit.
    Validate {
        /// Path to the YAML configuration file.
        /// If omitted, auto-discovers a single .yaml/.yml file in the current directory.
        config: Option<PathBuf>,

        /// Path to a .env file to load before reading the config.
        /// If omitted, looks for `.env` in the current directory (and parents).
        #[arg(long)]
        env_file: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    // Initialize tracing early so all errors are logged consistently.
    let env_filter =
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    let _ = tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .try_init();

    let cli = Cli::parse();

    let (explicit_path, no_watch, validate_only, env_file) = match &cli.command {
        Command::Start {
            config,
            no_watch,
            env_file,
        } => (config, *no_watch, false, env_file),
        Command::Validate { config, env_file } => (config, false, true, env_file),
    };

    // Load .env before anything else so env vars are available for config expansion
    match env_file {
        Some(path) => {
            if let Err(e) = dotenvy::from_path(path) {
                tracing::error!("Failed to load env file {}: {e}", path.display());
                return ExitCode::FAILURE;
            }
        }
        None => {
            dotenvy::dotenv().ok();
        }
    }

    let config_path = match explicit_path {
        Some(p) => p.clone(),
        None => match discover_config() {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("{e}");
                return ExitCode::FAILURE;
            }
        },
    };

    // Load and validate config
    let config = match loader::load_config(&config_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("{e}");
            return ExitCode::FAILURE;
        }
    };

    if validate_only {
        return run_validate(&config);
    }

    run_server(&config_path, no_watch, &config)
}

/// Scans the current directory for `.yaml` / `.yml` files that contain the
/// required top-level keys (`server`, `facilitator`, `database`).
/// Returns the path if exactly one candidate is found, or an error otherwise.
fn discover_config() -> anyhow::Result<PathBuf> {
    let candidates: Vec<PathBuf> = std::fs::read_dir(".")?
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("yaml" | "yml")))
        .filter(|p| {
            // Quick check: does it look like a tiders config?
            let Ok(raw) = std::fs::read_to_string(p) else {
                return false;
            };
            let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&raw) else {
                return false;
            };
            let Some(map) = value.as_mapping() else {
                return false;
            };
            ["server", "facilitator", "database"]
                .iter()
                .all(|key| map.contains_key(serde_yaml::Value::String((*key).to_string())))
        })
        .collect();

    match candidates.len() {
        0 => anyhow::bail!(
            "No config file found. Place a .yaml file with server, facilitator, and database \
             sections in the current directory, or pass a path explicitly:\n\n  \
             tiders-x402-server start path/to/config.yaml"
        ),
        1 => {
            let Some(path) = candidates.into_iter().next() else {
                unreachable!("length checked above");
            };
            tracing::info!("Auto-discovered config: {}", path.display());
            Ok(path)
        }
        n => {
            let names: Vec<_> = candidates
                .iter()
                .map(|p| format!("  - {}", p.display()))
                .collect();
            anyhow::bail!(
                "Found {n} candidate config files:\n{}\n\nSpecify which one to use:\n\n  \
                 tiders-x402-server start path/to/config.yaml",
                names.join("\n")
            )
        }
    }
}

fn run_validate(config: &config::Config) -> ExitCode {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!("Failed to create async runtime: {e}");
            return ExitCode::FAILURE;
        }
    };

    rt.block_on(async {
        match builder::build_app_state(config).await {
            Ok(state) => {
                // Verify database connectivity with a trivial query
                if let Err(e) = state.db.execute_query("SELECT 1").await {
                    tracing::error!("Database connectivity check failed: {e}");
                    return ExitCode::FAILURE;
                }

                let payment_config = state.payment_config.read().await;
                let table_count = payment_config.offers_tables.len();
                tracing::info!(
                    "Config is valid. {table_count} table(s) registered, database connected."
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                tracing::error!("Config validation passed but server setup failed:\n{e}");
                ExitCode::FAILURE
            }
        }
    })
}

fn run_server(config_path: &Path, no_watch: bool, config: &config::Config) -> ExitCode {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!("Failed to create async runtime: {e}");
            return ExitCode::FAILURE;
        }
    };

    rt.block_on(async {
        let state = match builder::build_app_state(config).await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to build server state:\n{e}");
                return ExitCode::FAILURE;
            }
        };

        // Start file watcher for hot reload (if enabled).
        // Clone refs the watcher needs before passing state ownership to start_server.
        let _watcher = if no_watch {
            None
        } else {
            let config_path = match config_path.canonicalize() {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        "Could not resolve config path for watcher: {e}. Hot reload disabled."
                    );
                    config_path.to_path_buf()
                }
            };

            let payment_config_handle = state.payment_config.clone();
            let db_handle = state.db.clone();

            match watcher::start_watcher(
                &config_path,
                config,
                payment_config_handle,
                db_handle,
            ) {
                Ok(w) => {
                    tracing::info!("Config file watcher started. Changes to tables, payment settings, and facilitator will be hot-reloaded.");
                    Some(w)
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to start config file watcher: {e}. Hot reload disabled."
                    );
                    None
                }
            }
        };

        tiders_x402_server::start_server(state).await;
        ExitCode::SUCCESS
    })
}
