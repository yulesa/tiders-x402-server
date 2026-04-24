# Tiders x402 Client & Dashboard — Architecture Design Document

**Date:** 2026-04-15
**Status:** Draft

---

## 1. Overview

Tiders is a decentralized data marketplace. Anyone can sell data by running a **tiders-x402-server**. The **tiders-dashboard** is a free, read-only storefront that lives **inside** the tiders-x402-server.

- **tiders-dashboard** — A new endpoint family and a small React single-page application, both shipped inside the tiders-x402-server. The server serves the SPA as static assets and the chart data as JSON from its own database.

### 1.1 How the pieces fit together

```
┌────────────────────────────────────────────────────────────────────────┐
│                            DATA PROVIDER                               │
│                                                                        │
│   tiders-x402-server (Rust, single process)                            │
│   ┌──────────────────────────────────────────────────────────────┐     │
│   │                                                              │     │
│   │   PAID API (x402-metered)                                    │     │
│   │   ─────────────────────────                                  │     │
│   │   POST /query              → restricted SQL, Arrow IPC       │     │
│   │   GET  /table/:name        → pricing + schema                │     │
│   │   GET  /                   → server info                     │     │
│   │                                                              │     │
│   │   DASHBOARD (free)                                           │     │
│   │   ─────────────────                                          │     │
│   │   GET  /dashboard/              → SPA (index.html)           │     │
│   │   GET  /dashboard/assets/*      → SPA bundle (JS/CSS)        │     │
│   │   GET  /dashboard/api/charts    → chart catalog (JSON)       │     │
│   │   GET  /dashboard/api/charts/:id/data   → rows (JSON)        │     │
│   │   GET  /dashboard/api/charts/:id/module → ES module (JS)     │     │
│   │                                                              │     │
│   │   ┌────────────────────────────────────────────────────┐     │     │
│   │   │  Database (DuckDB / Postgres / ClickHouse)         │     │     │
│   │   │  Shared between paid API and dashboard             │     │     │
│   │   └────────────────────────────────────────────────────┘     │     │
│   └──────────────────────────────────────────────────────────────┘     │
│                         ▲                      ▲                       │
└─────────────────────────┼──────────────────────┼───────────────────────┘
                          │                      │
                          │ x402 paid            │ free
                          │ (Arrow IPC)          │ (JSON)
                          │                      │
                  ┌───────┴────────┐      ┌──────┴──────────┐
                  │  Developer     │      │  Visitor        │
                  │  using         │      │  browsing in    │
                  │  tiders-x402-  │      │  a web browser  │
                  │  ts-client     │      │                 │
                  └────────────────┘      └─────────────────┘
```

### 1.2 Typical users

| User | What they deploy | What they use |
|------|-----------------|---------------|
| **Data provider** | tiders-x402-server (which bundles the dashboard) | Sells data via paid API; dashboard is the storefront |
| **Data consumer (analyst / visitor)** | Nothing — opens the dashboard URL | Browses pre-set interactive ECharts for free. Can download paid data through a download button and handle as csv. |

---

## 2. tiders-x402-server API (This repo)

The server exposes two surface areas: the existing paid API and the new dashboard endpoints.

### 2.1 Paid API (existing, unchanged)

| Method | Path | Description | Response |
|--------|------|-------------|----------|
| `GET` | `/` | Server info, available tables, SQL rules | Plain text |
| `POST` | `/query` | Execute a restricted SELECT | Arrow IPC binary (200) or payment requirements (402) |
| `GET` | `/table/:name` | Table schema and pricing details | JSON (200) or payment requirements (402) |

### 2.2 Dashboard endpoints (new, free)

| Method | Path | Description | Response |
|--------|------|-------------|----------|
| `GET` | `/dashboard/` | Dashboard SPA entry point | `index.html` |
| `GET` | `/dashboard/assets/*` | SPA bundle (JS, CSS, fonts) | Static files |
| `GET` | `/dashboard/api/charts` | Chart catalog: list of `{ id, title, moduleUrl, dataUrl }` | JSON |
| `GET` | `/dashboard/api/charts/:id/data` | Current result rows of a chart's SQL | JSON `{ rows: [...], generatedAt }` |
| `GET` | `/dashboard/api/charts/:id/module` | The chart's preconfigured JS module (ES module) that builds the ECharts option from rows | `application/javascript` |

