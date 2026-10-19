//! Add, remove and edit components through reflection, by type path.
//!
//! This is what the inspector, the CLI and MCP use, so any component that
//! derives `Reflect` with `#[reflect(Component, Default)]` is editable.

use std::{
    any::{Any, TypeId},
    borrow::Cow,
};

use bevy_ecs::{
    entity::Entity,
    reflect::{AppTypeRegistry, ReflectComponent},
    world::World,
};
use bevy_reflect::{GetPath, PartialReflect, Reflect, prelude::ReflectDefault};

use serde_json::{Value, json};

use crate::{
    CommandError, EditorCommand, Invocation, LevelRoot, level::require_in_level, reflect_json,
};

fn registry(world: &World) -> AppTypeRegistry {
    world.resource::<AppTypeRegistry>().clone()
}

/// The `ReflectComponent` for a type path, or a clear error.
pub(crate) fn reflect_component(
    registry: &bevy_reflect::TypeRegistry,
    type_path: &str,
) -> Result<ReflectComponent, CommandError> {
    let registration = registry
        .get_with_type_path(type_path)
        .or_else(|| registry.get_with_short_type_path(type_path))
        .ok_or_else(|| CommandError::UnknownComponent(type_path.to_owned()))?;
    registration
        .data::<ReflectComponent>()
        .cloned()
        .ok_or_else(|| CommandError::NotReflectable(type_path.to_owned()))
}

/// Adding or removing [`LevelRoot`] would leave the level with no root or
/// with two, so only opening and closing levels may do it.
fn forbid_level_root(
    registry: &bevy_reflect::TypeRegistry,
    type_path: &str,
) -> Result<(), CommandError> {
    let is_root = registry
        .get_with_type_path(type_path)
        .or_else(|| registry.get_with_short_type_path(type_path))
        .is_some_and(|registration| registration.type_id() == TypeId::of::<LevelRoot>());
    if is_root {
        return Err(CommandError::Invalid(
            "the level root marker cannot be added or removed".into(),
        ));
    }
    Ok(())
}

/// Set one field of a component, e.g. `translation.x` of a `Transform`.
/// An empty path replaces the whole component's value.
pub struct SetField {
    entity: Entity,
    component: String,
    path: String,
    value: Box<dyn PartialReflect>,
    old: Option<Box<dyn PartialReflect>>,
    /// The new value as JSON, for [`EditorCommand::invocation`]. Known from
    /// the start when built from JSON, otherwise read back after the first
    /// apply, when the field has its real type.
    json: Option<Value>,
}

impl SetField {
    /// Set `path` in `component` (a type path such as
    /// `bevy_transform::components::transform::Transform`) to `value`.
    pub fn new(
        entity: Entity,
        component: impl Into<String>,
        path: impl Into<String>,
        value: Box<dyn PartialReflect>,
    ) -> Self {
        Self {
            entity,
            component: component.into(),
            path: path.into(),
            value,
            old: None,
            json: None,
        }
    }

    /// Like [`SetField::new`], with the value given as JSON. The JSON is read
    /// using the field's own type, e.g. `{"x": 1, "y": 2, "z": 0}` for a `Vec3`.
    pub fn from_json(
        world: &World,
        entity: Entity,
        component: impl Into<String>,
        path: impl Into<String>,
        value: serde_json::Value,
    ) -> Result<Self, CommandError> {
        let component = component.into();
        let path = path.into();
        let reflected =
            reflect_json::field_from_json(world, entity, &component, &path, value.clone())?;
        let mut command = Self::new(entity, component, path, reflected);
        command.json = Some(value);
        Ok(command)
    }

    fn write(
        &mut self,
        world: &mut World,
        record_old: bool,
        use_old: bool,
    ) -> Result<(), CommandError> {
        let registry = registry(world);
        let registry = registry.read();
        let reflect_component = reflect_component(&registry, &self.component)?;
        let mut entity = world
            .get_entity_mut(self.entity)
            .map_err(|_| CommandError::NoEntity(self.entity))?;
        let mut component = reflect_component.reflect_mut(&mut entity).ok_or_else(|| {
            CommandError::MissingComponent {
                entity: self.entity,
                component: self.component.clone(),
            }
        })?;

        let field: &mut dyn PartialReflect = if self.path.is_empty() {
            component.as_partial_reflect_mut()
        } else {
            component
                .reflect_path_mut(self.path.as_str())
                .map_err(|e| CommandError::BadPath {
                    path: self.path.clone(),
                    reason: e.to_string(),
                })?
        };

        if record_old {
            let old = field
                .to_dynamic()
                .map_err(|e| CommandError::BadValue(e.to_string()))?;
            self.old = Some(old);
        }
        let value = if use_old {
            self.old.as_deref().ok_or(CommandError::NothingToUndo)?
        } else {
            self.value.as_ref()
        };
        field
            .try_apply(value)
            .map_err(|e| CommandError::BadValue(e.to_string()))
    }
}

