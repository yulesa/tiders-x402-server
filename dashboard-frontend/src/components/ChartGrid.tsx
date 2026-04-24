import { useCallback, useState } from "react";
import type { ChartDescriptor } from "../api/types";
import { useNow } from "../hooks/useNow";
import { ChartCard, type ChartCardState } from "./ChartCard";

interface ChartGridProps {
  charts: ChartDescriptor[];
}

/**
 * Responsive 1 / 2 / 3-column grid of chart cards. Runs a single
 * `useNow()` timer and hands the value down to every card so all
 * "Updated Xs ago" labels tick in unison.
 */
export function ChartGrid({ charts }: ChartGridProps) {
  const now = useNow(10_000);

  // Per-chart card state lives here. Commit 3 leaves every card at the
  // default "empty" state because the data fetch does not land until
  // commit 4. The reload button flips a card into "loading" and back —
  // enough to demo the UX plumbing without a real fetch.
  const [cardStates, setCardStates] = useState<Record<string, ChartCardState>>(
    {},
  );

  const handleReload = useCallback((id: string) => {
    setCardStates((prev) => ({ ...prev, [id]: { kind: "loading" } }));
    // Stub: pretend the fetch finished after a short delay. Real fetch
    // lands in commit 4; this is only here so clicking Reload has a
    // visible effect (loading skeleton → empty placeholder).
    window.setTimeout(() => {
      setCardStates((prev) => ({ ...prev, [id]: { kind: "empty" } }));
    }, 600);
  }, []);

  return (
    <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
      {charts.map((descriptor) => {
        const state: ChartCardState = cardStates[descriptor.id] ?? {
          kind: "empty",
        };
        return (
          <ChartCard
            key={descriptor.id}
            descriptor={descriptor}
            state={state}
            now={now}
            onReload={() => handleReload(descriptor.id)}
          />
        );
      })}
    </div>
  );
}
