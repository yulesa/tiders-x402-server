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

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

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
        /// Defaults to `tiders-x402-server.yaml` in the current directory.
        #[arg(default_value = "tiders-x402-server.yaml")]
        config: PathBuf,

        /// Disable automatic config file watching (hot reload is enabled by default).
        #[arg(long)]
        no_watch: bool,
    },

    /// Validate the config file and test database connectivity, then exit.
    Validate {
        /// Path to the YAML configuration file.
        /// Defaults to `tiders-x402-server.yaml` in the current directory.
        #[arg(default_value = "tiders-x402-server.yaml")]
        config: PathBuf,
    },
}

/// Writes a message to stderr without using the `eprintln!` macro
/// (which is denied by workspace lints intended for library code).
fn log_stderr(msg: &str) {
    let _ = writeln!(std::io::stderr(), "{msg}");
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // Load .env before anything else so env vars are available for config expansion
    dotenvy::dotenv().ok();

    let (config_path, no_watch, validate_only) = match &cli.command {
        Command::Start { config, no_watch } => (config, *no_watch, false),
        Command::Validate { config } => (config, false, true),
    };

    // Load and validate config
    let config = match loader::load_config(config_path) {
        Ok(c) => c,
        Err(e) => {
            log_stderr(&e.to_string());
            return ExitCode::FAILURE;
        }
    };

    if validate_only {
        return run_validate(&config);
    }

    run_server(config_path, no_watch, &config)
}

fn run_validate(config: &config::Config) -> ExitCode {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            log_stderr(&format!("Failed to create async runtime: {e}"));
            return ExitCode::FAILURE;
        }
    };

    rt.block_on(async {
        match builder::build_app_state(config).await {
            Ok(state) => {
                let payment_config = state.payment_config.read().await;
                let table_count = payment_config.offers_tables.len();
                log_stderr(&format!(
                    "Config is valid. {table_count} table(s) registered, database connected."
                ));
                ExitCode::SUCCESS
            }
            Err(e) => {
                log_stderr(&format!(
                    "Config validation passed but server setup failed:\n{e}"
                ));
                ExitCode::FAILURE
            }
        }
    })
}

fn run_server(config_path: &PathBuf, no_watch: bool, config: &config::Config) -> ExitCode {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            log_stderr(&format!("Failed to create async runtime: {e}"));
            return ExitCode::FAILURE;
        }
    };

    rt.block_on(async {
        let state = match builder::build_app_state(config).await {
            Ok(s) => s,
            Err(e) => {
                log_stderr(&format!("Failed to build server state:\n{e}"));
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
                    config_path.clone()
                }
            };

            let payment_config_handle = state.payment_config.clone();
            let db_handle = state.db.clone();
            let facilitator = state.payment_config.read().await.facilitator.clone();

            match watcher::start_watcher(
                config_path,
                config,
                payment_config_handle,
                db_handle,
                facilitator,
            ) {
                Ok(w) => {
                    tracing::info!("Config file watcher started. Changes to tables and payment settings will be hot-reloaded.");
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
