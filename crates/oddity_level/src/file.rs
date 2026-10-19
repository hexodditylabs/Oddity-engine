//! The level file itself, as data.

use std::{fs, path::Path};

use oddity_script::Prefab;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::LevelError;

/// The `format` of a level file.
pub const FORMAT: &str = "oddity-level";

/// The `format` of a prefab file. Its nodes are written as a level's are.
pub const PREFAB_FORMAT: &str = "oddity-prefab";

/// The newest `version` of the format this crate reads and writes.
pub const VERSION: u32 = 1;

/// A level file, or a prefab file (which has the same shape).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Level {
    /// [`FORMAT`], or [`PREFAB_FORMAT`] for a prefab.
    pub format: String,
    /// The format version, [`VERSION`] or older.
    pub version: u32,
    /// Parents first, siblings in order; exactly one has no parent.
    pub nodes: Vec<LevelNode>,
}

/// One node of a level.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LevelNode {
    /// Unique in the file; what `NodeRef`s and `parent` refer to.
    pub id: u64,
    /// Its name in the Hierarchy.
    pub name: String,
    /// The parent's `id`; `null` for the level's root.
    pub parent: Option<u64>,
    /// Its node type in the editor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Components by key: `script:<id>` or a Rust type path.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub components: Map<String, Value>,
    /// For a prefab instance: the prefab. The node's nodes and components
    /// then come from the prefab file, with [`overrides`](Self::overrides).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefab: Option<PrefabRef>,
    /// A prefab instance's values that differ from the prefab's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<Override>,
    /// Keys this version doesn't read, kept as they are.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

/// Which prefab an instance is of: its asset ID and, as a hint, its path.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PrefabRef {
    /// The prefab file's asset ID.
    pub asset: Option<Uuid>,
    /// Its path relative to the project folder.
    pub path: String,
}

impl From<&Prefab> for PrefabRef {
    fn from(prefab: &Prefab) -> Self {
        Self {
            asset: prefab.asset,
            path: prefab.path.clone(),
        }
    }
}

impl From<&PrefabRef> for Prefab {
    fn from(prefab: &PrefabRef) -> Self {
        Self {
            asset: prefab.asset,
            path: prefab.path.clone(),
        }
    }
}

/// One value of a prefab instance that differs from the prefab.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Override {
    /// The node, by path inside the prefab: `.` is its root, `Body/Gun` a
    /// node below it.
    pub node: String,
    /// The component's key, as in `components`.
    pub component: String,
    /// The field, as a reflect path (`max`, `translation.y`, `stats.level`);
    /// empty for the whole component, which also adds one the prefab lacks.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub field: String,
    /// The value, written as the component would write it. A `NodeRef`
    /// in it is an `id` in the level, not in the prefab.
    pub value: Value,
}

impl Level {
    /// A level with no nodes, in the current format.
    pub fn new() -> Self {
        Self {
            format: FORMAT.to_owned(),
            version: VERSION,
            nodes: Vec::new(),
        }
    }

    /// A prefab with no nodes, in the current format.
    pub fn new_prefab() -> Self {
        Self {
            format: PREFAB_FORMAT.to_owned(),
            ..Self::new()
        }
    }

    /// Read a level from JSON text.
    pub fn from_json(text: &str) -> Result<Self, LevelError> {
        serde_json::from_str(text).map_err(|error| LevelError::Parse(error.to_string()))
    }

    /// The level as pretty-printed JSON, ending with a newline.
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self)
            .expect("a level is plain JSON values, which always serialize");
        text.push('\n');
        text
    }

    /// Read the level file at `path`.
    pub fn read(path: impl AsRef<Path>) -> Result<Self, LevelError> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|source| LevelError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_json(&text).map_err(|error| match error {
            LevelError::Parse(message) => {
                LevelError::Parse(format!("{}: {message}", path.display()))
            }
            other => other,
        })
    }
}

impl Default for Level {
    fn default() -> Self {
        Self::new()
    }
}
