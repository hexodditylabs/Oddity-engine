//! Spawn, delete, rename and reparent entities (nodes).

use std::borrow::Cow;

use bevy_ecs::{
    entity::Entity,
    entity_disabling::Disabled,
    hierarchy::{ChildOf, Children},
    name::Name,
    world::World,
};
use serde_json::{Value, json};

use crate::{
    CommandError, EditorCommand, Invocation, LevelRoot, Selection,
    level::{level_root, require_in_level},
};

fn require(world: &World, entity: Entity) -> Result<(), CommandError> {
    world
        .get_entity(entity)
        .map(|_| ())
        .map_err(|_| CommandError::NoEntity(entity))
}

fn name_of(world: &World, entity: Entity) -> String {
    world
        .get::<Name>(entity)
        .map_or_else(|| format!("{entity}"), |n| n.as_str().to_owned())
}

/// The entity and all its descendants, parents first. Includes disabled ones.
pub(crate) fn subtree(world: &World, root: Entity) -> Vec<Entity> {
    let mut out = vec![root];
    let mut i = 0;
    while i < out.len() {
        if let Some(children) = world.get::<Children>(out[i]) {
            out.extend(children.iter());
        }
        i += 1;
    }
    out
}

/// Create a new named node in the open level: under a parent, or at the
/// top of the level.
///
/// The entity is created (disabled) as soon as the command is built, so its
/// id is known before it runs and stays the same through undo and redo.
pub struct SpawnEntity {
    entity: Entity,
    name: String,
    parent: Option<Entity>,
}

impl SpawnEntity {
    /// Prepare to spawn an entity called `name`.
    pub fn new(world: &mut World, name: impl Into<String>) -> Self {
        let name = name.into();
        let entity = world.spawn((Name::new(name.clone()), Disabled)).id();
        Self {
            entity,
            name,
            parent: None,
        }
    }

    /// Spawn it as the last child of `parent` instead of the level root.
    pub fn child_of(mut self, parent: Entity) -> Self {
        self.parent = Some(parent);
        self
    }

    /// The entity this command spawns.
    pub fn entity(&self) -> Entity {
        self.entity
    }
}

impl EditorCommand for SpawnEntity {
    fn label(&self) -> Cow<'_, str> {
        format!("Add {}", self.name).into()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        let parent = match self.parent {
            Some(parent) => parent,
            None => level_root(world).ok_or(CommandError::NoLevel)?,
        };
        require_in_level(world, parent)?;
        let mut entity = world
            .get_entity_mut(self.entity)
            .map_err(|_| CommandError::NoEntity(self.entity))?;
        entity.remove::<Disabled>();
        entity.insert(ChildOf(parent));
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        let mut entity = world
            .get_entity_mut(self.entity)
            .map_err(|_| CommandError::NoEntity(self.entity))?;
        entity.insert(Disabled);
        if let Some(mut selection) = world.get_resource_mut::<Selection>() {
            selection.remove(self.entity);
        }
        Ok(())
    }

    fn invocation(&self) -> Invocation {
        let mut args = json!({ "name": self.name });
        if let Some(parent) = self.parent {
            args["parent"] = json!(parent.to_bits());
        }
        Invocation::new("entity.spawn", args)
    }

    fn output(&self) -> Value {
        json!({ "entity": self.entity.to_bits() })
    }

    fn discard(&mut self, world: &mut World, applied: bool) {
        if !applied && let Ok(entity) = world.get_entity_mut(self.entity) {
            entity.despawn();
        }
    }
}

/// Delete an entity and its children.
///
/// They are disabled rather than despawned, so undo brings back the very
/// same entities. They are despawned for real once the step leaves the history.
pub struct DespawnEntity {
    entity: Entity,
    label: String,
    disabled: Vec<Entity>,
}

impl DespawnEntity {
    /// Prepare to delete `entity` and its children.
    pub fn new(world: &World, entity: Entity) -> Self {
        Self {
            entity,
            label: format!("Delete {}", name_of(world, entity)),
            disabled: Vec::new(),
        }
    }
}

