//! The editor's side of building a game (`roadmap/scripting.md`, "Build and
//! describe"):
//!
//! 1. [`Watcher`] sees a file under `src/` change, and waits half a second
//!    for more.
//! 2. [`Build`] runs cargo on the project's runner with JSON messages, and
//!    reports [`Diagnostic`]s as they come.
//! 3. After a good build it runs the game with `--oddity-describe`, and
//!    writes what it prints to `.oddity/types.json`.
//!
//! The editor never runs game code otherwise: that one call prints JSON and
//! exits.

mod build;
mod messages;
mod watch;

use std::path::PathBuf;

pub use build::{Build, BuildEvent, BuildRequest};
pub use messages::{CargoMessage, Diagnostic, Level, Location, parse_line, strip_ansi};
use oddity_project::Project;
pub use watch::{SETTLE, Watcher};

/// Where a project's runner and type description live.
pub trait ProjectBuildPaths {
    /// The runner's `Cargo.toml`: what the editor builds.
    fn runner_manifest(&self) -> PathBuf;
    /// `.oddity/types.json`: the game's type description.
    fn types_file(&self) -> PathBuf;
}

impl ProjectBuildPaths for Project {
    fn runner_manifest(&self) -> PathBuf {
        self.root.join(".oddity/runner/Cargo.toml")
    }

    fn types_file(&self) -> PathBuf {
        self.root.join(".oddity/types.json")
    }
}
