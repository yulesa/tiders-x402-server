import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Paths and ports are coupled to the Rust server's routes
// (see server/src/lib.rs and server/src/dashboard/handler_dashboard.rs)
// and the example config at examples/cli/tiders-x402-server.yaml.
//
// - `base: "/"` so built asset URLs match `/assets/*`, which
//   `serve_asset_in_assets` serves out of the embedded `rust-embed` bundle.
// - `build.outDir` writes the bundle into `server/assets/dashboard/`, which
//   is the `#[folder = "assets/dashboard/"]` source for `rust-embed`.
// - `server.proxy` lets `vite dev` on :5173 reach the Rust API on :4021,
//   covering both the dashboard endpoints and the paid `POST /api/query`.
export default defineConfig({
  base: "/",
  plugins: [react(), tailwindcss()],
  build: {
    outDir: "../server/assets/dashboard",
    emptyOutDir: true,
  },
  server: {
    proxy: {
      "/api": {
        target: "http://localhost:4021",
        changeOrigin: false,
      },
    },
  },
});
