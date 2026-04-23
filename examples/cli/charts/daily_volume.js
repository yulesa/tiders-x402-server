// Dashboard chart module: "Daily swap volume".
//
// The SPA (future commit) will:
//   1. Fetch /dashboard/api/charts/daily_volume/data as Arrow IPC and
//      decode it into an array of row objects.
//   2. Dynamically import this module and call module.default(rows, meta).
//   3. Pass the returned object straight to ECharts' setOption().
//
// Keep this module a pure function of its inputs — no DOM, no fetches, no
// globals — so it runs identically in the browser and in unit tests.

/**
 * Build an ECharts option object from chart rows.
 *
 * @param {Array<{day: string|Date, swap_count: number}>} rows
 *   One object per row returned by the chart's SQL query.
 * @param {{ generatedAt?: number, title?: string }} [meta]
 *   Optional metadata (e.g. generation timestamp from X-Tiders-Generated-At).
 * @returns {object} ECharts option.
 */
export default function build(rows, meta = {}) {
  const days = rows.map((r) =>
    r.day instanceof Date ? r.day.toISOString().slice(0, 10) : String(r.day).slice(0, 10),
  );
  const counts = rows.map((r) => Number(r.swap_count));

  return {
    title: { text: meta.title ?? "Daily swap volume" },
    tooltip: { trigger: "axis" },
    xAxis: { type: "category", data: days },
    yAxis: { type: "value", name: "Swaps" },
    series: [
      {
        name: "Swaps",
        type: "bar",
        data: counts,
      },
    ],
  };
}