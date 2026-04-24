import { useCallback, useEffect, useState } from "react";
import {
  DashboardNotConfiguredError,
  fetchCatalog,
} from "./api/catalog";
import type { Catalog } from "./api/types";
import { ChartGrid } from "./components/ChartGrid";

/**
 * Top-level catalog fetch state. Distinguishing "not configured" (503)
 * from other errors matters because the provider's fix is different:
 * enable `dashboard:` in the YAML vs. debug the server.
 */
type CatalogState =
  | { kind: "loading" }
  | { kind: "notConfigured" }
  | { kind: "error"; message: string }
  | { kind: "ready"; catalog: Catalog };

export default function App() {
  const [state, setState] = useState<CatalogState>({ kind: "loading" });

  const loadCatalog = useCallback(async () => {
    setState({ kind: "loading" });
    try {
      const catalog = await fetchCatalog();
      setState({ kind: "ready", catalog });
    } catch (err) {
      if (err instanceof DashboardNotConfiguredError) {
        setState({ kind: "notConfigured" });
      } else {
        setState({
          kind: "error",
          message: err instanceof Error ? err.message : String(err),
        });
      }
    }
  }, []);

  useEffect(() => {
    void loadCatalog();
  }, [loadCatalog]);

  return (
    <main className="min-h-screen bg-neutral-950 text-neutral-100">
      <div className="mx-auto max-w-6xl px-6 py-10">
        <Header state={state} />
        <div className="mt-8">
          <Body state={state} onRetry={loadCatalog} />
        </div>
      </div>
    </main>
  );
}

function Header({ state }: { state: CatalogState }) {
  const title =
    state.kind === "ready" ? state.catalog.title : "Tiders dashboard";
  return (
    <header className="border-b border-neutral-800 pb-4">
      <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
    </header>
  );
}

function Body({
  state,
  onRetry,
}: {
  state: CatalogState;
  onRetry: () => void;
}) {
  switch (state.kind) {
    case "loading":
      // Three skeleton cards so the grid shape is visible while we wait.
      return (
        <div
          className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3"
          aria-busy="true"
          aria-label="Loading chart catalog"
        >
          {[0, 1, 2].map((n) => (
            <div
              key={n}
              className="h-72 animate-pulse rounded-lg border border-neutral-800 bg-neutral-900/60"
            />
          ))}
        </div>
      );
    case "notConfigured":
      return (
        <EmptyState
          title="Dashboard not configured"
          body={
            <>
              This server has no{" "}
              <code className="rounded bg-neutral-800 px-1 py-0.5 text-xs">
                dashboard:
              </code>{" "}
              section in its YAML config. Add one — or enable it — to see
              charts here.
            </>
          }
        />
      );
    case "error":
      return (
        <div className="mx-auto max-w-md rounded-lg border border-red-800/60 bg-red-950/30 p-6 text-center">
          <p className="text-sm text-red-200">
            Could not load charts: {state.message}
          </p>
          <button
            type="button"
            onClick={onRetry}
            className="mt-4 rounded border border-red-700/60 px-3 py-1 text-xs text-red-100 hover:bg-red-900/40 focus:outline-none focus:ring-1 focus:ring-red-600"
          >
            Retry
          </button>
        </div>
      );
    case "ready":
      if (state.catalog.charts.length === 0) {
        return (
          <EmptyState
            title="No charts configured yet"
            body={
              <>
                The dashboard is enabled, but the YAML config has no{" "}
                <code className="rounded bg-neutral-800 px-1 py-0.5 text-xs">
                  charts:
                </code>{" "}
                entries. Add one to get started.
              </>
            }
          />
        );
      }
      return <ChartGrid charts={state.catalog.charts} />;
  }
}

function EmptyState({
  title,
  body,
}: {
  title: string;
  body: React.ReactNode;
}) {
  return (
    <div className="mx-auto max-w-md rounded-lg border border-neutral-800 bg-neutral-900/40 p-6 text-center">
      <h2 className="text-base font-medium text-neutral-100">{title}</h2>
      <p className="mt-2 text-sm text-neutral-400">{body}</p>
    </div>
  );
}
