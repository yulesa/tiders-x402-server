# tiders-x402-server (CLI)

Command-line interface for [tiders-x402-server](https://github.com/yulesa/tiders-x402-server) — a payment-enabled database API server that combines analytical databases with the [x402 payment protocol](https://www.x402.org/), enabling pay-per-query data access using cryptocurrency micropayments.

This package installs a single binary, `tiders-x402-server`, which runs the server from a YAML config file with no Python or Rust code required. All database backends (DuckDB, PostgreSQL, ClickHouse) are bundled.

Full documentation: <https://yulesa.github.io/tiders-x402-server/>

## Installation

```bash
pip install tiders-x402-server
# or
cargo install tiders-x402-server
```

## Usage

```bash
tiders-x402-server start [CONFIG]       # start the server (hot-reloads config by default)
tiders-x402-server start --no-watch     # disable config hot-reload
tiders-x402-server validate [CONFIG]    # validate config and test DB connectivity, then exit
tiders-x402-server --help
```

If `CONFIG` is omitted, the CLI auto-discovers a single `.yaml`/`.yml` file in the current directory that contains `server`, `facilitator`, and `database` keys. A `.env` file in the current directory (or a parent) is loaded automatically; override with `--env-file PATH`.

## Related packages

- [`tiders-x402-server-sdk`](https://pypi.org/project/tiders-x402-server-sdk/) — Python SDK for embedding the server in your own code (`import tiders_x402_server`).
- [`tiders-x402-server`](https://crates.io/crates/tiders-x402-server) — Rust library and binary on crates.io.

## License

Licensed under either of [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT license](https://opensource.org/licenses/MIT) at your option.
