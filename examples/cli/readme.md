# CLI example — DuckDB + Uniswap V3 swaps

This example runs `tiders-x402-server` against a local DuckDB file seeded with the sample `uniswap_v3_pool_swap.csv` dataset using python.

## Prerequisites

```bash
  cd examples/cli
```

- **Create a venv with [uv](https://docs.astral.sh/uv/) (Optional)**:

  ```bash
  uv venv
  source .venv/bin/activate
  ```

- **Install Tiders-x402-Server**:

  ```bash
  uv pip install tiders-x402-server
  # or
  pip install tiders-x402-server
  ```

- **Install [DuckDB CLI](https://duckdb.org/docs/installation/)**:

  ```bash
  uv pip install duckdb
  # or
  pip install duckdb
  ```

- **Create the DuckDB db and seed it with some data**:

  The YAML config points at `../data/duckdb.db`, and the seed CSV lives at `../uniswap_v3_pool_swap.csv`. The table name must match the `tables:` entry in the config (`uniswap_v3_pool_swap`).

  ```bash
  mkdir -p ../data
  duckdb ../data/duckdb.db \
    "CREATE TABLE uniswap_v3_pool_swap AS SELECT * FROM read_csv_auto('../uniswap_v3_pool_swap.csv');"
  ```

### 2. Start the server

```bash
tiders-x402-server start tiders-x402-server.yaml
```

The server binds to `0.0.0.0:4021` by default. Verify it's up in another terminal:

```bash
curl http://localhost:4021/api
```

### 3. Create a dashboard page (Optional)

This step can be done before or after having the server running in another terminal. It requires [node](https://nodejs.org/en/download) installed.

```bash
  cd examples/cli
```

- **Scaffold the Dashboard Project**:
  ```bash
  tiders-x402-server dashboard
  ```

- **Install dependencies and build this dashboard page**:

  ```bash
  (cd dashboards/uniswap_v3 && npm install && npm run build)
  ```

- **Tiders will serve the page at http://localhost:4021/**

- **Edit the dashboard project**:
  - Main files to edit are `pages/index.md`, `sources/`
  - Open the file `dashboards/uniswap_v3/pages/index.md` and add `<Value data={sample} column="sender" row={0} />` above the dataTable.
  - Rebuild the page with `(cd dashboards/uniswap_v3 && npm run build)`

Dashboards are static — they do not update live. Rebuild whenever the underlying data changes.