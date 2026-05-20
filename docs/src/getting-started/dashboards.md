# Dashboards

Tiders can publish [Evidence](https://evidence.dev/) dashboards alongside its paid API — let buyers preview your data visually before deciding to pay for the full dataset.

Each dashboard is an [Evidence](https://evidence.dev/) project: a folder of Markdown files with embedded SQL queries that compile into a fast, self-contained static website. Tiders scaffolds the project for you, pre-wires it to your database, and serves the built site at `/<slug>/`. Dashboards are optional — omit the `dashboards:` block and the server only exposes `/api/*`.

When at least one dashboard is configured, the root path `/` becomes a generated landing page listing every dashboard. Each dashboard also ships with a built-in wallet-connect button: visitors can connect a crypto wallet directly in the page and pay to download the underlying updated table as a CSV — the full x402 payment flow, with no extra code required on your end.

Dashboards are static — they do not update live. Whenever the underlying data changes, rebuild the project and the server picks up the new files automatically.

> **Note:** Content rendered in the dashboard is publicly visible and can be scraped without payment. Only show data you are comfortable sharing freely; keep sensitive tables behind the paid API.

## Why Evidence

[Evidence](https://evidence.dev/) is a static-site generator built for data dashboards. Pages are witten with commands in Markdown files with embedded SQL queries — accessible to data engineers and analysts, yet flexible enough for rich charts, narrative context, and programmatic logic. At build time, `npm run build` runs the queries, writes the results to Parquet files, and bundles everything into a static site. At runtime, the server just serves HTML, JS, and CSS — no database calls, no server-side rendering, making pages fast and lightweight.

## Workflow

1. Add an entry under `dashboards:` in your config (see [YAML Reference](../cli/yaml-reference.md#dashboards)).
2. Scaffold the project: `tiders-x402-server dashboard <slug>`.
3. Edit the files creating your dashboard page.
4. Build it: `cd dashboards/<slug> && npm install && npm run build`.
5. Start the server: `tiders-x402-server start`. The built site is served at `/<slug>/`.

The same flow applies to refreshes: edit your `pages/*.md`, rerun `npm run build`, and the live server picks up the new files (no restart needed).

## Quick Example

```yaml
# tiders-x402-server.yaml

dashboards:
  entries:
    - slug: uniswap_v3
      title: "Uniswap V3"
      description: "Pool swaps and liquidity events on Uniswap V3."
      tags: ["Dex", "DeFi", "Ethereum"]
```

```bash
tiders-x402-server dashboard uniswap_v3
cd dashboards/uniswap_v3 && npm install && npm run build
cd ../.. && tiders-x402-server start
```

Open <http://localhost:4021/> for the landing page or <http://localhost:4021/uniswap_v3/> for the dashboard itself.

## Scaffolded Project Layout

```
dashboards/
  index.html                  # generated landing page snapshot
  <slug>/
    pages/
      +layout.svelte          # wraps the page with Tiders components (overwritten on --force)
      index.md                # (edit) user-owned dashboard main page (created once, never overwritten)
    sources/                  # dashboard data sources referenced in the pages
      <source_name>/
        connection.yaml       # (edit) evidence database connection credetials: (DuckDB / PG / ClickHouse)
        <table>.sql           # (edit) database sourced sql files: `select * from <table> limit 10`
    components/               # Tiders custom components and libraries.
      ConnectButton.svelte
      WalletPicker.svelte
      TidersDownloadButton.svelte
      TidersDownloadModal.svelte
      lib/
        eip6963.ts            # wallet discovery
        wagmi.ts              # wagmi config
        walletStore.ts
        x402Client.ts         # x402 payment + Arrow IPC fetch
        arrowToCsv.ts
    build/                    # produced by `npm run build` (served at runtime)
    .tiders-managed.json      # sha256 manifest of every managed file (drift detection)
    .npmrc
    .gitignore
    package.json
    evidence.config.yaml      # evidence configs
```
*`(edit)` are the main files users need to edit.*

## Multi-page Dashboard vs Multiple Dashboards

Evidence supports multiple pages within a single dashboard project — just add more `.md` files under `pages/`. You can use this to group related data into sections within one site, or create separate dashboard entries in Tiders for each group.

| | Single dashboard, multiple pages | Multiple dashboards |
|---|---|---|
| **Build** | One `npm run build` covers everything | Each project builds independently |
| **Linking** | Easy cross-page navigation and shared components | Separate sites; linking requires full URLs |
| **Isolation** | A change anywhere requires a full rebuild | Rebuild only the affected dashboard |

Use multiple pages when your content is closely related and you want a unified site. Use separate dashboards when the topics are independent and you want to rebuild or deploy them separately.

## Hot-Reload of `dashboards:`

The CLI watches the YAML config and atomically swaps the live dashboard router (`arc-swap`) when entries are added, removed, enabled, or disabled.

## Bundled Wallet Connect + Paid Download

The `TidersDownloadButton` + `TidersDownloadModal` components embedded in every scaffolded project handle the full x402 dance:

1. Discover EIP-6963 wallets in the browser.
2. Let the user pick one and connect via wagmi.
3. Submit a query to `/api/query`, intercept the 402, and prompt the user to sign the payment via `@x402/evm`.
4. Resubmit with the `Payment-Signature` header, decode the Arrow IPC response, and offer it as a CSV download.

Drop `<TidersDownloadButton table="my_table" />` into any Markdown page to expose a paid download for that table.
