# Installation

## CLI

The CLI is distributed as a prebuilt binary via both pip and cargo. Pick whichever you prefer — they install the same `tiders-x402-server` binary with all database backends bundled.

```bash
pip install tiders-x402-server
# or
cargo install tiders-x402-server
```

Once installed, see the [CLI Quick Start](./cli-quickstart.md) to get running.

## Python SDK

To embed the server in your own Python code, install the SDK instead of the CLI:

```bash
uv pip install tiders-x402-server-sdk
```
```python
import tiders_x402_server
```

The python SDK includes all database backends (DuckDB, PostgreSQL, ClickHouse).

Running the example:

```bash
cd examples/python
uv run duckdb_server.py
```

*You need a virtual environment active. Use uv venv && source .venv/bin/activate*

## Rust SDK

By default `tiders-x402-server` enables all three database backends and the CLI dependencies. If you're embedding it as a library and only need one backend, opt out of the defaults:

| Feature | Description |
|---|---|
| `duckdb` | DuckDB backend |
| `postgresql` | PostgreSQL backend |
| `clickhouse` | ClickHouse backend |
| `cli` | CLI/YAML loader, file watcher, and related deps (default) |
| `pyo3` | Python bindings (used by the SDK wheel) |

```toml
[dependencies]
tiders-x402-server = { version = "0.2", default-features = false, features = ["duckdb"] }
```

Or combine multiple backends:

```toml
[dependencies]
tiders-x402-server = { version = "0.2", default-features = false, features = ["duckdb", "clickhouse", "postgresql"] }
```

Running the example:

```bash
cd examples/rust
cargo run
```

The Rust example (`examples/rust/Cargo.toml`) enables all three backends. Edit its `features` list if you only need one.

## Development Setup

Build the CLI from source:

 ```bash
 cargo install --path server
 ```

If you're modifying `tiders-x402-server` repo locally, you probably want to build the example against your local version.

### Python

Build the Python binding using maturin:

```bash
cd python
maturin develop --uv   # builds the Rust extension and installs it into the active venv
```

### Rust

Build the example with the local server:

```bash
cargo build --config 'patch.crates-io.tiders-x402-server="../../server"'
```

**Persistent local development**

For persistent local development, you can put this in `examples/rust/Cargo.toml`:

```toml
[patch.crates-io]
tiders-x402-server = { path = "../../server" }
```

This avoids passing `--config` on every build command.