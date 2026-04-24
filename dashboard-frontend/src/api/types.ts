// Shared types for the dashboard API.
//
// Mirrors the Rust `CatalogResponse` / `ChartDescriptor` in
// server/src/dashboard/handler_dashboard.rs. Keep field names in sync
// — the server uses `#[serde(rename_all = "camelCase")]` so these map
// 1:1 to the JSON.

export interface ChartDescriptor {
  id: string;
  title: string;
  sql: string;
  moduleUrl: string;
  dataUrl: string;
}

export interface Catalog {
  title: string;
  charts: ChartDescriptor[];
}