impl EditorCommand for SetField {
    fn label(&self) -> Cow<'_, str> {
        let short = self
            .component
            .rsplit("::")
            .next()
            .unwrap_or(&self.component);
        if self.path.is_empty() {
            format!("Set {short}").into()
        } else {
            format!("Set {short}.{}", self.path).into()
        }
    }

    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        require_in_level(world, self.entity)?;
        let first_time = self.old.is_none();
        self.write(world, first_time, false)?;
        if self.json.is_none() {
            self.json =
                reflect_json::field_json(world, self.entity, &self.component, &self.path).ok();
        }
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        self.write(world, false, true)
    }

    /// Another edit of the same field takes over the value; this step keeps
    /// the value from before the first, so undo goes all the way back.
    fn merge(&mut self, next: &dyn EditorCommand) -> bool {
        let Some(next) = (next as &dyn Any).downcast_ref::<SetField>() else {
            return false;
        };
        if (next.entity, &next.component, &next.path) != (self.entity, &self.component, &self.path)
        {
            return false;
        }
        let Ok(value) = next.value.to_dynamic() else {
            return false;
        };
        self.value = value;
        self.json.clone_from(&next.json);
        true
    }

    fn invocation(&self) -> Invocation {
        Invocation::new(
            "component.set",
            json!({
                "entity": self.entity.to_bits(),
                "component": self.component,
                "path": self.path,
                "value": self.json,
            }),
        )
    }
}

/// Add a component with its default value (Godot's "Add Node" for one part).
pub struct AddComponent {
    entity: Entity,
    component: String,
}

impl AddComponent {
    /// Prepare to add `component` (a type path) to `entity`.
    pub fn new(entity: Entity, component: impl Into<String>) -> Self {
        Self {
            entity,
            component: component.into(),
        }
    }
}

impl EditorCommand for AddComponent {
    fn label(&self) -> Cow<'_, str> {
        let short = self
            .component
            .rsplit("::")
            .next()
            .unwrap_or(&self.component);
        format!("Add {short}").into()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        require_in_level(world, self.entity)?;
        let registry = registry(world);
        let registry = registry.read();
        let reflect_component = reflect_component(&registry, &self.component)?;
        forbid_level_root(&registry, &self.component)?;
        let registration = registry
            .get_with_type_path(&self.component)
            .or_else(|| registry.get_with_short_type_path(&self.component))
            .ok_or_else(|| CommandError::UnknownComponent(self.component.clone()))?;
        let default = registration
            .data::<ReflectDefault>()
            .ok_or_else(|| CommandError::NotReflectable(self.component.clone()))?
            .default();

        let mut entity = world
            .get_entity_mut(self.entity)
            .map_err(|_| CommandError::NoEntity(self.entity))?;
        if reflect_component.contains(&entity) {
            return Err(CommandError::AlreadyHasComponent {
                entity: self.entity,
                component: self.component.clone(),
            });
        }
        reflect_component.insert(&mut entity, default.as_partial_reflect(), &registry);
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        let registry = registry(world);
        let registry = registry.read();
        let reflect_component = reflect_component(&registry, &self.component)?;
        let mut entity = world
            .get_entity_mut(self.entity)
            .map_err(|_| CommandError::NoEntity(self.entity))?;
        reflect_component.remove(&mut entity);
        Ok(())
    }

    fn invocation(&self) -> Invocation {
        Invocation::new(
            "component.add",
            json!({ "entity": self.entity.to_bits(), "component": self.component }),
        )
    }
}

/// Remove a component. Undo puts back the exact value it had.
pub struct RemoveComponent {
    entity: Entity,
    component: String,
    removed: Option<Box<dyn Reflect>>,
}

impl RemoveComponent {
    /// Prepare to remove `component` (a type path) from `entity`.
    pub fn new(entity: Entity, component: impl Into<String>) -> Self {
        Self {
            entity,
            component: component.into(),
            removed: None,
        }
    }
}

impl EditorCommand for RemoveComponent {
    fn label(&self) -> Cow<'_, str> {
        let short = self
            .component
            .rsplit("::")
            .next()
            .unwrap_or(&self.component);
        format!("Remove {short}").into()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        require_in_level(world, self.entity)?;
        let registry = registry(world);
        let registry = registry.read();
        let reflect_component = reflect_component(&registry, &self.component)?;
        forbid_level_root(&registry, &self.component)?;
        let mut entity = world
            .get_entity_mut(self.entity)
            .map_err(|_| CommandError::NoEntity(self.entity))?;
        let removed =
            reflect_component
                .take(&mut entity)
                .ok_or_else(|| CommandError::MissingComponent {
                    entity: self.entity,
                    component: self.component.clone(),
                })?;
        self.removed = Some(removed);
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        let registry = registry(world);
        let registry = registry.read();
        let reflect_component = reflect_component(&registry, &self.component)?;
        let removed = self.removed.take().ok_or(CommandError::NothingToUndo)?;
        let mut entity = world
            .get_entity_mut(self.entity)
            .map_err(|_| CommandError::NoEntity(self.entity))?;
        reflect_component.insert(&mut entity, removed.as_partial_reflect(), &registry);
        Ok(())
    }

    fn invocation(&self) -> Invocation {
        Invocation::new(
            "component.remove",
            json!({ "entity": self.entity.to_bits(), "component": self.component }),
        )
    }
}