No authentication. No x402 on this path. Internal TTL cache (see §4.3) avoids hammering the DB. Chart modules are served directly from disk with `ETag` / `Cache-Control` so visitors refetch only when the provider edits a module file.

---

## 3. tiders-x402-ts-client (/home/yulesa/repos/tiders-x402-ts-client)

### 3.1 Purpose

A TypeScript library that abstracts the complexity of:
1. Discovering what data a tiders-x402-server offers
2. Executing SQL queries against the paid API
3. Handling the x402 payment flow (402 response → sign → retry)
4. Parsing Arrow IPC responses
5. Managing payment budgets and allowances

The client is standalone. The dashboard does **not** use it — the dashboard is internal to the server and does not make paid requests.

### 3.2 Environments

- **Node.js / Bun / Deno** — scripts, backend services, CLIs. Ships with a private-key signer.
- **Browser** — any web app that wants to consume x402 data. Caller provides a wallet-backed signer (e.g. built on viem's `WalletClient`).

### 3.3 Key interfaces

See the existing `tiders-x402-ts-client/src/client.ts` reference for the `TidersClient`, `PaymentSigner`, and budget/allowance interfaces.

---

## 4. tiders-dashboard

### 4.1 Purpose

A free, read-only storefront built into the tiders-x402-server. It advertises the provider's data via pre-defined, interactive ECharts. Its role is to make developers think "this data looks useful; I'll download the paid data or wire up tiders-x402-ts-client and pay for unrestricted access"

The interactive ECharts are deliberately **not** a paid surface. Visitors only see a wallet popup when clicking a download button, x402 payments remain exclusively on `POST /query`.

### 4.2 Tech stack

| Layer | Technology | Why |
|-------|-----------|-----|
| **Runtime** | Rust (same server process as the paid API) | No new deployable service; reuses existing DB connection and config machinery |
| **Dashboard HTTP** | Whatever framework the Rust server uses axum | Consistent with the existing codebase |
| **Query execution** | Whatever DB the server is configured against (DuckDB / PG / CH) | No new analytical engine |
| **Frontend build** | Vite | Fast dev server, small static bundle |
| **Frontend framework** | React + TypeScript | Largest charting ecosystem |
| **Charts** | Apache ECharts (echarts-for-react) | Rich chart types, handles large datasets, fully client-side zoom/pan/tooltips |
| **Styling** | Tailwind CSS | Utility-first, good for non-frontend specialists |

### 4.3 Chart definition (on disk)

Each chart is a pair of artifacts authored by the provider and stored on disk alongside the server config:

- **A SQL query** — run server-side against the provider's database; produces the rows the chart needs.
- **A JavaScript ES module** — shipped as-is to the browser; exports a default function that maps rows to an ECharts option object.

The provider lists each chart in the server config with `id`, `title`, a path to the SQL file (or inline SQL), a path to the JS module file, and an optional `cache_ttl_seconds`. Exact TOML shape will be finalized during implementation and is deliberately left out of this document so it can evolve freely.

Chart module contract:

```js
// charts/volume_chart.js
export default function build(rows, meta) {
  // rows:  Array<Record<string, unknown>>         (from the chart's SQL)
  // meta:  { id, title, generatedAt }
  // returns: an ECharts option object
  return {
    title: { text: meta.title },
    xAxis: { type: 'category', data: rows.map(r => r.day) },
    yAxis: { type: 'value' },
    series: [{ type: 'line', data: rows.map(r => r.volume), smooth: true }],
  };
}
```

Modules are plain ES modules (no bundler, no TypeScript). The browser loads them via dynamic `import()`. A module that throws at load time or at call time produces a visible error card for that chart — it does not break the rest of the dashboard.

### 4.4 Query execution and caching

When `GET /dashboard/api/charts/:id/data` is called:

1. Server looks up chart `:id`.
2. If the chart has a fresh cached result (within its `cache_ttl_seconds`), return it.
3. Otherwise:
   a. Execute the chart's preconfigured SQL against the configured database.
   b. Serialize the result rows to JSON.
   c. Cache the result in an in-memory TTL map keyed by chart id.
   d. Return the JSON.

`GET /dashboard/api/charts/:id/module` is a plain static-file serve from the chart's configured module path on disk, with `ETag` derived from mtime and content-type `application/javascript`.

In-memory caching is enough for the MVP.

**SQL safety.** Dashboard SQL is authored by the provider in the server config. It is not influenced by visitor input. The server will:
- Enforce a per-query timeout (configurable, default e.g. 10s).
- Log slow queries.
- **Not** attempt to parse or restrict the SQL — it is trusted.

**Module safety.** Chart modules run in the visitor's browser. They are authored by the provider (trusted: anyone who can deploy the server can already run arbitrary code against their visitors). No parsing, sandboxing, or validation is performed on the module contents.

### 4.5 Dashboard data flow

```
┌─── Visitor opens /dashboard/ ──────────────────────────────────────────────┐
│                                                                            │
│ 1. Browser loads SPA (index.html + JS/CSS) from the Rust server.           │
│                                                                            │
│ 2. SPA calls GET /dashboard/api/charts                                     │
│    → JSON list: [{ id, title, moduleUrl, dataUrl }, ...]                   │
│                                                                            │
│ 3. For each chart, in parallel:                                            │
│      a. import(moduleUrl)    → chart-specific build() function             │
│      b. fetch(dataUrl)       → { rows, generatedAt }                       │
│                                                                            │
│ 4. SPA calls module.default(rows, meta) → ECharts option object,           │
│    and renders it with echarts-for-react.                                  │
│                                                                            │
│ 5. User interacts: zoom, pan, tooltips, legend toggling.                   │
│    All of this is client-side, no server roundtrip.                        │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘
```

No wallet connection. No 402. No x402 signatures anywhere on this path.

### 4.6 Frontend (React SPA) responsibilities

- Fetch `/dashboard/api/charts` on load; render a grid of chart cards.
- For each chart, concurrently `import(moduleUrl)` and `fetch(dataUrl)`; then call `module.default(rows, meta)` and render the returned option with `echarts-for-react`.
- Catch and display errors from module load, data fetch, or module execution as a per-card error state — do not crash other charts.

The SPA's chart card component is small:

```tsx
// frontend/src/charts/ChartCard.tsx (sketch)
type ChartDescriptor = { id: string; title: string; moduleUrl: string; dataUrl: string };
type ChartData = { rows: Record<string, unknown>[]; generatedAt: string };
type BuildFn = (rows: ChartData["rows"], meta: { id: string; title: string; generatedAt: string }) => echarts.EChartsOption;

async function loadChart(desc: ChartDescriptor): Promise<echarts.EChartsOption> {
  const [mod, data] = await Promise.all([
    import(/* @vite-ignore */ desc.moduleUrl) as Promise<{ default: BuildFn }>,
    fetch(desc.dataUrl).then(r => r.json() as Promise<ChartData>),
  ]);
  return mod.default(data.rows, { id: desc.id, title: desc.title, generatedAt: data.generatedAt });
}
```

### 4.7 Visual design direction

Inspired by Dune Analytics' data hub:
- Dark theme by default.
- Card-based layout for charts and metrics.
- Clean typography, data-dense but readable.
- Responsive grid layout for chart panels.
- Each card has a title, a last-updated timestamp, and a subtle reload button.
- No wallet-connect UI anywhere. No payment UI anywhere.

### 4.8 Build and packaging

Two options for how the SPA is shipped with the Rust server:

**Option A — Embed at compile time (preferred).** The SPA is built with Vite (`vite build`), producing a `dist/` directory. A Rust build step embeds `dist/` into the binary using `include_dir!` or `rust-embed`. The server serves these assets at `/dashboard/assets/*` directly from memory. One binary, zero external files.

**Option B — Serve from disk.** The SPA's `dist/` directory ships alongside the Rust binary. The server reads from disk at runtime. Simpler to iterate on during development; slightly more fragile to deploy.

MVP picks **Option A** for single-binary deploys. During development, the Rust server can be configured to proxy `/dashboard/*` to the Vite dev server on port 5173 for hot reload.

### 4.9 Deployment

The dashboard is not a separate deployment. It is the same Rust binary that already serves the paid API. Any existing hosting approach for tiders-x402-server works unchanged:

- **Single binary on a VM.** Copy the binary + config.
- **Docker.** One image, one container, one port.
- **Behind a reverse proxy.** Nginx/Caddy terminates TLS; the Rust server handles everything under `/` including `/dashboard/`.

---
