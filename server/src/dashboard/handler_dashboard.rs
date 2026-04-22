//! HTTP handlers + router for the dashboard API.
//!
//! Three endpoints, all unpaid:
//! - `GET /dashboard/api/charts` — JSON catalog descriptor.
//! - `GET /dashboard/api/charts/{id}/data` — Arrow IPC chart data (TTL-cached).
//! - `GET /dashboard/api/charts/{id}/module` — the chart's ECharts build
//!   module served from disk with an mtime-derived ETag.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::Serialize;
use tokio::fs;
use tracing::instrument;

use crate::AppState;
use crate::dashboard::DashboardState;
use crate::dashboard::cache::get_or_fetch;
use crate::database::serialize_batches_to_arrow_ipc;

/// Builds the dashboard sub-router. Meant to be mounted under `/dashboard`
/// via `Router::nest` so the paths here read naturally.
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/charts", get(list_charts))
        .route("/api/charts/{id}/data", get(chart_data))
        .route("/api/charts/{id}/module", get(chart_module))
}

/// One entry in the dashboard catalog response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartDescriptor {
    id: String,
    title: String,
    module_url: String,
    data_url: String,
}

/// `GET /dashboard/api/charts` — catalog listing.
#[axum::debug_handler]
#[instrument(skip_all)]
pub async fn list_charts(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ChartDescriptor>>, DashboardError> {
    let dashboard = load_dashboard(&state).await?;

    let mut descriptors: Vec<ChartDescriptor> = dashboard
        .charts
        .values()
        .map(|c| ChartDescriptor {
            id: c.id.clone(),
            title: c.title.clone(),
            module_url: format!("/dashboard/api/charts/{}/module", c.id),
            data_url: format!("/dashboard/api/charts/{}/data", c.id),
        })
        .collect();

    // HashMap iteration order is non-deterministic; sort so catalog output is
    // stable across requests and process restarts.
    descriptors.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(Json(descriptors))
}

/// `GET /dashboard/api/charts/{id}/data` — Arrow IPC bytes, TTL-cached.
#[axum::debug_handler]
#[instrument(skip_all, fields(chart_id = %id))]
pub async fn chart_data(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, DashboardError> {
    let dashboard = load_dashboard(&state).await?;
    let chart = dashboard
        .charts
        .get(&id)
        .ok_or_else(|| DashboardError::NotFound(id.clone()))?
        .clone();

    let db = state.db.clone();
    let query_timeout = dashboard.query_timeout;
    let ttl = chart.cache_ttl;
    let sql = chart.sql.clone();
    let chart_id_for_fetch = chart.id.clone();

    let entry = get_or_fetch(&dashboard.cache, &chart.id, ttl, || async move {
        run_chart_query(db.as_ref(), &sql, query_timeout, &chart_id_for_fetch).await
    })
    .await?;

    let generated_at_secs = entry
        .generated_at
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let body = Bytes::copy_from_slice(entry.ipc_bytes.as_ref());
    Ok((
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                "application/vnd.apache.arrow.stream".to_string(),
            ),
            (
                header::HeaderName::from_static("x-tiders-generated-at"),
                generated_at_secs.to_string(),
            ),
        ],
        body,
    )
        .into_response())
}

/// `GET /dashboard/api/charts/{id}/module` — serves the chart's JS build
/// module from disk. ETag is derived from mtime so browsers can revalidate
/// cheaply; `Cache-Control: no-cache` forces revalidation on every request
/// but allows 304 short-circuits.
#[axum::debug_handler]
#[instrument(skip_all, fields(chart_id = %id))]
pub async fn chart_module(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, DashboardError> {
    let dashboard = load_dashboard(&state).await?;
    let chart = dashboard
        .charts
        .get(&id)
        .ok_or_else(|| DashboardError::NotFound(id.clone()))?
        .clone();

    let metadata = fs::metadata(&chart.module_path).await.map_err(|e| {
        tracing::error!(
            chart_id = %chart.id,
            path = %chart.module_path.display(),
            error = %e,
            "dashboard module file unreadable"
        );
        DashboardError::Internal(format!(
            "module file for chart \"{}\" is unreadable",
            chart.id
        ))
    })?;

    let etag = module_etag(&metadata, chart.module_path.to_string_lossy().as_ref());

    if let Some(if_none_match) = headers.get(header::IF_NONE_MATCH)
        && if_none_match.as_bytes() == etag.as_bytes()
    {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [
                (header::ETAG, etag.clone()),
                (header::CACHE_CONTROL, "no-cache".to_string()),
            ],
        )
            .into_response());
    }

    let bytes = fs::read(&chart.module_path).await.map_err(|e| {
        tracing::error!(
            chart_id = %chart.id,
            path = %chart.module_path.display(),
            error = %e,
            "dashboard module file read failed"
        );
        DashboardError::Internal(format!(
            "module file for chart \"{}\" could not be read",
            chart.id
        ))
    })?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/javascript".to_string()),
            (header::CACHE_CONTROL, "no-cache".to_string()),
            (header::ETAG, etag),
        ],
        Bytes::from(bytes),
    )
        .into_response())
}

