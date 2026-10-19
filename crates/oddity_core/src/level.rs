//! The level being edited.
//!
//! Editor UI and the edited level live in the same Bevy `World`. Everything
//! under the entity marked [`LevelRoot`] is the level: what the Hierarchy
//! shows, what gets saved to a `.level` file, and what Play runs. UI entities
//! are never under it, so they can't be selected, saved or deleted by accident.
//!
//! Prefabs opened in isolation (Phase 3) get their own root the same way.

use bevy_ecs::{
    component::Component, entity::Entity, entity_disabling::Disabled, query::With,
    reflect::ReflectComponent, world::World,
};
use bevy_reflect::{Reflect, prelude::ReflectDefault};

use crate::CommandError;

/// Marks the top node of the level being edited. There is at most one.
#[derive(Component, Reflect, Default, Debug, Clone, Copy, PartialEq, Eq)]
#[reflect(Component, Default)]
pub struct LevelRoot;

/// The node type a node was created as, such as `"Camera3D"`: what the
/// level file records as its `kind`, and what picks its icon. A node without
/// one is a plain `Node`.
#[derive(Component, Reflect, Debug, Clone, PartialEq, Eq)]
#[reflect(Component)]
pub struct NodeKind(pub String);

/// The current level's root node, if a level is open.
pub fn level_root(world: &mut World) -> Option<Entity> {
    world
        .query_filtered::<Entity, With<LevelRoot>>()
        .iter(world)
        .next()
}

/// Whether `entity` is the level root or one of its descendants.
pub fn is_in_level(world: &World, entity: Entity) -> bool {
    let mut current = Some(entity);
    while let Some(e) = current {
        if world.get::<LevelRoot>(e).is_some() {
            return true;
        }
        current = world
            .get::<bevy_ecs::hierarchy::ChildOf>(e)
            .map(|child_of| child_of.parent());
    }
    false
}

/// Checks that `entity` is a live node of the open level. Commands change
/// only the level, never editor UI, and deleted nodes are only disabled
/// (waiting for undo), so they count as gone.
pub(crate) fn require_in_level(world: &World, entity: Entity) -> Result<(), CommandError> {
    let found = world
        .get_entity(entity)
        .map_err(|_| CommandError::NoEntity(entity))?;
    if found.contains::<Disabled>() {
        return Err(CommandError::NoEntity(entity));
    }
    if !is_in_level(world, entity) {
        return Err(CommandError::NotInLevel(entity));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use bevy_ecs::{hierarchy::ChildOf, name::Name};

    use super::*;

    #[test]
    fn finds_root_and_membership() {
        let mut world = World::new();
        let ui = world.spawn(Name::new("Some UI panel")).id();
        assert_eq!(level_root(&mut world), None);

        let root = world.spawn((Name::new("Main"), LevelRoot)).id();
        let child = world.spawn((Name::new("Player"), ChildOf(root))).id();
        let grandchild = world.spawn((Name::new("Mesh"), ChildOf(child))).id();

        assert_eq!(level_root(&mut world), Some(root));
        assert!(is_in_level(&world, root));
        assert!(is_in_level(&world, grandchild));
        assert!(!is_in_level(&world, ui));
    }
}
