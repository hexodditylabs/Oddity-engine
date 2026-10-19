//! Reading and writing component values as JSON, through reflection.
//! The CLI and MCP see entities this way.

use bevy_ecs::{
    entity::Entity,
    reflect::{AppTypeRegistry, ReflectComponent},
    world::World,
};
use bevy_reflect::{
    GetPath, PartialReflect, ReflectRef,
    serde::{TypedReflectDeserializer, TypedReflectSerializer},
};
use serde::de::DeserializeSeed;
use serde_json::Value;

use crate::{CommandError, commands::reflect_component};

/// Type paths of every reflectable component on an entity, sorted.
pub fn components_of(world: &World, entity: Entity) -> Result<Vec<String>, CommandError> {
    let registry = world.resource::<AppTypeRegistry>().read();
    let infos = world
        .inspect_entity(entity)
        .map_err(|_| CommandError::NoEntity(entity))?;
    let mut paths: Vec<String> = infos
        .filter_map(|(_, info)| info.type_id())
        .filter_map(|id| registry.get(id))
        .filter(|reg| reg.data::<ReflectComponent>().is_some())
        .map(|reg| reg.type_info().type_path().to_owned())
        .collect();
    paths.sort();
    Ok(paths)
}

/// A component's value as JSON.
pub fn component_json(
    world: &World,
    entity: Entity,
    component: &str,
) -> Result<Value, CommandError> {
    field_json(world, entity, component, "")
}

/// One field of a component as JSON (empty `path` = the whole component).
pub fn field_json(
    world: &World,
    entity: Entity,
    component: &str,
    path: &str,
) -> Result<Value, CommandError> {
    let registry = world.resource::<AppTypeRegistry>().read();
    let reflect_component = reflect_component(&registry, component)?;
    let entity_ref = world
        .get_entity(entity)
        .map_err(|_| CommandError::NoEntity(entity))?;
    let value =
        reflect_component
            .reflect(entity_ref)
            .ok_or_else(|| CommandError::MissingComponent {
                entity,
                component: component.to_owned(),
            })?;
    let field: &dyn PartialReflect = if path.is_empty() {
        value.as_partial_reflect()
    } else {
        value
            .reflect_path(path)
            .map_err(|e| CommandError::BadPath {
                path: path.to_owned(),
                reason: e.to_string(),
            })?
    };
    serde_json::to_value(TypedReflectSerializer::new(field, &registry))
        .map_err(|e| CommandError::BadValue(e.to_string()))
}

/// One top-level field of a component, for the Inspector.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldInfo {
    /// The field's name, and its path for `component.set`. Empty for a
    /// component that isn't a struct with named fields: the whole value.
    pub name: String,
    /// The field's Rust type path, e.g. `glam::Vec3`: what picks the
    /// Inspector field that draws it.
    pub type_path: String,
    /// Its value as JSON.
    pub value: Value,
}

/// A component's top-level fields, in declaration order. A component that
/// isn't a struct with named fields (`Visibility`, a newtype) comes back as
/// one unnamed field holding the whole value.
pub fn fields_of(
    world: &World,
    entity: Entity,
    component: &str,
) -> Result<Vec<FieldInfo>, CommandError> {
    let registry = world.resource::<AppTypeRegistry>().read();
    let reflect_component = reflect_component(&registry, component)?;
    let entity_ref = world
        .get_entity(entity)
        .map_err(|_| CommandError::NoEntity(entity))?;
    let value =
        reflect_component
            .reflect(entity_ref)
            .ok_or_else(|| CommandError::MissingComponent {
                entity,
                component: component.to_owned(),
            })?;
    let json = |field: &dyn PartialReflect| {
        serde_json::to_value(TypedReflectSerializer::new(field, &registry))
            .map_err(|e| CommandError::BadValue(e.to_string()))
    };
    match value.reflect_ref() {
        ReflectRef::Struct(fields) => (0..fields.field_len())
            .filter_map(|index| Some((fields.name_at(index)?, fields.field_at(index)?)))
            .map(|(name, field)| {
                Ok(FieldInfo {
                    name: name.to_owned(),
                    type_path: field.reflect_type_path().to_owned(),
                    value: json(field)?,
                })
            })
            .collect(),
        _ => Ok(vec![FieldInfo {
            name: String::new(),
            type_path: value.reflect_type_path().to_owned(),
            value: json(value.as_partial_reflect())?,
        }]),
    }
}

/// Read JSON as a value of the type the field currently has.
pub(crate) fn field_from_json(
    world: &World,
    entity: Entity,
    component: &str,
    path: &str,
    json: Value,
) -> Result<Box<dyn PartialReflect>, CommandError> {
    let registry = world.resource::<AppTypeRegistry>().read();
    let reflect_component = reflect_component(&registry, component)?;
    let entity_ref = world
        .get_entity(entity)
        .map_err(|_| CommandError::NoEntity(entity))?;
    let value =
        reflect_component
            .reflect(entity_ref)
            .ok_or_else(|| CommandError::MissingComponent {
                entity,
                component: component.to_owned(),
            })?;
    let field: &dyn PartialReflect = if path.is_empty() {
        value.as_partial_reflect()
    } else {
        value
            .reflect_path(path)
            .map_err(|e| CommandError::BadPath {
                path: path.to_owned(),
                reason: e.to_string(),
            })?
    };
    let type_id = field
        .get_represented_type_info()
        .map(|info| info.type_id())
        .ok_or_else(|| CommandError::BadValue("the field's type is not known".into()))?;
    let registration = registry
        .get(type_id)
        .ok_or_else(|| CommandError::BadValue("the field's type is not registered".into()))?;
    TypedReflectDeserializer::new(registration, &registry)
        .deserialize(json)
        .map_err(|e| CommandError::BadValue(e.to_string()))
}
