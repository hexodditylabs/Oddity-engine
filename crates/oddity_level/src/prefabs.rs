//! Prefabs (`roadmap/scripting.md`, "The level format"): prefab files, the
//! instances levels place, and the plugin that lets scripts instantiate
//! them.
//!
//! An instance stores only its prefab and its overrides, so changing the
//! prefab file changes every instance that didn't override that value.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::{platform::collections::HashMap, prelude::*};
use oddity_project::{AssetId, AssetIndex};
use oddity_script::{Prefab, PrefabSpawner};

use crate::{Level, LevelError, Override, PREFAB_FORMAT, load::spawn_prefab};

/// On a prefab instance's root: which prefab it is, and its overrides,
/// which levels save instead of its nodes.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct PrefabInstance {
    /// The prefab.
    pub prefab: Prefab,
    /// Its values that differ from the prefab's.
    pub overrides: Vec<Override>,
}

/// On every node below an instance's root that came from its prefab:
/// levels don't save these.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FromPrefab;

/// The project's prefab files, read on first use and kept.
#[derive(Resource)]
pub struct PrefabLibrary {
    root: PathBuf,
    index: Option<AssetIndex>,
    files: HashMap<Prefab, Arc<Level>>,
}

impl PrefabLibrary {
    /// The prefabs of the project in the folder `root`, found by their
    /// asset IDs (read from its `.import` files) or, failing that, by path.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let index = match AssetIndex::scan(&root) {
            Ok(index) => Some(index),
            Err(error) => {
                warn!("prefabs will be found by path only: {error}");
                None
            }
        };
        Self {
            root,
            index,
            files: HashMap::default(),
        }
    }

    /// The prefab file `prefab` points at.
    pub fn get(&mut self, prefab: &Prefab) -> Result<Arc<Level>, LevelError> {
        if let Some(file) = self.files.get(prefab) {
            return Ok(file.clone());
        }
        let path = self.path_of(prefab)?;
        let file = Level::read(&path)?;
        if file.format != PREFAB_FORMAT {
            return Err(LevelError::Invalid(format!(
                "{} is a {:?} file, not a prefab",
                path.display(),
                file.format
            )));
        }
        let file = Arc::new(file);
        self.files.insert(prefab.clone(), file.clone());
        Ok(file)
    }

    /// Use `file` for `prefab` instead of reading one from disk.
    pub fn insert(&mut self, prefab: Prefab, file: Level) {
        self.files.insert(prefab, Arc::new(file));
    }

    fn path_of(&self, prefab: &Prefab) -> Result<PathBuf, LevelError> {
        let by_id = prefab
            .asset
            .and_then(|id| self.index.as_ref()?.path_of(AssetId::from(id)));
        match by_id.or((!prefab.path.is_empty()).then_some(prefab.path.as_str())) {
            Some(path) => Ok(self.root.join(Path::new(path))),
            None => Err(LevelError::Invalid("the prefab reference is empty".into())),
        }
    }
}

/// Lets scripts instantiate prefabs (`ctx.instantiate`), found in the
/// project in the folder `root`, and gives [`MeshShape`](crate::MeshShape)s
/// their meshes.
pub struct LevelPlugin {
    /// The project folder.
    pub root: PathBuf,
}

impl LevelPlugin {
    /// Prefabs come from the project in `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PrefabLibrary::new(self.root.clone()))
            .insert_resource(PrefabSpawner(instantiate))
            .add_systems(PostUpdate, crate::shapes::build_shapes);
    }
}

/// `ctx.instantiate`: the prefab's nodes under `parent`, the root at
/// `transform`.
pub fn instantiate(
    world: &mut World,
    prefab: &Prefab,
    transform: Transform,
    parent: Option<Entity>,
) -> Result<Entity, String> {
    let file = world
        .get_resource_mut::<PrefabLibrary>()
        .ok_or("no PrefabLibrary (add LevelPlugin)")?
        .get(prefab)
        .map_err(|error| error.to_string())?;
    let root = spawn_prefab(world, prefab, &file, parent, &[], &HashMap::default())
        .map_err(|error| error.to_string())?;
    world.entity_mut(root).insert(transform);
    Ok(root)
}
