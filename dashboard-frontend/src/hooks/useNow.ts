import { useEffect, useState } from "react";

/**
 * Returns a monotonically increasing timestamp that re-renders the caller
 * every `intervalMs` milliseconds. Used at the grid level so all chart
 * cards share a single interval for their "Updated Xs ago" labels instead
 * of one `setInterval` per card.
 */
export function useNow(intervalMs: number = 10_000): number {
  const [now, setNow] = useState<number>(() => Date.now());

  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(id);
  }, [intervalMs]);

  return now;
}

/**
 * Formats a rough "Xs ago" / "Xm ago" string for display in chart card
 * footers. `generatedAt` is null when the chart has not produced a
 * timestamp yet (commit 4 fills this in when the data fetch lands).
 */
export function formatRelativeTime(
  generatedAt: Date | null,
  now: number,
): string | null {
  if (generatedAt === null) return null;
  const deltaMs = now - generatedAt.getTime();
  if (deltaMs < 0) return "just now";
  const seconds = Math.floor(deltaMs / 1000);
  if (seconds < 5) return "just now";
  if (seconds < 60) return `Updated ${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `Updated ${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `Updated ${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `Updated ${days}d ago`;
}
