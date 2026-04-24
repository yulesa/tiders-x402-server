//! Dashboard subsystem: runtime types for the read-only storefront.
//!
//! The HTTP surface and embedded assets land in later commits. This module
//! owns the runtime chart catalog, the TTL cache, and the types handlers
//! will eventually consume.

pub mod cache;
pub mod handler_dashboard;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;

use cache::CachedArrow;

/// Default SQL query timeout when the caller does not specify one.
const DEFAULT_QUERY_TIMEOUT_SECS: u64 = 60;

/// Runtime shape of a dashboard chart: all paths resolved, all defaults
/// applied. Constructed from the YAML `ChartConfig` via
/// [`crate::cli::builder::resolve_chart`], or directly by library callers.
#[derive(Debug, Clone)]
pub struct DashboardChart {
    pub id: String,
    pub title: String,
    pub sql: String,
    pub module_path: PathBuf,
    pub cache_ttl: Duration,
}

/// Runtime state for the dashboard subsystem.
///
/// Holds the resolved chart catalog, the TTL cache of serialized Arrow IPC
/// responses, and the SQL query timeout. Attached to `AppState` behind an
/// `Arc<RwLock<Option<Arc<DashboardState>>>>` so the whole state can be
/// swapped atomically on hot reload without disturbing in-flight requests.
#[derive(Debug)]
pub struct DashboardState {
    /// Dashboard display title, shown in the SPA header and included in
    /// the `/api/charts` catalog response.
    pub title: String,
    /// Timeout applied to each chart SQL query.
    pub query_timeout: Duration,
    /// Chart catalog keyed by chart id.
    pub charts: HashMap<String, DashboardChart>,
    /// Chart-id → cached Arrow IPC response. Populated on first request per
    /// chart; refreshed when the entry is older than the chart's TTL.
    pub cache: RwLock<HashMap<String, CachedArrow>>,
}

impl DashboardState {
    /// Creates a `DashboardState` from a resolved chart list. When
    /// `query_timeout` is `None`, [`DEFAULT_QUERY_TIMEOUT_SECS`] is applied.
    pub fn new(
        title: String,
        query_timeout: Option<Duration>,
        charts: Vec<DashboardChart>,
    ) -> Self {
        let charts = charts.into_iter().map(|c| (c.id.clone(), c)).collect();
        Self {
            title,
            query_timeout: query_timeout
                .unwrap_or_else(|| Duration::from_secs(DEFAULT_QUERY_TIMEOUT_SECS)),
            charts,
            cache: RwLock::new(HashMap::new()),
        }
    }
}

/// Type alias mirroring the `payment_config` handle: a swappable optional
/// dashboard state shared across all request handlers.
pub type SharedDashboardState = Arc<RwLock<Option<Arc<DashboardState>>>>;
