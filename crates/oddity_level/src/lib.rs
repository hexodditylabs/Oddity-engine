//! Level files (`roadmap/scripting.md`, "The level format"): pretty-printed
//! JSON, written from a World with [`save_level`] and read back into one
//! with [`spawn_level`], through reflection.
//!
//! ```ignore
//! let level = save_level(world, root)?;          // the level under `root`
//! std::fs::write(path, level.to_json())?;
//! let root = spawn_level(world, &Level::read(path)?)?;
//! ```
//!
//! - Nodes are listed parents first, siblings in order. Their `id`s only
//!   mean something inside the file: a `NodeRef` (or any `Entity` field) is
//!   saved as its target's `id`, and `null` when the target isn't in the
//!   level. A node keeps its `id` from save to save ([`NodeId`]); new ones
//!   get the next free number.
//! - Scripts are stored as `script:<id>` (or by their Rust path when they
//!   have no `id`), with only their saved fields. Other components are
//!   stored by their Rust path, whole.
//! - A component that can't be read (its type is gone, or the game doesn't
//!   have it, as in the editor) is kept as written, in [`UnknownData`], and
//!   saved again unchanged, so opening and saving never loses data.
//! - Prefab files (`.prefab`) have the same shape. A level's prefab
//!   instance stores only its prefab and its [`Override`]s (see `prefabs`).

mod entities;
mod file;
mod load;
mod prefabs;
mod save;
mod shapes;

pub use file::{FORMAT, Level, LevelNode, Override, PREFAB_FORMAT, PrefabRef, VERSION};
pub use load::spawn_level;
pub use prefabs::{FromPrefab, LevelPlugin, PrefabInstance, PrefabLibrary, instantiate};
pub use save::{save_level, save_prefab};
pub use shapes::{MeshShape, Shape};

use std::path::PathBuf;

use bevy::prelude::*;
use serde_json::{Map, Value};

/// A node's type in the editor (`"Node3D"`, `"Camera3D"`): the level file's
/// `kind`. The editor picks its icon from it; the game doesn't use it.
#[derive(Component, Reflect, Debug, Clone, PartialEq, Eq)]
#[reflect(Component)]
pub struct NodeKind(pub String);

/// A node's `id` in the file it was loaded from. Saving keeps it, so the
/// references the loader couldn't read (kept as written) and overrides stay
/// right, and diffs of level files stay small.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u64);

/// What a node's file entry held that couldn't be read into the World:
/// components whose type isn't known here, and keys from newer files. Saved
/// again exactly as read.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct UnknownData {
    /// Components by their key in the file, with their values.
    pub components: Map<String, Value>,
    /// The node's other keys.
    pub keys: Map<String, Value>,
}

/// Something went wrong reading or writing a level.
#[derive(Debug, thiserror::Error)]
pub enum LevelError {
    /// Reading the file failed.
    #[error("{path}: {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The file isn't JSON in the level format.
    #[error("not a valid level file: {0}")]
    Parse(String),
    /// The file parsed, but what it says doesn't make a level.
    #[error("invalid level: {0}")]
    Invalid(String),
    /// A component couldn't be written.
    #[error("{node}: can't save {component}: {message}")]
    Component {
        /// The node's name.
        node: String,
        /// The component's key.
        component: String,
        /// Why.
        message: String,
    },
}
