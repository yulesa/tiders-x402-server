# Tiders dashboard — SPA implementation plan
**Scope:** Front-end SPA plus a one-off server URL rework that puts the dashboard at `/` and the whole API (paid + dashboard) under `/api/`.

---

## Goals

Build the React + TypeScript + Vite single-page app that the Rust server
embeds at `/`. The SPA must:

1. Render a Dune-style, dark-themed grid of chart cards for a provider's data.
2. Load each chart by fetching its on-disk JS module and its Arrow IPC data in
   parallel, then rendering the resulting ECharts option.
3. Degrade gracefully — a broken chart produces a per-card error without
   breaking the rest of the grid.
4. Offer a "Download data" action that lets a visitor edit the chart's SQL,
   connect a wallet, pay via x402, and save the result as CSV.

Non-goals:

- A paid surface for the dashboard itself (only `POST /api/query` is paid).
- Server-side rendering, auth.
- A dashboard-admin UI — charts are authored by editing files on disk.
- CORS / dual-host support. The single-origin shape below maps cleanly onto
  a later Caddy-based subdomain split (`example.com` → `/`, `api.example.com`
  → `/api/*` with a path rewrite) without any further server changes.

## Server URL shape (after commit 2's rework)

**Browser surface** — served to visitors and their browsers:

| Path | Response |
|------|----------|
| `GET /` | SPA `index.html` |
| `GET /assets/{*path}` | SPA bundle files (hashed JS/CSS/fonts) |
| `GET /favicon.svg` | Favicon |

**API surface** — all JSON or Arrow IPC, consumed by clients + the SPA:

| Method | Path | Auth | Response |
|--------|------|------|----------|
| `GET` | `/api` | free | JSON server metadata, table list, SQL rules |
| `POST` | `/api/query` | x402 | Arrow IPC (200) or payment requirements (402) |
| `GET` | `/api/table/{name}` | free or x402 | JSON schema + pricing |
| `GET` | `/api/charts` | free | JSON `{ title, charts: [{ id, title, sql, moduleUrl, dataUrl }, ...] }` |
| `GET` | `/api/charts/{id}/data` | free | Arrow IPC (`application/vnd.apache.arrow.stream`), `X-Tiders-Generated-At` header |
| `GET` | `/api/charts/{id}/module` | free | `application/javascript` ES module (served from disk, mtime ETag) |

**Important correction vs. the draft architecture doc.** The draft
(`tiders-dashboards-architecture-draft.md`) nests everything under
`/dashboard/*` and says the chart data endpoint returns JSON. Neither is
true after this plan lands: the URL prefix is gone, and chart data is Arrow
IPC (the SPA decodes it client-side so chart-module authors still receive
plain row objects).

## Tech stack

- **Vite** (build + dev server) with React + TypeScript.
- **React 19**, `echarts` + `echarts-for-react` for rendering.
- **Tailwind CSS v4** (PostCSS-free, `@import "tailwindcss"`).
- **`apache-arrow`** for decoding chart data and paid `POST /api/query` responses.
- **wagmi + viem** for the download-button wallet flow.
- No router needed — one page.

## Layout

- `dashboard-frontend/` (repo root) — the Vite project. Source of truth for the SPA.
- `server/assets/dashboard/` — Vite's build output. Committed (or built in CI; see commit 6) so `rust-embed` can include it at `cargo build` time.

## Commit list

1. **[DONE] Bootstrap Vite + React + Tailwind v4 SPA wired to `server/assets/dashboard/`**
2. **Server URL rework: move API under `/api/`, SPA at `/`**
3. **App shell, chart catalog fetch, per-card states, reload button**
4. **Arrow IPC decode + dynamic module import + ECharts rendering**
5. **Wallet connect + download modal with paid `POST /api/query` and CSV export**
6. **Build script + CI + README: build the SPA as part of `cargo build`**
7. **Delete placeholder bundle and reconcile architecture doc**

---

