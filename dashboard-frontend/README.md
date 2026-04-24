# Tiders dashboard — SPA

React + TypeScript + Vite + Tailwind v4 single-page app served by the Rust
`tiders-x402-server` at `/`. Built output lands in
`../server/assets/dashboard/`, which `rust-embed` bakes into the server
binary at `cargo build` time.

## Develop

The SPA talks to the Rust server for its catalog, chart data, chart
modules, and the paid `POST /api/query`. Run both side by side:

```sh
# terminal 1 — Rust server on :4021 (from examples/cli/)
cd examples/cli && tiders-x402-server start

# terminal 2 — Vite dev server on :5173 (from this directory)
npm install
npm run dev
```

Then open http://localhost:5173/. Vite's dev server proxies any
`/api/*` request to `http://localhost:4021`, so the catalog, chart,
and paid-query endpoints work with no CORS glue.

## Build

```sh
npm run build
```

Writes the bundle to `../server/assets/dashboard/`, overwriting whatever
was there. The next `cargo build` of the server embeds the new bundle.
