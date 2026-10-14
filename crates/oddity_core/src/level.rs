//! The level being edited.
//!
//! Editor UI and the edited level live in the same Bevy `World`. Everything
//! under the entity marked [`LevelRoot`] is the level: what the Hierarchy
//! shows, what gets saved to a `.level` file, and what Play runs. UI entities
//! are never under it, so they can't be selected, saved or deleted by accident.
//!
//! Prefabs opened in isolation (Phase 3) get their own root the same way.

use bevy_ecs::{
    component::Component, entity::Entity, query::With, reflect::ReflectComponent, world::World,
};
use bevy_reflect::{Reflect, prelude::ReflectDefault};

/// Marks the top node of the level being edited. There is at most one.
#[derive(Component, Reflect, Default, Debug, Clone, Copy, PartialEq, Eq)]
#[reflect(Component, Default)]
pub struct LevelRoot;

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