impl EditorCommand for DespawnEntity {
    fn label(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.label)
    }

    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        require_in_level(world, self.entity)?;
        if world.get::<LevelRoot>(self.entity).is_some() {
            return Err(CommandError::Invalid(
                "the level root cannot be deleted".into(),
            ));
        }
        self.disabled.clear();
        for entity in subtree(world, self.entity) {
            // Leave alone anything another command already disabled.
            if world.get::<Disabled>(entity).is_none() {
                world.entity_mut(entity).insert(Disabled);
                self.disabled.push(entity);
            }
        }
        if let Some(mut selection) = world.get_resource_mut::<Selection>() {
            for entity in &self.disabled {
                selection.remove(*entity);
            }
        }
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        for entity in &self.disabled {
            if let Ok(mut entity) = world.get_entity_mut(*entity) {
                entity.remove::<Disabled>();
            }
        }
        Ok(())
    }

    fn invocation(&self) -> Invocation {
        Invocation::new("entity.despawn", json!({ "entity": self.entity.to_bits() }))
    }

    fn discard(&mut self, world: &mut World, applied: bool) {
        if applied && let Ok(entity) = world.get_entity_mut(self.entity) {
            entity.despawn();
        }
    }
}

/// Change an entity's [`Name`].
pub struct RenameEntity {
    entity: Entity,
    name: String,
    old: Option<Option<String>>,
}

impl RenameEntity {
    /// Prepare to rename `entity` to `name`.
    pub fn new(entity: Entity, name: impl Into<String>) -> Self {
        Self {
            entity,
            name: name.into(),
            old: None,
        }
    }
}

impl EditorCommand for RenameEntity {
    fn label(&self) -> Cow<'_, str> {
        format!("Rename to {}", self.name).into()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(CommandError::BadValue("a name cannot be empty".into()));
        }
        require_in_level(world, self.entity)?;
        let mut entity = world
            .get_entity_mut(self.entity)
            .map_err(|_| CommandError::NoEntity(self.entity))?;
        if self.old.is_none() {
            self.old = Some(entity.get::<Name>().map(|n| n.as_str().to_owned()));
        }
        entity.insert(Name::new(name.to_owned()));
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        let mut entity = world
            .get_entity_mut(self.entity)
            .map_err(|_| CommandError::NoEntity(self.entity))?;
        match self.old.clone().flatten() {
            Some(old) => {
                entity.insert(Name::new(old));
            }
            None => {
                entity.remove::<Name>();
            }
        }
        Ok(())
    }

    fn invocation(&self) -> Invocation {
        Invocation::new(
            "entity.rename",
            json!({ "entity": self.entity.to_bits(), "name": self.name }),
        )
    }
}

/// Move a node under a new parent, or to the top of the level with `None`.
/// Undo puts it back at the same position among its old siblings.
pub struct ReparentEntity {
    entity: Entity,
    parent: Option<Entity>,
    /// Old parent and index among its children.
    old: Option<Option<(Entity, usize)>>,
}

impl ReparentEntity {
    /// Prepare to move `entity` under `parent` (`None` = the level root).
    pub fn new(entity: Entity, parent: Option<Entity>) -> Self {
        Self {
            entity,
            parent,
            old: None,
        }
    }
}

impl EditorCommand for ReparentEntity {
    fn label(&self) -> Cow<'_, str> {
        Cow::Borrowed("Reparent")
    }

    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        require_in_level(world, self.entity)?;
        if world.get::<LevelRoot>(self.entity).is_some() {
            return Err(CommandError::Invalid(
                "the level root cannot be moved".into(),
            ));
        }
        let parent = match self.parent {
            Some(parent) => parent,
            None => level_root(world).ok_or(CommandError::NoLevel)?,
        };
        require_in_level(world, parent)?;
        if subtree(world, self.entity).contains(&parent) {
            return Err(CommandError::Invalid(
                "a node cannot become a child of itself or its own children".into(),
            ));
        }
        if self.old.is_none() {
            self.old = Some(world.get::<ChildOf>(self.entity).map(|child_of| {
                let parent = child_of.parent();
                let index = world
                    .get::<Children>(parent)
                    .and_then(|c| c.iter().position(|e| *e == self.entity))
                    .unwrap_or(0);
                (parent, index)
            }));
        }
        world.entity_mut(parent).add_child(self.entity);
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        require(world, self.entity)?;
        match self.old.flatten() {
            Some((parent, index)) => {
                require(world, parent)?;
                world
                    .entity_mut(parent)
                    .insert_children(index, &[self.entity]);
            }
            None => {
                world.entity_mut(self.entity).remove::<ChildOf>();
            }
        }
        Ok(())
    }

    fn invocation(&self) -> Invocation {
        Invocation::new(
            "entity.reparent",
            json!({
                "entity": self.entity.to_bits(),
                "parent": self.parent.map(Entity::to_bits),
            }),
        )
    }
}
