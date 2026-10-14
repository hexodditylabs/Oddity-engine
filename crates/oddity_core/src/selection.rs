//! What is selected in the editor.

use bevy_ecs::{entity::Entity, resource::Resource};

/// The selected entities, in the order they were selected.
/// The last one is the primary selection (shown in the inspector).
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    entities: Vec<Entity>,
}

impl Selection {
    /// Selected entities, oldest first.
    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }

    /// The entity the inspector shows.
    pub fn primary(&self) -> Option<Entity> {
        self.entities.last().copied()
    }

    /// Whether an entity is selected.
    pub fn contains(&self, entity: Entity) -> bool {
        self.entities.contains(&entity)
    }

    /// Whether nothing is selected.
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Replace the selection.
    pub fn set(&mut self, entities: impl IntoIterator<Item = Entity>) {
        self.entities.clear();
        for entity in entities {
            self.add(entity);
        }
    }

    /// Select only this entity (a plain click).
    pub fn select(&mut self, entity: Entity) {
        self.set([entity]);
    }

    /// Add to the selection, making it primary (Shift+click).
    pub fn add(&mut self, entity: Entity) {
        self.entities.retain(|e| *e != entity);
        self.entities.push(entity);
    }

    /// Add or remove one entity (Ctrl+click).
    pub fn toggle(&mut self, entity: Entity) {
        if self.contains(entity) {
            self.remove(entity);
        } else {
            self.add(entity);
        }
    }

    /// Deselect one entity.
    pub fn remove(&mut self, entity: Entity) {
        self.entities.retain(|e| *e != entity);
    }

    /// Deselect everything.
    pub fn clear(&mut self) {
        self.entities.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_is_the_last_selected() {
        let a = Entity::from_raw_u32(1).unwrap();
        let b = Entity::from_raw_u32(2).unwrap();
        let mut selection = Selection::default();
        selection.add(a);
        selection.add(b);
        assert_eq!(selection.primary(), Some(b));
        selection.add(a);
        assert_eq!(selection.entities(), &[b, a]);
        selection.toggle(a);
        assert_eq!(selection.entities(), &[b]);
        selection.select(a);
        assert_eq!(selection.entities(), &[a]);
    }
}
