//! Runtime configuration and scaffolding I/O types for dashboards.
//!
//! [`Dashboard`] and [`DashboardsState`] are built from the YAML config (see
//! `cli::config::DashboardConfigYaml`) by `cli::builder::resolve_dashboards`,
//! with `folder_path` and `build_path` resolved to absolute paths against
//! the config file's directory.
//!
//! [`ScaffoldInput`] and [`ScaffoldResult`] are I/O types for the `dashboard`
//! subcommand — gated on the `cli` feature since they're only used by the
//! scaffolder.

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
    pub slug: String,
    /// Human-readable title shown on the landing page.
    pub title: String,
    /// Optional one-line description shown under the title.
    pub description: Option<String>,
    /// Tags rendered as pills on the landing page card.
    pub tags: Vec<String>,
    /// Whether this dashboard is included in the active router. Disabled
    /// entries are kept in [`DashboardsState`] but skipped by
    /// `build_dashboard_router`. Toggleable at runtime via config hot-reload.
    pub enabled: bool,
    /// Absolute path to the dashboard's project directory.
    /// Defaults to `<config_dir>/dashboards/<slug>`.
    pub folder_path: PathBuf,
    /// Absolute path to the dashboard's `build/` directory.
    /// Defaults to `<folder_path>/build`.
    pub build_path: PathBuf,
}

/// Outcome of scaffolding a single dashboard.
#[cfg(feature = "cli")]
pub struct ScaffoldResult {
    /// Absolute path to the dashboard project directory that was written.
    pub project_dir: PathBuf,
    /// Project-relative paths of files written or overwritten this run.
    pub written: Vec<String>,
    /// Project-relative paths of user-owned files left untouched (e.g. `pages/index.md`).
    pub preserved: Vec<String>,
    /// Managed files that the user had edited locally; they were copied to
    /// `.old/<filename>` before being overwritten with the new template.
    pub backed_up: Vec<String>,
}

/// Inputs to the scaffolder.
#[cfg(feature = "cli")]
pub struct ScaffoldInput<'a> {
    /// Absolute path where the dashboard project will live. The caller is
    /// responsible for resolving this relative to the config file.
    pub project_dir: &'a Path,
    /// URL slug — also the path prefix the dashboard is served under and
    /// the substituted value of `{{SLUG}}` in templates.
    pub slug: &'a str,
    /// Human-readable title — used by templates that substitute `{{TITLE}}`
    /// (e.g. `package.json` and the starter `pages/index.md`).
    pub title: &'a str,
    /// First table from `tables:` — used as the default in the starter
    /// `pages/index.md` so a freshly scaffolded dashboard works out of the box.
    pub seed_table: &'a str,
    /// Evidence source name — becomes the schema prefix in page queries
    /// (e.g. `local_duckdb`, `pg`, `clickhouse`).
    pub source_name: &'a str,
    /// When true, allow overwriting a non-empty existing project directory.
    /// User-owned files (`pages/*.md`, `sources/**/*.sql`) are still preserved;
    /// modified managed files are backed up to `.old/` before being replaced.
    pub force: bool,
    /// Pre-rendered files supplied by the caller (the connection.yaml plus
    /// one SQL file per table), as `(project-relative path, contents)`.
    /// Written alongside the embedded templates and treated as managed files.
    pub rendered_files: Vec<(PathBuf, String)>,
}
