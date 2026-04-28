//! Axum handler for the `GET /` endpoint.
//!
//! Returns a small HTML landing page that lists all enabled dashboards and
//! links to the API description at `/api/`.

use crate::AppState;
use crate::dashboard::Dashboard;
use axum::extract::State;
use axum::response::Html;
use std::fmt::Write as _;
use std::sync::Arc;

/// Handles `GET /` — renders the dashboard index.
#[axum::debug_handler]
pub async fn landing_handler(State(state): State<Arc<AppState>>) -> Html<String> {
    let dashboards = state.dashboards.load_full();
    Html(render(&dashboards))
}

fn render(dashboards: &Arc<Vec<Dashboard>>) -> String {
    let enabled: Vec<&Dashboard> = dashboards.iter().filter(|d| d.enabled).collect();

    let mut body = String::new();

    if enabled.is_empty() {
        let _ = body.write_str(
            "<p class=\"empty\">No dashboards are configured yet. Add a \
             <code>dashboards:</code> entry to your YAML config and run \
             <code>tiders-x402-server dashboard &lt;name&gt;</code> to scaffold one.</p>",
        );
    } else {
        let _ = body.write_str("<ul class=\"dashboards\">");
        for d in &enabled {
            let name = html_escape(&d.name);
            let _ = write!(
                body,
                "<li><a href=\"/{name}/\">{name}</a></li>",
                name = name
            );
        }
        let _ = body.write_str("</ul>");
    }

    format!(
        "<!DOCTYPE html>\n\
         <html lang=\"en\">\n\
         <head>\n\
           <meta charset=\"utf-8\" />\n\
           <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n\
           <title>Tiders-x402</title>\n\
           <style>\n\
             :root {{ color-scheme: dark; }}\n\
             body {{ font-family: -apple-system, system-ui, sans-serif; background: #0b1220; color: #e2e8f0; max-width: 640px; margin: 4rem auto; padding: 0 1.5rem; line-height: 1.6; }}\n\
             h1 {{ font-size: 1.6rem; margin-bottom: 0.25rem; }}\n\
             p {{ color: #94a3b8; }}\n\
             code {{ background: #1e293b; padding: 0.1rem 0.35rem; border-radius: 4px; font-size: 0.9em; }}\n\
             ul.dashboards {{ list-style: none; padding: 0; margin: 1.5rem 0; }}\n\
             ul.dashboards li {{ margin: 0.5rem 0; }}\n\
             ul.dashboards a {{ display: inline-block; padding: 0.6rem 1rem; background: #1e293b; border-radius: 6px; color: #60a5fa; text-decoration: none; font-weight: 500; }}\n\
             ul.dashboards a:hover {{ background: #334155; color: #93c5fd; }}\n\
             footer {{ margin-top: 3rem; padding-top: 1rem; border-top: 1px solid #1e293b; font-size: 0.9rem; color: #64748b; }}\n\
             footer a {{ color: #60a5fa; }}\n\
             .empty {{ background: #1e293b; padding: 1rem 1.25rem; border-radius: 6px; }}\n\
           </style>\n\
         </head>\n\
         <body>\n\
           <h1>Tiders-x402</h1>\n\
           <p>Pay-per-query data marketplace.</p>\n\
           {body}\n\
           <footer>API description and table catalog: <a href=\"/api/\">/api/</a></footer>\n\
         </body>\n\
         </html>"
    )
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

