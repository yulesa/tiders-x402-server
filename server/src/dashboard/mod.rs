//! Dashboard subsystem: runtime types for the read-only storefront.
//!
//! The HTTP surface, TTL cache, and embedded assets land in later commits.
//! This module currently exposes only the runtime chart type, used by the
//! CLI's config-to-runtime builder and (eventually) by `DashboardState`.

use std::path::PathBuf;
use std::time::Duration;

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
