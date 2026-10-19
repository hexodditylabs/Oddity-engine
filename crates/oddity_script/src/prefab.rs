//! Prefabs from scripts (`roadmap/scripting.md`, "Node references,
//! prefabs, assets"): the [`Prefab`] field type, and what
//! `ctx.instantiate` spawns it with.
//!
//! ```ignore
//! pub bullet: Prefab,                       // dragged in from the FileSystem
//! ctx.instantiate(&self.bullet, at);
//! ```
//!
//! Prefab files are read by `oddity_level`, which depends on this crate, so
//! it puts its spawner here as a resource ([`PrefabSpawner`]) instead.

use bevy::{asset::uuid::Uuid, prelude::*, reflect::std_traits::ReflectDefault};

/// A prefab file, for script fields: drag a `.prefab` from the FileSystem
/// onto the field in the Inspector. The default points at none.
#[derive(Reflect, Clone, Debug, Default, PartialEq, Eq, Hash)]
#[reflect(Default, Debug, PartialEq, Hash)]
pub struct Prefab {
    /// The file's asset ID, from its `.import` file.
    pub asset: Option<Uuid>,
    /// Where the file was when the reference was made, relative to the
    /// project folder: for people reading level files, and for finding it
    /// again if its `.import` file was lost.
    pub path: String,
}

impl Prefab {
    /// A prefab by path only, until it has an ID (in tests, say).
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            asset: None,
            path: path.into(),
        }
    }

    /// Whether it points at no prefab.
    pub fn is_none(&self) -> bool {
        self.asset.is_none() && self.path.is_empty()
    }
}

/// Spawns a prefab under `parent` with the root at `transform`, and
/// returns its root: what `ctx.instantiate` calls. Added by
/// `oddity_level`'s plugin.
#[derive(Resource, Clone, Copy)]
pub struct PrefabSpawner(
    pub fn(&mut World, &Prefab, Transform, Option<Entity>) -> Result<Entity, String>,
);
