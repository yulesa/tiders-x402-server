//! File watcher for hot-reloading the YAML config.
//!
//! Watches the config file (and, when a dashboard is configured, its
//! `charts/` directory) for changes and atomically swaps the payment and
//! dashboard state when hot-reloadable fields change. Logs warnings for
//! fields that require a server restart.

use std::path::Path;
use std::sync::Arc;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::RwLock;

use crate::Database;
use crate::dashboard::SharedDashboardState;
use crate::payment_config::GlobalPaymentConfig;

use super::config::Config;
use super::loader::load_config;
use super::validate::resolve_charts_dir;

/// Shared, swappable payment configuration used by the server.
/// This is the same type as `AppState.payment_config`.
pub type SharedPaymentConfig = Arc<RwLock<Arc<GlobalPaymentConfig>>>;

/// Starts a file watcher on the config file (and, when configured, the
/// dashboard `charts/` directory). When anything in those paths changes,
/// re-parses the config and swaps the payment + dashboard state atomically.
///
/// Fields that require a restart (server bind address, database, facilitator,
/// dashboard add/remove) are detected and logged as warnings — they are not
/// applied until restart.
pub fn start_watcher(
    config_path: &Path,
    original_config: &Config,
    payment_config: SharedPaymentConfig,
    dashboard_state: SharedDashboardState,
    db: Arc<dyn Database>,
) -> Result<RecommendedWatcher, notify::Error> {
    let original_bind = original_config.server.bind_address.clone();
    let original_base_url = original_config.server.base_url.clone();
    let original_db_fingerprint = db_fingerprint(&original_config.database);
    let original_dashboard_present = original_config.dashboard.is_some();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res
                && matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                )
            {
                // Non-blocking send; drop if channel is full (debounce)
                let _ = tx.try_send(());
            }
        },
        notify::Config::default(),
    )?;

    // Watch the parent directory to catch atomic renames (editors often
    // write to a temp file then rename).
    let watch_dir = config_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    watcher.watch(watch_dir, RecursiveMode::NonRecursive)?;

    // If a dashboard is configured, also watch its charts directory
    // recursively. The directory is derived from the config path by
    // `resolve_charts_dir` (no YAML field controls it). If the user adds
    // or removes the dashboard section at runtime, they'll see a
    // "restart required" warning on reload (see below).
    if original_dashboard_present {
        match resolve_charts_dir(original_config) {
            Ok(charts_dir) => match watcher.watch(&charts_dir, RecursiveMode::Recursive) {
                Ok(()) => tracing::info!(
                    "Watching dashboard charts directory: {}",
                    charts_dir.display()
                ),
                Err(e) => tracing::warn!(
                    "Failed to watch dashboard charts directory \"{}\": {e}. Module files will not trigger hot reload.",
                    charts_dir.display()
                ),
            },
            Err(e) => tracing::warn!(
                "Could not resolve dashboard charts directory: {e}. Module files will not trigger hot reload."
            ),
        }
    }

    let config_path_clone = config_path.to_path_buf();

    // Spawn the reload loop
    tokio::spawn(async move {
        loop {
            // Wait for a file change notification
            if rx.recv().await.is_none() {
                break; // channel closed
            }

            // Debounce: drain any queued notifications and wait briefly
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            while rx.try_recv().is_ok() {}

            tracing::info!("Config file change detected, reloading...");

            let new_config = match load_config(&config_path_clone) {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("Failed to reload config: {e}. Keeping current configuration.");
                    continue;
                }
            };

            // Warn about cold-restart-only changes
            if new_config.server.bind_address != original_bind {
                tracing::warn!("server.bind_address changed — restart required to take effect.");
            }
            if new_config.server.base_url != original_base_url {
                tracing::warn!("server.base_url changed — restart required to take effect.");
            }
            if db_fingerprint(&new_config.database) != original_db_fingerprint {
                tracing::warn!("database configuration changed — restart required to take effect.");
            }
            // Adding or removing the `dashboard:` section at runtime means
            // the watcher would need to (de)register the `charts/` watch,
            // which it can't do without being rebuilt. Warn and keep the
            // previous dashboard state; catalog edits within an already-
            // enabled dashboard still hot-reload below.
            if new_config.dashboard.is_some() != original_dashboard_present {
                tracing::warn!(
                    "dashboard section added or removed — restart required to (un)watch the charts directory."
                );
            }

            // Rebuild facilitator (hot-reloadable)
            let facilitator = match super::builder::build_facilitator(&new_config.facilitator) {
                Ok(f) => f,
                Err(e) => {
                    tracing::error!(
                        "Failed to rebuild facilitator from reloaded config: {e}. Keeping current configuration."
                    );
                    continue;
                }
            };

            // Rebuild payment config with new facilitator and tables (hot-reloadable)
            match super::builder::build_payment_config_from_tables(
                &new_config,
                db.as_ref(),
                facilitator,
            )
            .await
            {
                Ok(new_payment_config) => {
                    let mut guard = payment_config.write().await;
                    *guard = Arc::new(new_payment_config);
                    tracing::info!("Payment configuration reloaded successfully.");
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to build payment config from reloaded file: {e}. Keeping current configuration."
                    );
                }
            }

            // Rebuild dashboard state (hot-reloadable). Only apply the swap
            // if the dashboard section was already present at startup; a
            // mid-run add/remove was warned about above and is ignored here.
            if original_dashboard_present {
                match super::builder::build_dashboard_state_from_config(&new_config) {
                    Ok(new_state) => {
                        let arc_state = new_state.map(Arc::new);
                        let mut guard = dashboard_state.write().await;
                        *guard = arc_state;
                        tracing::info!("Dashboard charts reloaded");
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to rebuild dashboard state from reloaded config: {e}. Keeping current dashboard state."
                        );
                    }
                }
            }
        }
    });

    Ok(watcher)
}

/// A rough fingerprint of the database config for change detection.
fn db_fingerprint(db: &super::config::DatabaseConfig) -> String {
    if let Some(d) = &db.duckdb {
        return format!("duckdb:{}", d.path);
    }
    if let Some(p) = &db.postgresql {
        return format!("postgresql:{}", p.connection_string);
    }
    if let Some(c) = &db.clickhouse {
        return format!("clickhouse:{}", c.url);
    }
    "none".to_string()
}
