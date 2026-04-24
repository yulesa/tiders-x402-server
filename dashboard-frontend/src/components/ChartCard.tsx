import type { ChartDescriptor } from "../api/types";
import { formatRelativeTime } from "../hooks/useNow";

/**
 * Per-card render state. Commit 3 stops at the catalog-only state, so the
 * default (`"empty"`) is what visitors see once the catalog has loaded.
 * Commits 4 and 5 add the data fetch and download flow, which is why the
 * richer variants exist up front — it keeps the API stable as new states
 * come online.
 */
export type ChartCardState =
  | { kind: "loading" }
  | { kind: "ready"; generatedAt: Date }
  | { kind: "empty" }
  | { kind: "error"; message: string };

interface ChartCardProps {
  descriptor: ChartDescriptor;
  state: ChartCardState;
  now: number;
  onReload: () => void;
}

/**
 * One tile in the dashboard grid. Owns the card chrome (title, chart
 * area, footer) but not the chart itself — commit 4 plugs the ECharts
 * render into the chart-area `<div>`.
 */
export function ChartCard({
  descriptor,
  state,
  now,
  onReload,
}: ChartCardProps) {
  const relativeTime =
    state.kind === "ready" ? formatRelativeTime(state.generatedAt, now) : null;

  return (
    <article
      aria-labelledby={`chart-${descriptor.id}-title`}
      className="flex flex-col rounded-lg border border-neutral-800 bg-neutral-900/60"
    >
      <header className="flex items-start justify-between border-b border-neutral-800 px-4 py-3">
        <h2
          id={`chart-${descriptor.id}-title`}
          className="text-sm font-medium text-neutral-100"
        >
          {descriptor.title}
        </h2>
      </header>

      <div className="flex min-h-[18rem] items-center justify-center p-4">
        {renderBody(state, onReload)}
      </div>

      <footer className="flex items-center justify-between border-t border-neutral-800 px-4 py-2 text-xs text-neutral-500">
        <span aria-live="polite">{relativeTime ?? "—"}</span>
        <button
          type="button"
          onClick={onReload}
          className="rounded px-2 py-1 text-neutral-400 hover:bg-neutral-800 hover:text-neutral-100 focus:outline-none focus:ring-1 focus:ring-neutral-600"
          aria-label={`Reload ${descriptor.title}`}
        >
          Reload
        </button>
      </footer>
    </article>
  );
}

function renderBody(state: ChartCardState, onReload: () => void) {
  switch (state.kind) {
    case "loading":
      return (
        <div
          className="h-full w-full animate-pulse rounded bg-neutral-800/60"
          aria-hidden="true"
        />
      );
    case "ready":
      // The ECharts render lands in commit 4. Until then "ready" is
      // unreachable (no data fetch happens), but the branch is wired up
      // so the type stays honest.
      return (
        <p className="text-sm text-neutral-500">
          Chart rendering ships in commit 4.
        </p>
      );
    case "empty":
      return (
        <p className="text-sm text-neutral-500">
          Chart data loads in commit 4.
        </p>
      );
    case "error":
      return (
        <div className="flex flex-col items-center gap-3 text-center">
          <p className="text-sm text-red-300">{state.message}</p>
          <button
            type="button"
            onClick={onReload}
            className="rounded border border-red-700/60 px-3 py-1 text-xs text-red-200 hover:bg-red-900/40 focus:outline-none focus:ring-1 focus:ring-red-600"
          >
            Retry
          </button>
        </div>
      );
  }
}
