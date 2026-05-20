//! File watcher for hot-reloading the YAML config.
//!
//! Watches the config file for changes and atomically swaps the payment
//! configuration when hot-reloadable fields change. Logs warnings for
//! fields that require a server restart.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwap;
use axum::Router;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::RwLock;

use crate::Database;
use crate::dashboard::{Dashboard, DashboardsState, build_dashboard_router};
use crate::payment::config::GlobalPaymentConfig;

use super::builder::resolve_dashboards;
use super::config::Config;
use super::loader::load_config;

/// Same type as [`crate::AppState::payment_config`] — passed in so the watcher
/// can swap the inner `Arc<GlobalPaymentConfig>` on config reload.
pub type SharedPaymentConfig = Arc<RwLock<Arc<GlobalPaymentConfig>>>;

/// Same type as [`crate::AppState::dashboards`] — the watcher rebuilds and
/// stores a new [`DashboardsState`] when the `dashboards:` block changes.
pub type SharedDashboards = Arc<ArcSwap<DashboardsState>>;

/// Same type as [`crate::AppState::dashboard_router`] — the watcher swaps in
/// a freshly built sub-router whenever dashboards are added, removed, or
/// re-enabled.
pub type SharedDashboardRouter = Arc<ArcSwap<Router>>;

/// Starts a file watcher and spawns a background task that hot-reloads the
/// config on every change.
///
/// Watches the config file's parent directory (so atomic-rename saves from
/// editors are caught) and debounces bursts of events. On each change it
/// re-parses the YAML; the reload swaps in a new payment config, dashboard
/// state, and dashboard router via `arc-swap`/`RwLock`.
///
/// Cold-restart-only fields (`server.bind_address`, `server.base_url`,
/// `database`) are detected via fingerprint comparison and surfaced as
/// warnings — the running server keeps using the original values until it
/// is restarted.
pub fn start_watcher(
    config_path: &Path,
    original_config: &Config,
    payment_config: SharedPaymentConfig,
    db: Arc<dyn Database>,
    dashboards: SharedDashboards,
    dashboard_router: SharedDashboardRouter,
) -> Result<Arc<Mutex<RecommendedWatcher>>, notify::Error> {
    let original_bind = original_config.server.bind_address.clone();
    let original_base_url = original_config.server.base_url.clone();
    let original_db_fingerprint = db_fingerprint(&original_config.database);

    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);

    let watcher = RecommendedWatcher::new(
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
    let watcher: Arc<Mutex<notify::INotifyWatcher>> = Arc::new(Mutex::new(watcher));

    // Watch the parent directory to catch atomic renames (editors often
    // write to a temp file then rename).
    let watch_dir = config_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    watcher
        .lock()
        .unwrap()
        .watch(watch_dir, RecursiveMode::NonRecursive)?;

    // Subscribe to each dashboard's folder/build paths so a fresh
    // `npm run build` (which materializes `build/index.html`) wakes the
    // reload loop and flips the route from the "unbuilt" placeholder to
    // ServeDir.
    let mut dashboard_paths: Vec<PathBuf> = Vec::new();
    refresh_dashboard_watches(
        &mut watcher.lock().unwrap(),
        &mut dashboard_paths,
        &resolve_dashboards(original_config).dashboards,
    );

    let config_path_clone = config_path.to_path_buf();
    let watcher_for_task = watcher.clone();

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
            let new_router = build_dashboard_router(&resolved.dashboards);
            dashboard_router.store(Arc::new(new_router));
            let enabled_count = resolved.dashboards.iter().filter(|d| d.enabled).count();
            refresh_dashboard_watches(
                &mut watcher_for_task.lock().unwrap(),
                &mut dashboard_paths,
                &resolved.dashboards,
            );
            dashboards.store(Arc::new(resolved));
            tracing::info!("Dashboard routes reloaded ({enabled_count} enabled).");

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

/// Re-subscribes the filesystem watcher to each enabled dashboard's
/// `folder_path` (and `build_path` when it exists). Events on these paths
/// are funneled into the same debounce channel used for config-file
/// changes, so a fresh `npm run build` triggers a router rebuild — that's
/// what flips the route from the "unbuilt" placeholder to `ServeDir`.
///
/// Watches are non-recursive on purpose: `node_modules/` is huge and
/// changes inside an existing `build/` are already served dynamically by
/// `ServeDir`. The only routing-relevant transition is `build/index.html`
/// appearing or disappearing, which surfaces as a direct-child event.
fn refresh_dashboard_watches(
    watcher: &mut RecommendedWatcher,
    currently_watched: &mut Vec<PathBuf>,
    dashboards: &[Dashboard],
) {
    for p in currently_watched.drain(..) {
        let _ = watcher.unwatch(&p);
    }
    for d in dashboards.iter().filter(|d| d.enabled) {
        for path in [&d.folder_path, &d.build_path] {
            if path.is_dir() && watcher.watch(path, RecursiveMode::NonRecursive).is_ok() {
                currently_watched.push(path.clone());
            }
        }
    }
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
