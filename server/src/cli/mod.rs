//! YAML-configured CLI for running the server from a config file.
//!
//! This module powers the `tiders-x402-server` binary. It reads a YAML config,
//! builds the runtime state, and either starts the server (with optional
//! hot reload) or validates the config and exits.
//!
//! The binary entrypoint lives in `src/bin/tiders-x402-server.rs` and calls
//! [`run`] after parsing command-line arguments.

mod builder;
pub(crate) mod config;
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
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
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

    /// Scaffold a new Evidence dashboard project from the server config.
    ///
    /// If `<slug>` matches an entry in `dashboards:`, only that dashboard is
    /// scaffolded. Omit it to scaffold every entry. The project is written to
    /// `{dashboards_root}/{slug}/`.
    Dashboard {
        /// Path to the YAML configuration file.
        /// If omitted, auto-discovers a single .yaml/.yml file in the current directory.
        config: Option<PathBuf>,

        /// Dashboard slug. If omitted, scaffolds
        /// every dashboard configured in `dashboards:`.
        slug: Option<String>,

        /// Overwrite managed files in an existing dashboard directory.
        /// User-owned files (`pages/*.md`, `sources/**/*.sql`) are preserved.
        #[arg(long)]
        force: bool,

        /// Path to a .env file to load before reading the config.
        #[arg(long)]
        env_file: Option<PathBuf>,
    },
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
}

/// Runs the CLI. Parses argv, loads the config, and dispatches to the subcommand.
pub fn run() -> ExitCode {
    // Initialize tracing early so all errors are logged consistently.
    let env_filter =
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    let _ = tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .try_init();

    let cli = Cli::parse();

    enum Action<'a> {
        Start { no_watch: bool },
        Validate,
        Dashboard {
            slug: Option<&'a str>,
            force: bool,
        },
    }

    let (explicit_path, action, env_file): (&Option<PathBuf>, Action<'_>, &Option<PathBuf>) =
        match &cli.command {
            Command::Validate { config, env_file } => (config, Action::Validate, env_file),
            Command::Dashboard {
                slug,
                config,
                force,
                env_file,
            } => (
                config,
                Action::Dashboard {
                    slug: slug.as_deref(),
                    force: *force,
                },
                env_file,
            ),
            Command::Start {
                config,
                no_watch,
                env_file,
            } => (
                config,
                Action::Start {
                    no_watch: *no_watch,
                },
                env_file,
            ),
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

    match action {
        Action::Validate => run_validate(&config),
        Action::Dashboard { slug, force } => run_dashboard(&config_path, &config, slug, force),
        Action::Start { no_watch } => run_server(&config_path, no_watch, &config),
    }
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

fn run_dashboard(config_path: &Path, config: &config::Config, slug: Option<&str>, force: bool) -> ExitCode {
    use crate::dashboard::config::ScaffoldInput;
    use crate::dashboard::scaffold::scaffold_dashboard_folder;
    use crate::dashboard::templates::{render_connection_files, render_landing_page_file, render_sql_files};

    let resolved = builder::resolve_dashboards(config);

    let targets: Vec<&crate::dashboard::Dashboard> = match slug {
        None => {
            if resolved.dashboards.is_empty() {
                tracing::error!(
                    "No dashboards configured. Add a `dashboards:` block to {}.",
                    config_path.display()
                );
                return ExitCode::FAILURE;
            }
            resolved.dashboards.iter().collect()
        }
        Some(s) => match resolved.dashboards.iter().find(|d| d.slug == s) {
            Some(d) => vec![d],
            None => {
                let known: Vec<&str> = resolved.dashboards.iter().map(|d| d.slug.as_str()).collect();
                tracing::error!(
                    "Dashboard \"{s}\" not found in config. Known dashboards: [{}]",
                    known.join(", ")
                );
                return ExitCode::FAILURE;
            }
        },
    };

    // All table names drive SQL file generation; the first also seeds pages/index.md.
    let all_tables: Vec<&str> = config.tables.iter().map(|t| t.name.as_str()).collect();
    let seed_table = all_tables.first().copied().unwrap_or("YOUR_TABLE");
    let source_name = match &config.database {
        db if db.duckdb.is_some()     => "local_duckdb",
        db if db.postgresql.is_some() => "pg",
        db if db.clickhouse.is_some() => "clickhouse",
        _                             => "",
    };

    for d in targets {
        if let Some(parent) = d.folder_path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            tracing::error!(
                "Failed to create parent directory {}: {e}",
                parent.display()
            );
            return ExitCode::FAILURE;
        }

        // Create pre-rendered files with a relative path and content`(project-relative path, content)`
        let mut rendered_files = render_connection_files(&config.database, &d.folder_path);
        rendered_files.extend(render_sql_files(source_name, &all_tables));

        let input = ScaffoldInput {
            project_dir: &d.folder_path,
            name: &d.slug,
            seed_table,
            source_name: &source_name,
            force,
            rendered_files,
        };
        let project_dir = match scaffold_dashboard_folder(&input) {
            Ok(result) => {
                let abs = result
                    .project_dir
                    .canonicalize()
                    .unwrap_or_else(|_| result.project_dir.clone());
                tracing::info!(
                    "Scaffolded dashboard \"{}\" at {} ({} files written, {} preserved)",
                    d.slug,
                    abs.display(),
                    result.written.len(),
                    result.preserved.len()
                );
                abs
            }
            Err(e) => {
                tracing::error!("Failed to scaffold dashboard \"{}\": {e}", d.slug);
                return ExitCode::FAILURE;
            }
        };

        tracing::info!(
            "Install dependencies and build this dashboard page: (cd {} && npm install && npm run build)",
            project_dir.display(),
        );
    }

    // Write a single index.html to the dashboards root listing every
    // configured dashboard as a static snapshot.
    let all_dashboards: Vec<&crate::dashboard::Dashboard> = resolved.dashboards.iter().collect();
    if let Err(e) = std::fs::create_dir_all(&resolved.root) {
        tracing::error!("Failed to create dashboards root {}: {e}", resolved.root.display());
        return ExitCode::FAILURE;
    }
    let (rel, contents) = render_landing_page_file(&all_dashboards);
    let dest = resolved.root.join(rel);
    if let Err(e) = std::fs::write(&dest, contents) {
        tracing::error!("Failed to write {}: {e}", dest.display());
        return ExitCode::FAILURE;
    }

    tracing::info!(
        "After building: tiders-x402-server start {}",
        config_path.display(),
    );
    ExitCode::SUCCESS
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
            let dashboards_handle = state.dashboards.clone();
            let dashboard_router_handle = state.dashboard_router.clone();

            match watcher::start_watcher(
                &config_path,
                config,
                payment_config_handle,
                db_handle,
                dashboards_handle,
                dashboard_router_handle,
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

        crate::start_server(state).await;
        ExitCode::SUCCESS
    })
}
