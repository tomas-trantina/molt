//! Molt - a local-first terminal UI and CLI for managing Python virtual
//! environments and running code inside them.
//!
//! The binary is intentionally thin: all behaviour lives in the `molt` library
//! crate. It delegates to [`molt::cli::run`], which either dispatches a
//! non-interactive subcommand or launches the interactive TUI.

use std::process::ExitCode;

use molt::cli;
use molt::error;

fn main() -> ExitCode {
    match cli::run() {
        Ok(code) => ExitCode::from(code as u8),
        Err(err) => {
            // `{:#}` prints the full anyhow context chain.
            eprintln!("molt: error: {err:#}");
            ExitCode::from(error::exit::GENERIC as u8)
        }
    }
}
