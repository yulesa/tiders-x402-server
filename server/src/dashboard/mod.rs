//! Dashboard support: parse `dashboards:` config, register static routes for
//! each enabled dashboard, and (via [`scaffold`]) generate new dashboard
//! project directories from embedded templates.

pub mod config;
pub mod routes;
#[cfg(feature = "cli")]
pub mod scaffold;
#[cfg(feature = "cli")]
pub mod templates;

pub use config::Dashboard;
pub use routes::{DashboardSwap, build_router};
