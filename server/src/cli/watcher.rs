//! File watcher for hot-reloading the YAML config.
//!
//! Watches the config file for changes and atomically swaps the payment
//! configuration when hot-reloadable fields change. Logs warnings for
//! fields that require a server restart.

use std::path::Path;
use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::Router;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::RwLock;

use crate::Database;
use crate::dashboard::{Dashboard, build_router as build_dashboard_router};
use crate::payment_config::GlobalPaymentConfig;

use super::builder::resolve_dashboards;
use super::config::Config;
use super::loader::load_config;

/// Shared, swappable payment configuration used by the server.
/// This is the same type as `AppState.payment_config`.
pub type SharedPaymentConfig = Arc<RwLock<Arc<GlobalPaymentConfig>>>;

/// Shared, swappable dashboard list and router. Mirrors fields on `AppState`.
pub type SharedDashboards = Arc<ArcSwap<Vec<Dashboard>>>;
pub type SharedDashboardRouter = Arc<ArcSwap<Router>>;

/// Starts a file watcher on the config file. When the file changes,
/// re-parses it and swaps the payment configuration atomically.
///
/// Fields that require a restart (server bind address, database, facilitator)
/// are detected and logged as warnings — they are not applied until restart.
pub fn start_watcher(
    config_path: &Path,
    original_config: &Config,
    payment_config: SharedPaymentConfig,
    db: Arc<dyn Database>,
    dashboards: SharedDashboards,
    dashboard_router: SharedDashboardRouter,
) -> Result<RecommendedWatcher, notify::Error> {
    let original_bind = original_config.server.bind_address.clone();
    let original_base_url = original_config.server.base_url.clone();
    let original_db_fingerprint = db_fingerprint(&original_config.database);

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

            // Hot-reload dashboard routes.
            let resolved = resolve_dashboards(&new_config);
            let new_router = build_dashboard_router(&resolved);
            dashboard_router.store(Arc::new(new_router));
            dashboards.store(Arc::new(resolved.clone()));
            tracing::info!(
                "Dashboard routes reloaded ({} enabled).",
                resolved.iter().filter(|d| d.enabled).count()
            );

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
        }
    });

    Ok(watcher)
}

/// A rough fingerprint of the database config for change detection.
fn db_fingerprint(db: &super::config::DatabaseConfig) -> String {
    if let Some(d) = &db.duckdb {
        return format!("duckdb:{}", d.path.display());
    }
    if let Some(p) = &db.postgresql {
        return format!("postgresql:{}", p.connection_string);
    }
    if let Some(c) = &db.clickhouse {
        return format!("clickhouse:{}", c.url);
    }
    "none".to_string()
}
