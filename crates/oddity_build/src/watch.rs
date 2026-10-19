//! Watching the game's code (`roadmap/scripting.md`, "Build and describe"):
//! when a file under `src/` (or `Cargo.toml`) changes, wait until nothing
//! has changed for half a second, then build.
//!
//! The folder is looked at a few times a second rather than through the
//! OS's file events: a game's `src/` is small, and this needs nothing more
//! than the standard library on every OS.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

/// How long the code must stay unchanged before a build starts.
pub const SETTLE: Duration = Duration::from_millis(500);

/// How often the files are looked at.
const SCAN_EVERY: Duration = Duration::from_millis(200);

/// Each watched file's path, modification time and size, sorted.
type Snapshot = Vec<(PathBuf, Option<SystemTime>, u64)>;

/// Watches a game's code for changes.
pub struct Watcher {
    root: PathBuf,
    seen: Snapshot,
    next_scan: Instant,
    /// When the last change was seen, while waiting for it to settle.
    changed_at: Option<Instant>,
}

impl Watcher {
    /// Watch the game in the folder `root`. What is there now counts as
    /// built.
    pub fn new(root: impl Into<PathBuf>, now: Instant) -> Self {
        let root = root.into();
        let seen = snapshot(&root);
        Self {
            root,
            seen,
            next_scan: now,
            changed_at: None,
        }
    }

    /// Whether it's time to build: something changed, and nothing has for
    /// [`SETTLE`]. True once per burst of changes.
    pub fn poll(&mut self, now: Instant) -> bool {
        if now >= self.next_scan {
            self.next_scan = now + SCAN_EVERY;
            let current = snapshot(&self.root);
            if current != self.seen {
                self.seen = current;
                self.changed_at = Some(now);
            }
        }
        match self.changed_at {
            Some(at) if now.duration_since(at) >= SETTLE => {
                self.changed_at = None;
                true
            }
            _ => false,
        }
    }
}

fn snapshot(root: &Path) -> Snapshot {
    let mut files = Vec::new();
    add(&root.join("Cargo.toml"), &mut files);
    walk(&root.join("src"), &mut files);
    files.sort();
    files
}

fn add(path: &Path, files: &mut Snapshot) {
    if let Ok(meta) = fs::metadata(path) {
        files.push((path.to_path_buf(), meta.modified().ok(), meta.len()));
    }
}

/// Every file under `dir`. Symbolic links aren't followed, so a link back
/// up the tree can't loop.
fn walk(dir: &Path, files: &mut Snapshot) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            walk(&entry.path(), files);
        } else if kind.is_file() {
            add(&entry.path(), files);
        }
    }
}
