//! Entities in component values, as file `id`s: a `NodeRef` (whose inside
//! is an `Option<Entity>`) or a plain `Entity` field is written as its
//! target's `id`, and read back as the entity spawned for that `id`.

use std::any::TypeId;

use bevy::{
    platform::collections::HashMap,
    prelude::*,
    reflect::{
        PartialReflect, TypeRegistration, TypeRegistry,
        serde::{ReflectDeserializerProcessor, ReflectSerializerProcessor},
    },
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Writes entities as their `id` in the file; `null` for one outside it.
pub(crate) struct EntityToId<'a>(pub &'a HashMap<Entity, u64>);

impl ReflectSerializerProcessor for EntityToId<'_> {
    fn try_serialize<S: Serializer>(
        &self,
        value: &dyn PartialReflect,
        _registry: &TypeRegistry,
        serializer: S,
    ) -> Result<Result<S::Ok, S>, S::Error> {
        match value.try_downcast_ref::<Entity>() {
            Some(entity) => self.0.get(entity).serialize(serializer).map(Ok),
            None => Ok(Err(serializer)),
        }
    }
}

/// Reads `id`s back as the entities spawned for them. An `id` that isn't in
/// the file, or `null` where an `Entity` must be, becomes
/// `Entity::PLACEHOLDER`.
pub(crate) struct IdToEntity<'a>(pub &'a HashMap<u64, Entity>);

impl ReflectDeserializerProcessor for IdToEntity<'_> {
    fn try_deserialize<'de, D: Deserializer<'de>>(
        &mut self,
        registration: &TypeRegistration,
        _registry: &TypeRegistry,
        deserializer: D,
    ) -> Result<Result<Box<dyn PartialReflect>, D>, D::Error> {
        if registration.type_id() != TypeId::of::<Entity>() {
            return Ok(Err(deserializer));
        }
        let id = Option::<u64>::deserialize(deserializer)?;
        let entity = id
            .and_then(|id| self.0.get(&id).copied())
            .unwrap_or(Entity::PLACEHOLDER);
        Ok(Ok(Box::new(entity)))
    }
}
