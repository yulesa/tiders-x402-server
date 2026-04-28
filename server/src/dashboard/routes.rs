//! Build the Axum sub-router that serves all enabled dashboards.
//!
//! Each enabled dashboard becomes a `/<name>/*` static route backed by a
//! `ServeDir` with SPA fallback (unknown paths under `/<name>/` fall through
//! to the dashboard's `index.html` so client-side routing works).
//!
//! Dashboards whose `build_path` is missing or doesn't contain `index.html`
//! get a 503 fallback page that points at the `dashboard <name>` scaffolder.

use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::Router;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use axum::routing::any;
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};

use super::Dashboard;

/// Builds a router that nests every enabled dashboard at `/<name>`.
///
/// Missing or unbuilt dashboards get a 503 page instead of a static service.
pub fn build_router(dashboards: &[Dashboard]) -> Router {
    let mut router = Router::new();

    for d in dashboards.iter().filter(|d| d.enabled) {
        let index = d.build_path.join("index.html");
        if index.is_file() {
            // SPA fallback: client-side router handles deep links.
            let serve = ServeDir::new(&d.build_path).fallback(ServeFile::new(&index));
            router = router.nest_service(&format!("/{}", d.name), serve);
        } else {
            let name = d.name.clone();
            let build_path = d.build_path.display().to_string();
            tracing::warn!(
                "Dashboard \"{name}\" build directory missing or has no index.html at {build_path}. \
                 Run `tiders-x402-server dashboard {name}` to scaffold and `npm run build` to populate it."
            );
            let route = format!("/{}", d.name);
            router = router.route(
                &route,
                any(move || render_unbuilt(name.clone(), build_path.clone())),
            );
            // Catch deep links too.
            let catchall = format!("/{}/{{*rest}}", d.name);
            let name_c = d.name.clone();
            let path_c = d.build_path.display().to_string();
            router = router.route(
                &catchall,
                any(move || render_unbuilt(name_c.clone(), path_c.clone())),
            );
        }
    }

    router
}

async fn render_unbuilt(name: String, build_path: String) -> impl IntoResponse {
    let body = format!(
        "<!DOCTYPE html>\n\
         <html lang=\"en\"><head><meta charset=\"utf-8\" />\n\
         <title>Dashboard \"{name}\" not built</title>\n\
         <style>\n\
           :root {{ color-scheme: dark; }}\n\
           body {{ font-family: -apple-system, system-ui, sans-serif; background: #0b1220; color: #e2e8f0; max-width: 640px; margin: 4rem auto; padding: 0 1.5rem; line-height: 1.6; }}\n\
           code {{ background: #1e293b; padding: 0.1rem 0.4rem; border-radius: 4px; }}\n\
           h1 {{ font-size: 1.4rem; }}\n\
         </style></head>\n\
         <body>\n\
         <h1>Dashboard <code>{name}</code> is configured but not built yet.</h1>\n\
         <p>Expected a built Evidence project at:</p>\n\
         <p><code>{build_path}</code></p>\n\
         <p>Run:</p>\n\
         <pre><code>tiders-x402-server dashboard {name}\n\
cd {build_path}/.. &amp;&amp; npm install &amp;&amp; npm run build</code></pre>\n\
         </body></html>"
    );
    (StatusCode::SERVICE_UNAVAILABLE, Html(body))
}

/// Tower service that delegates every request to the currently swapped-in
/// dashboard router. The swap is lock-free so route reloads don't block
/// in-flight requests.
#[derive(Clone)]
pub struct DashboardSwap(pub Arc<ArcSwap<Router>>);

impl tower::Service<Request> for DashboardSwap {
    type Response = axum::response::Response;
    type Error = std::convert::Infallible;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let router = self.0.load_full();
        Box::pin(async move {
            let r: Router = (*router).clone();
            r.oneshot(req).await
        })
    }
}
