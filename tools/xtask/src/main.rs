//! Repository checks that cargo itself can't express.
//!
//! - `cargo xtask layers`: every workspace crate declares a layer, and every
//!   dependency between workspace crates follows the layer rules in
//!   `roadmap/modularity.md`. CI runs it next to fmt, clippy and tests.

mod layers;

use std::{env, process::ExitCode};

fn main() -> ExitCode {
    match env::args().nth(1).as_deref() {
        Some("layers") => layers::run(),
        _ => {
            eprintln!("usage: cargo xtask layers");
            ExitCode::FAILURE
        }
    }
}
