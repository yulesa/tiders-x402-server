//! Dashboard support.
//!
//! - [`config`] — runtime types ([`DashboardsState`], [`Dashboard`]) and the
//!   scaffolder I/O types.
//! - [`routes`] — Axum [`landing_handler`], [`build_dashboard_router`], and
//!   the [`DashboardSwap`] tower service used as the router fallback.
//! - `scaffold` (cli only) — writes a new Evidence project to disk, with
//!   sha256-based drift detection backing up user-edited managed files.
//! - `templates` (cli only) — embedded Svelte/TS/config templates plus
//!   placeholder substitution.

pub mod config;
pub mod routes;
#[cfg(feature = "cli")]
pub mod scaffold;
#[cfg(feature = "cli")]
pub mod templates;

pub use config::{Dashboard, DashboardsState};
pub use routes::{DashboardSwap, build_dashboard_router, landing_handler};