/// Reads the shared dashboard state handle, returning a concrete
/// `Arc<DashboardState>` for the handler to work against. Returns `503`
/// when no dashboard is configured or the dashboard was disabled via
/// hot-reload (commit 5) — the router is always mounted, so absence of
/// state is reported per-request rather than at startup.
async fn load_dashboard(state: &AppState) -> Result<Arc<DashboardState>, DashboardError> {
    state
        .dashboard
        .read()
        .await
        .clone()
        .ok_or(DashboardError::Unavailable)
}

/// Executes the chart's SQL under `timeout`, serializes batches to Arrow IPC.
async fn run_chart_query(
    db: &dyn crate::Database,
    sql: &str,
    timeout: Duration,
    chart_id: &str,
) -> anyhow::Result<Vec<u8>> {
    let batches = match tokio::time::timeout(timeout, db.execute_query(sql)).await {
        Ok(Ok(batches)) => batches,
        Ok(Err(e)) => {
            return Err(anyhow::anyhow!(
                "chart query for \"{chart_id}\" failed: {e}"
            ));
        }
        Err(_) => {
            tracing::warn!(
                chart_id = %chart_id,
                timeout_secs = timeout.as_secs(),
                "dashboard chart query exceeded timeout"
            );
            return Err(anyhow::Error::from(ChartQueryError::Timeout));
        }
    };

    serialize_batches_to_arrow_ipc(&batches)
        .map_err(|e| anyhow::anyhow!("Arrow IPC serialization failed: {e}"))
}

/// Sentinel error used to distinguish SQL timeouts from other failures as the
/// error bubbles out of the cache layer. Matched with `downcast_ref` in the
/// handler's `IntoResponse` path so timeouts map to `504`.
#[derive(Debug, thiserror::Error)]
enum ChartQueryError {
    #[error("chart query timed out")]
    Timeout,
}

/// Builds an ETag from an mtime + a salt so the value changes across files
/// with the same mtime. Quoted per RFC 7232.
fn module_etag(metadata: &std::fs::Metadata, salt: &str) -> String {
    let mtime_nanos = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    // Simple non-cryptographic mix of salt into the mtime so two modules
    // with identical mtimes still get distinct ETags.
    let mut salt_hash: u64 = 14695981039346656037; // FNV-1a offset basis (64-bit)
    for b in salt.bytes() {
        salt_hash ^= u64::from(b);
        salt_hash = salt_hash.wrapping_mul(1099511628211);
    }
    format!("\"{mtime_nanos:x}-{salt_hash:x}\"")
}

/// Error outcomes from the dashboard handlers.
#[derive(Debug)]
pub enum DashboardError {
    NotFound(String),
    Timeout,
    Internal(String),
    Unavailable,
}

impl From<anyhow::Error> for DashboardError {
    fn from(err: anyhow::Error) -> Self {
        if err.downcast_ref::<ChartQueryError>().is_some() {
            Self::Timeout
        } else {
            Self::Internal(err.to_string())
        }
    }
}

impl IntoResponse for DashboardError {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound(id) => {
                tracing::info!(chart_id = %id, "dashboard chart not found");
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "chart not found" })),
                )
                    .into_response()
            }
            Self::Timeout => (
                StatusCode::GATEWAY_TIMEOUT,
                [(header::CONTENT_TYPE, "text/plain")],
                Bytes::from_static(b"chart query timed out"),
            )
                .into_response(),
            Self::Internal(msg) => {
                tracing::error!("dashboard request failed: {msg}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [(header::CONTENT_TYPE, "text/plain")],
                    Bytes::from_static(b"dashboard request failed"),
                )
                    .into_response()
            }
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                [(header::CONTENT_TYPE, "text/plain")],
                Bytes::from_static(b"dashboard not configured"),
            )
                .into_response(),
        }
    }
}
