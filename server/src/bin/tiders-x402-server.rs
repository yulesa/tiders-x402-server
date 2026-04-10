//! Entrypoint for the `tiders-x402-server` binary.
//!
//! All the logic lives in [`tiders_x402_server::cli`]; this file is just a
//! thin shim so the crate has a `[[bin]]` target. Gated behind the `cli`
//! feature so that library-only builds skip the CLI dependencies entirely.

#![cfg(feature = "cli")]

use std::process::ExitCode;

fn main() -> ExitCode {
    tiders_x402_server::cli::run()
}
