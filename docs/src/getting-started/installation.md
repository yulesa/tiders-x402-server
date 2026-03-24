# Installation

## Python

Using uv (recommended):

```bash
uv pip install tiders-x402-server
```
```python
import tiders_x402_server
```

Running the example:

```bash
cd examples/python
uv run duckdb_server.py
```

*You need a virtual environment active. Use uv venv && source .venv/bin/activate*

## Rust

```bash
cd examples/rust
cargo run
```

## Development Setup

If you're modifying `tiders-x402-server` repo locally, you probably want to build it against your local version.

### Python

Build the python binding using maturin:

```bash
cd python   #python project
maturin develop --uv   # builds the Rust extension and installs it into the active venv
```

### Rust

Build the example with the local x402-server:

```
cargo build --config 'patch.crates-io.tiders-x402="../../server"'
```

**Persistent local development**

For persistent local development, you can put this in `examples/rust/Cargo.toml`:

```toml
[patch.crates-io]
tiders-x402 = { path = "../../server" }
```

This avoids passing `--config` on every build command.