## Commit 1 — Bootstrap Vite + React + Tailwind v4 SPA wired to `server/assets/dashboard/` [DONE]

Vite project scaffolded at `dashboard-frontend/`. Build output lands in
`server/assets/dashboard/` so `rust-embed` picks it up at `cargo build` time.

Runtime deps pinned now (echarts, echarts-for-react, apache-arrow, tailwindcss
v4) to avoid later version churn. Minimal dark-themed `App.tsx` placeholder.

Current Vite config uses `base: "/dashboard/"` — **commit 2 flips that to `/`**
as part of the URL rework. Same for the dev-server proxy.

## Commit 2 — Server URL rework: move API under `/api/`, SPA at `/`

Break the legacy `POST /query` + `GET /table/{name}` paths in one clean commit, alongside dropping the `/dashboard/` prefix from SPA + dashboard-API paths.

### Server routing

Replace the current top-level + nested-dashboard routers with a single flat router:

- **Browser surface at the root.**
  - `GET /` → `serve_index` → embedded `index.html`.
  - `GET /assets/{*path}` → `serve_asset_in_assets` → embedded bundle file.
  - `GET /favicon.svg` → `serve_favicon`.
- **Paid + dashboard API under `/api/`.**
  - `GET /api` → new handler that replaces `root_handler`, returns **JSON** (see below).
  - `POST /api/query` → existing `query_handler`.
  - `GET /api/table/{name}` → existing `table_detail_handler`.
  - `GET /api/charts` → existing `list_charts`.
  - `GET /api/charts/{id}/data` → existing `chart_data`.
  - `GET /api/charts/{id}/module` → existing `chart_module`.
- **Drop the `/dashboard/` nest entirely.** Also drop the top-level `/dashboard/` → `/dashboard` redirect route in `lib.rs` — no longer needed.

### JSON `/api`

Convert the plain-text `root_handler` to a JSON response with roughly this shape (field names TBD during implementation):

```json
{
  "name": "tiders-x402-server",
  "version": "0.2.0",
  "tables": [
    { "name": "uniswap_v3_pool_swap", "description": "Uniswap V3 pool swaps", "paymentRequired": true, "detailsUrl": "/api/table/uniswap_v3_pool_swap" }
  ],
  "sqlRules": [
    "Only SELECT statements",
    "Only one statement per request",
    "Only one table in the FROM clause",
    "No GROUP BY / HAVING / JOIN / subqueries",
    "Only simple field names in SELECT, no expressions",
    "WHERE, ORDER BY, LIMIT allowed with restrictions"
  ],
  "x402DocsUrl": "https://x402.gitbook.io/x402"
}
```

Keep it small. Don't add fields "just in case" — a real client that needs more can fetch `/api/table/{name}` per-table.

### Breaking changes to document/update

