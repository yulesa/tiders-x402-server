import type { Catalog } from "./types";

/**
 * Thrown when the server responds 503 to `GET /api/charts`, which happens
 * when the YAML config has no `dashboard:` section (or the section is
 * disabled). Surfaced to the UI as a dedicated "not configured" state.
 */
export class DashboardNotConfiguredError extends Error {
  constructor() {
    super("Dashboard not configured");
    this.name = "DashboardNotConfiguredError";
  }
}

/**
 * Fetches the dashboard catalog from the server.
 *
 * Resolves with the `{ title, charts }` payload on success. Rejects with
 * `DashboardNotConfiguredError` on 503, or a plain `Error` with the HTTP
 * status for everything else.
 */
export async function fetchCatalog(): Promise<Catalog> {
  const response = await fetch("/api/charts", {
    headers: { Accept: "application/json" },
  });

  if (response.status === 503) {
    throw new DashboardNotConfiguredError();
  }

  if (!response.ok) {
    throw new Error(`GET /api/charts failed: HTTP ${response.status}`);
  }

  return (await response.json()) as Catalog;
}
