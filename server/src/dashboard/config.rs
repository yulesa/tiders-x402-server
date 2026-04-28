//! Runtime configuration and scaffolding I/O types for dashboards.
//!
//! `DashboardConfig` is built from the YAML config (see
//! `cli::config::DashboardConfigYaml`) with `build_path` resolved to an
//! absolute path against the config file's directory.

use std::path::{Path, PathBuf};

/// Runtime dashboards state: the root directory and all configured dashboards.
#[derive(Debug, Clone)]
pub struct DashboardsState {
    /// Absolute path to the directory that contains all dashboard project folders
    /// and where the scaffolded `index.html` landing page is written.
    pub root: PathBuf,
    /// All configured dashboards.
    pub dashboards: Vec<Dashboard>,
}

/// Resolved dashboard configuration used at runtime.
#[derive(Debug, Clone)]
pub struct Dashboard {
    /// URL slug — also the path prefix this dashboard is served under.
    pub name: String,
    /// Whether this dashboard is registered with the router on startup.
    pub enabled: bool,
    /// Absolute path to the dashboard's project directory.
    /// Defaults to `<config_dir>/dashboards/<name>`.
    pub folder_path: PathBuf,
    /// Absolute path to the dashboard's `build/` directory.
    /// Defaults to `<folder_path>/build`.
    pub build_path: PathBuf,
}

/// Outcome of scaffolding a single dashboard.
#[cfg(feature = "cli")]
pub struct ScaffoldResult {
    pub project_dir: PathBuf,
    pub written: Vec<String>,
    pub preserved: Vec<String>,
    /// Managed files that had user edits and were copied to `.old/` before overwriting.
    pub backed_up: Vec<String>,
}

/// Inputs to the scaffolder.
#[cfg(feature = "cli")]
pub struct ScaffoldInput<'a> {
    /// Where to write the dashboard project. Must be the absolute path the
    /// caller wants the project to live at.
    pub project_dir: &'a Path,
    pub name: &'a str,
    pub seed_table: &'a str,
    /// Evidence source name — becomes the schema prefix in page queries
    /// (e.g. `local_duckdb`, `pg`, `clickhouse`).
    pub source_name: &'a str,
    pub force: bool,
    /// Pre-rendered Connection and SQL files to write into the project, as `(project-relative path, content)`.
    pub rendered_files: Vec<(PathBuf, String)>,
}
