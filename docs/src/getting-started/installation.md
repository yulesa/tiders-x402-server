# Installation

## CLI

Install from crates or pip.

```bash
cargo install tiders-x402-server-cli
```

Once installed, see the [CLI Quick Start](./cli-quickstart.md) to get running.

## Python

Using uv (recommended):

```bash
uv pip install tiders-x402-server
```
```python
import tiders_x402_server
```

The published Python package includes all database backends (DuckDB, PostgreSQL, ClickHouse).

Running the example:

```bash
cd examples/python
uv run duckdb_server.py
```

*You need a virtual environment active. Use uv venv && source .venv/bin/activate*

## Rust

Each database backend is a separate Cargo feature. You must enable at least one:

| Feature | Database |
|---|---|
| `duckdb` | DuckDB |
| `postgresql` | PostgreSQL |
| `clickhouse` | ClickHouse |

No database is included by default — this keeps compile times and binary size down when you only need one backend.

Add `tiders-x402` to your `Cargo.toml` with the features you need:

```toml
[dependencies]
tiders-x402 = { version = "0.1.0", features = ["duckdb"] }
```

Or combine multiple backends:

```toml
[dependencies]
tiders-x402 = { version = "0.1.0", features = ["duckdb", "clickhouse", "postgresql"] }
```

Running the example:

```bash
cd examples/rust
cargo run
```

The Rust example (`examples/rust/Cargo.toml`) enables all three backends by default. Edit its `features` list if you only need one.

## Development Setup

Build the CLI from source:

 ```bash
 cargo install --path cli
 ```
 
If you're modifying `tiders-x402-server` repo locally, you probably want to build it against your local version.

### Python

Build the Python binding using maturin:

```bash
cd python
maturin develop --uv   # builds the Rust extension and installs it into the active venv
```

### Rust

Build the example with the local server:

```bash
cargo build --config 'patch.crates-io.tiders-x402="../../server"'
```

**Persistent local development**

For persistent local development, you can put this in `examples/rust/Cargo.toml`:

```toml
[patch.crates-io]
tiders-x402 = { path = "../../server" }
```

This avoids passing `--config` on every build command.