- `POST /query` → `POST /api/query`. Update:
  - `examples/cli/tiders-x402-server.yaml` comments if any reference the path (the file itself doesn't hardcode URLs; double-check).
  - `examples/python/duckdb_server.py`, `examples/rust/src/main.rs` if they call the paid API.
  - `tiders-x402-ts-client` — change the default endpoint. (Out of scope for this repo; open a companion PR there.)
  - `docs/` if they mention the paths.
  - Root `README.md`.
- `GET /table/{name}` → `GET /api/table/{name}`. Same update list.
- Client libraries and downstream callers are expected to break. That's the cost; the subdomain split becomes one Caddy stanza later.

### SPA adjustments

The Vite scaffold from commit 1 still has dashboard-era URLs. Flip them:

- `vite.config.ts`: `base: "/dashboard/"` → `base: "/"`; dev proxy `"/dashboard/api"` → `"/api"` targeting `http://localhost:4021`.
- `index.html` favicon link: `/dashboard/favicon.svg` → `/favicon.svg`.
- `dashboard-frontend/README.md`: update the dev URL to `http://localhost:5173/` and adjust the proxy description.
- Run `npm run build` to regenerate `server/assets/dashboard/` with the new asset URLs, so `index.html` references `/assets/<hash>.js` instead of `/dashboard/assets/<hash>.js`.

### Exit criteria

- `curl http://localhost:4021/` → 200, HTML, Vite-built index.
- `curl http://localhost:4021/assets/index-<hash>.js` → 200, `application/javascript`.
- `curl http://localhost:4021/favicon.svg` → 200, `image/svg+xml`.
- `curl http://localhost:4021/api` → 200, `application/json`, parseable.
- `curl -X POST http://localhost:4021/api/query -d '{"sql":"SELECT 1"}'` → 402 with payment requirements (unchanged behaviour, new URL).
- `curl http://localhost:4021/api/table/uniswap_v3_pool_swap` → valid response.
- `curl http://localhost:4021/api/charts` → 200, JSON list.
- `curl http://localhost:4021/query` → 404. `curl http://localhost:4021/dashboard` → 404. Legacy paths gone.
- Browser at `http://localhost:4021/` shows the Vite-built "Tiders dashboard" placeholder on a dark background.

## Commit 3 — App shell, chart catalog fetch, per-card states, reload button

Turn the hello-world into a real dashboard layout that fetches the catalog, renders one card per chart (title only), and supports per-card reload. Still no chart rendering — that's commit 4.

- `src/App.tsx`: top bar with the dashboard title, subtle border, responsive container.
- `src/components/ChartGrid.tsx`: CSS grid, 1 / 2 / 3 columns across small / medium / large breakpoints via Tailwind.
- `src/components/ChartCard.tsx`: card shell with title, placeholder chart area, footer with "Updated Xs ago" slot and a reload button. Four states: `loading` (pulsing skeleton), `ready`, `empty`, `error` (compact red block with retry).
- `src/styles/theme.css` (or Tailwind config): Dune-inspired palette — near-black background, low-chroma gray borders, a single accent color.
- `src/api/catalog.ts`: `fetchCatalog(): Promise<Catalog>` — plain `fetch("/api/charts")`, typed response. 503 from the server (`Unavailable`) becomes a dashboard-wide empty state ("Dashboard not configured").
- `src/api/types.ts`: `ChartDescriptor = { id, title, sql, moduleUrl, dataUrl }`, matching the server's `camelCase` serde output.
- `src/App.tsx` top-level states: loading (3 skeleton cards), error (single centered block with retry), empty ("No charts configured yet"), populated.
- Reload button: stub that calls a no-op handler for now; commit 4 wires it to the real fetch.
- "Updated Xs ago" ticker: single `setInterval` at the grid level, pushing relative-time strings into each card — not one interval per card.

**Required server changes.**

- Extend `list_charts` to return `{ title, charts: [...] }` instead of a bare array so the SPA header matches the provider's configured title.
- Add `sql` to `ChartDescriptor` on each entry. SQL becoming public is fine — the dashboard exists to advertise what data is available, and the paid API already publishes schema via `GET /api/table/{name}`. Needed now so the commit 5 download modal can pre-fill it without a separate fetch.

Both changes are small (~10 lines total in `handler_dashboard.rs`) + the module-level doc comment.

**Exit criteria.** With the example config (one chart, `daily_volume`), the SPA renders a header with "Tiders demo dashboard" and one card titled "Daily swap volume" in a "no data yet" placeholder state. With `dashboard.enabled: false`, the page shows the "not configured" empty state. The responsive grid works at small / medium / large viewport widths.

## Commit 4 — Arrow IPC decode + dynamic module import + ECharts rendering

The heart of the feature. Each card fetches its data and module in parallel, then calls the module's default export to get an ECharts option and renders it.

- `src/api/arrow.ts`:
  - `fetchChartData(dataUrl): Promise<{ rows: Record<string, unknown>[]; generatedAt: Date }>`.
  - Uses `fetch(dataUrl)`, reads response as `ArrayBuffer`, decodes with `apache-arrow`'s `tableFromIPC`, converts rows to plain JS objects. Reads `X-Tiders-Generated-At` for `generatedAt`.
  - Normalizes `BigInt` columns to `Number` where they fit in `Number.MAX_SAFE_INTEGER`, otherwise to string. ECharts does not accept BigInt. Chart-module authors can assume JSON-safe rows.
- `src/api/module.ts`:
  - `loadChartModule(moduleUrl): Promise<BuildFn>` — `const mod = await import(/* @vite-ignore */ moduleUrl); return mod.default;`. The `/* @vite-ignore */` stops Vite from trying to bundle the URL.
- `src/components/ChartCard.tsx`:
  - On mount (and on reload-button click) run `Promise.all([loadChartModule, fetchChartData])`, then `build(rows, { id, title, generatedAt })`, then render via `<ReactECharts option={option} theme="dark" style={{ height: 320 }} />`.
  - Four error categories — module load, data fetch, build(), render — all bubble into the existing error state from commit 3. Reload button re-triggers the whole fetch.
- Confirm the example module (`examples/cli/charts/daily_volume.js`) renders: a bar chart of `day` vs `swap_count` against the example DuckDB file.

**Exit criteria.** Browsing `/` with the example config shows a fully-rendered "Daily swap volume" bar chart. Force-throwing inside the module produces a per-card error while other charts keep working. Clicking reload on one card refetches that card only. "Updated Xs ago" increments live after a successful load.

## Commit 5 — Wallet connect + download modal with paid `POST /api/query` and CSV export

The paid flow, end to end, in the browser. Wagmi scaffolding ships together with its only consumer so no dead code lands in a standalone commit.

- Add deps: `wagmi`, `viem`, `@tanstack/react-query`. (wagmi v2 requires react-query.)
- `src/wallet/config.ts`: `createConfig` with `mainnet` and `baseSepolia` (the server example pays in `usdc/base_sepolia`). `injected()` and `walletConnect()` connectors; WalletConnect project id read from `VITE_WALLETCONNECT_PROJECT_ID`, with `injected`-only fallback if unset.
- Wrap `App.tsx` in `<WagmiProvider>` and `<QueryClientProvider>`.
- Header: small "Connect wallet" / "0x1234…abcd" button backed by `useAccount` + `useConnect` + `useDisconnect`. Simple dropdown for connector choice — no custom wallet modal.
- `src/wallet/signer.ts`: `useX402Signer()` hook exposing `sign(payload)` via viem's `signTypedData`.
- `src/components/DownloadModal.tsx`:
  - Trigger: "Download data" button in each card's footer (next to the reload button from commit 3).
  - Modal contents: `<textarea>` pre-filled with the chart's SQL (from the catalog), a one-sentence explainer, a "Connect wallet" CTA (reusing the header state) if disconnected, and "Run & download" / "Cancel" actions.
- `src/api/paid.ts`: `runPaidQuery({ sql, signer, account })` implements the x402 flow:
  1. `POST /api/query` with JSON `{ sql }`.
  2. 200 → decode Arrow IPC → return rows.
  3. 402 → parse payment requirements, build EIP-712 payload, `signer.sign(payload)`, retry `POST /api/query` with `X-PAYMENT` header, decode Arrow IPC.
  4. Anything else surfaces as an error in the modal.
- `src/util/csv.ts`: `rowsToCsv(rows)` — RFC-4180-ish serializer (quote fields containing `"`, `,`, `\n`; double quotes escape). Fires a `Blob` download named `<chart_id>-<YYYYMMDD-HHmm>.csv`.
- Error surfaces: wallet-rejected signature, payment failure, query failure, network failure — each a distinct message in the modal.

**Exit criteria.** With the example config, clicking "Download data" on the `daily_volume` card opens the modal pre-filled with the chart SQL; connecting an injected wallet, editing the SQL if desired, and clicking "Run & download" triggers a wallet signature, the server returns Arrow IPC, and the browser downloads `daily_volume-YYYYMMDD-HHmm.csv` with the expected rows. Rejecting the signature shows an error and leaves the modal open. Reload and the chart view itself keep working.

## Commit 6 — Build script + CI + README: build the SPA as part of `cargo build`

Make sure Rust-only contributors can `cargo run` without touching the Node toolchain, and catch SPA-forgot-to-rebuild mistakes in CI.

- `server/build.rs`: optional build script that runs `npm ci && npm run build` in `../dashboard-frontend` when the Vite bundle is missing from `server/assets/dashboard/`, and warns (not errors) when `npm` is unavailable. Guarded by `TIDERS_SKIP_SPA_BUILD=1` for fast Rust-only iteration.
- Commit the build output (`server/assets/dashboard/*`) to git so a plain `cargo build` on a fresh clone always works without Node installed. The SPA is small (tens of KB minified); requiring Node for every Rust developer is worse.
- Root `README.md`: new "Dashboard development" section — `cd dashboard-frontend && npm install && npm run dev`, with the two-origin story (Vite on 5173, Rust on 4021).
- CI: GitHub Actions job that runs `npm ci && npm run build` in `dashboard-frontend` and then `git diff --exit-code server/assets/dashboard/` to catch commits that forgot to rebuild. If no CI workflow exists yet, mention this as a follow-up instead of adding one here.

**Exit criteria.** A fresh clone → `cargo build -p tiders-x402-server` succeeds without touching `dashboard-frontend`, and the resulting binary serves the real SPA. `cd dashboard-frontend && npm run build && git diff --stat server/assets/dashboard/` is empty.

## Commit 7 — Delete placeholder bundle and reconcile architecture doc

Housekeeping.

- `git rm` any files under `server/assets/dashboard/` that the Vite build does not produce (the old `app.css` and the placeholder `index.html` were overwritten in commit 1; this commit makes the removal explicit in history if anything stragglers remain).
- Update `tiders-dashboards-architecture-draft.md`:
  - §1.1 ASCII diagram: replace `/dashboard/...` paths with the `/` + `/api/...` shape.
  - §2.1 + §2.2 endpoint tables: rewrite to match the "Server URL shape" table at the top of this document.
  - §2.1 paid API: `POST /query` → `POST /api/query`, `GET /table/:name` → `GET /api/table/:name`; root plain-text → `GET /api` JSON.
  - §4.5 step 3b: "fetch(dataUrl) → { rows, generatedAt }" → "fetch(dataUrl) → Arrow IPC → decode to { rows, generatedAt }".
  - §4.2 tech stack: add `wagmi + viem` and `apache-arrow` rows.
  - §1.2 "download button": cross-reference commit 5 in this plan.
  - Flip "Status: Draft" to "Implemented" (or delete the field — your call).
  - Add a short "Deployment — subdomain split" subsection describing the optional Caddy setup: `example.com` → the full server, `api.example.com` with `rewrite * /api{uri}` → the same server, so API clients get clean `api.example.com/query` URLs without any server code changes.
- Remove this file (`dashboard-spa-plan.md`) if you prefer the architecture doc as single source of truth, or keep it as an implementation journal.

**Exit criteria.** No stale placeholder files; architecture doc matches reality.

---

## Risks and follow-ups (not part of the 7 commits)

- **Downstream client libs.** `tiders-x402-ts-client` and the example `examples/rust/` / `examples/python/` callers need matching PRs after commit 2. Track separately.
- **Module CSP.** `import()` of arbitrary JS from the provider's own origin is fine, but if a provider ever puts the server behind a strict CSP, dynamic imports could be blocked. Note it in provider docs when that day comes.
- **BigInt handling.** Arrow's 64-bit integer columns arrive as `BigInt`. The commit 4 decoder converts them to `Number` when they fit, otherwise string. Document in a short `CHART_AUTHORING.md` next to the chart examples (follow-up, not blocking).
- **wagmi version churn.** Pin wagmi/viem exactly in `package.json`.
- **Mobile layout.** The grid already responds at breakpoints, but the download modal on small screens needs a once-over after commit 5.
