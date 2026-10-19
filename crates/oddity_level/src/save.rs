//! Writing a level from a World.

use std::any::TypeId;

use bevy::{
    camera::visibility::{InheritedVisibility, ViewVisibility, VisibilityClass},
    ecs::{component::ComponentId, entity_disabling::Disabled},
    platform::collections::{HashMap, HashSet},
    prelude::*,
    reflect::{
        PartialReflect, ReflectRef, TypeRegistration, TypeRegistry, serde::TypedReflectSerializer,
        std_traits::ReflectDefault,
    },
    transform::components::TransformTreeChanged,
};
use oddity_script::{ReflectScript, attributes::Export};
use serde_json::{Map, Value};

use crate::{
    Level, LevelError, LevelNode, MeshShape, NodeId, NodeKind, PrefabRef, UnknownData,
    entities::EntityToId,
    prefabs::{FromPrefab, PrefabInstance},
};

/// Components a level never stores: the file's own fields (name,
/// hierarchy, kind), and what Bevy works out from other components.
fn not_saved() -> [TypeId; 10] {
    [
        TypeId::of::<Name>(),
        TypeId::of::<NodeId>(),
        TypeId::of::<ChildOf>(),
        TypeId::of::<Children>(),
        TypeId::of::<NodeKind>(),
        TypeId::of::<GlobalTransform>(),
        TypeId::of::<TransformTreeChanged>(),
        TypeId::of::<InheritedVisibility>(),
        TypeId::of::<ViewVisibility>(),
        TypeId::of::<VisibilityClass>(),
    ]
}

/// The level under `root`: `root` and everything below it, except deleted
/// (`Disabled`) nodes. A prefab instance is saved as its prefab and its
/// overrides; the nodes that came from its prefab aren't saved.
pub fn save_level(world: &World, root: Entity) -> Result<Level, LevelError> {
    save(world, root, Level::new())
}

/// The prefab under `root`, as [`save_level`] saves a level.
pub fn save_prefab(world: &World, root: Entity) -> Result<Level, LevelError> {
    let prefab = save(world, root, Level::new_prefab())?;
    if let Some(node) = prefab.nodes.iter().find(|node| node.prefab.is_some()) {
        return Err(LevelError::Invalid(format!(
            "{:?} is an instance of another prefab; nested prefabs come later",
            node.name
        )));
    }
    Ok(prefab)
}

fn save(world: &World, root: Entity, mut level: Level) -> Result<Level, LevelError> {
    let mut order = Vec::new();
    collect(world, root, &mut order);
    let ids = ids(world, &order);
    let registry = world.resource::<AppTypeRegistry>().read();
    let not_saved = not_saved();
    for &entity in &order {
        let instance = world.get::<PrefabInstance>(entity);
        let shaped = world.get::<MeshShape>(entity).is_some();
        let name = world
            .get::<Name>(entity)
            .map_or_else(String::new, |name| name.as_str().to_owned());
        let unknown = world.get::<UnknownData>(entity);
        let mut components = Vec::new();
        let entity_ref = world.entity(entity);
        // An instance's values come from its prefab and its overrides.
        let own: Vec<_> = world
            .inspect_entity(entity)
            .into_iter()
            .flatten()
            .filter(|_| instance.is_none())
            .collect();
        // What the node's components bring in by themselves (a camera brings
        // a dozen): saved only when changed, since loading brings them back.
        let required: HashSet<ComponentId> = own
            .iter()
            .flat_map(|(_, info)| info.required_components().iter_ids())
            .collect();
        for (id, info) in own {
            let Some(type_id) = info.type_id() else {
                continue;
            };
            if not_saved.contains(&type_id) {
                continue;
            }
            // A shape's mesh and material are made from it when it loads.
            if shaped
                && (type_id == TypeId::of::<Mesh3d>()
                    || type_id == TypeId::of::<MeshMaterial3d<StandardMaterial>>())
            {
                continue;
            }
            let Some(registration) = registry.get(type_id) else {
                continue;
            };
            let Some(reflect) = registration
                .data::<ReflectComponent>()
                .and_then(|component| component.reflect(entity_ref))
            else {
                continue;
            };
            let type_path = registration.type_info().type_path();
            let (key, value) = match registration.data::<ReflectScript>() {
                Some(script) => (
                    script
                        .id()
                        .map_or_else(|| type_path.to_owned(), |id| format!("script:{id}")),
                    saved_fields(reflect.as_partial_reflect(), &registry, &ids),
                ),
                None => (
                    type_path.to_owned(),
                    to_json(reflect.as_partial_reflect(), &registry, &ids),
                ),
            };
            if required.contains(&id) {
                // Some of Bevy's can't be written at all; they always come
                // back as they were brought in.
                if value.is_err() || at_default(registration, reflect.as_partial_reflect()) {
                    continue;
                }
            }
            let value = value.map_err(|message| LevelError::Component {
                node: name.clone(),
                component: key.clone(),
                message,
            })?;
            components.push((key, value));
        }
        if let Some(unknown) = unknown {
            for (key, value) in &unknown.components {
                if !components.iter().any(|(known, _)| known == key) {
                    components.push((key.clone(), value.clone()));
                }
            }
        }
        components.sort_by(|a, b| a.0.cmp(&b.0));
        level.nodes.push(LevelNode {
            id: ids[&entity],
            name,
            parent: world
                .get::<ChildOf>(entity)
                .filter(|_| entity != root)
                .and_then(|child_of| ids.get(&child_of.parent()).copied()),
            kind: world
                .get::<NodeKind>(entity)
                .filter(|_| instance.is_none())
                .map(|kind| kind.0.clone()),
            components: components.into_iter().collect(),
            prefab: instance.map(|instance| PrefabRef::from(&instance.prefab)),
            overrides: instance
                .map(|instance| instance.overrides.clone())
                .unwrap_or_default(),
            other: unknown
                .map(|unknown| unknown.keys.clone())
                .unwrap_or_default(),
        });
    }
    Ok(level)
}

/// Each node's `id`: the one it was loaded with if no other node here has
/// it, else the next free number.
fn ids(world: &World, order: &[Entity]) -> HashMap<Entity, u64> {
    let mut ids = HashMap::default();
    let mut used = HashSet::new();
    for &entity in order {
        if let Some(&NodeId(id)) = world.get::<NodeId>(entity)
            && used.insert(id)
        {
            ids.insert(entity, id);
        }
    }
    let mut next = used.iter().max().map_or(1, |max| max + 1);
    for &entity in order {
        ids.entry(entity).or_insert_with(|| {
            next += 1;
            next - 1
        });
    }
    ids
}

/// Whether `value` is its type's default.
fn at_default(registration: &TypeRegistration, value: &dyn PartialReflect) -> bool {
    registration
        .data::<ReflectDefault>()
        .and_then(|default| value.reflect_partial_eq(default.default().as_partial_reflect()))
        .unwrap_or(false)
}

/// `entity` and its descendants, parents first, skipping deleted nodes
/// and the nodes instances got from their prefabs.
fn collect(world: &World, entity: Entity, order: &mut Vec<Entity>) {
    if world.get::<Disabled>(entity).is_some() {
        return;
    }
    order.push(entity);
    if let Some(children) = world.get::<Children>(entity) {
        for child in children.iter() {
            if world.get::<FromPrefab>(child).is_none() {
                collect(world, child, order);
            }
        }
    }
}

/// A script's saved fields (those with `Export`), by name.
fn saved_fields(
    script: &dyn PartialReflect,
    registry: &TypeRegistry,
    ids: &HashMap<Entity, u64>,
) -> Result<Value, String> {
    let ReflectRef::Struct(fields) = script.reflect_ref() else {
        return Err("a script must be a struct with named fields".into());
    };
    let Some(info) = script
        .get_represented_type_info()
        .and_then(|info| info.as_struct().ok())
    else {
        return Err("the script's type isn't registered".into());
    };
    let mut saved = Map::new();
    for (index, field) in info.iter().enumerate() {
        if !field.has_attribute::<Export>() {
            continue;
        }
        if let Some(value) = fields.field_at(index) {
            let value = to_json(value, registry, ids)
                .map_err(|message| format!("field `{}`: {message}", field.name()))?;
            saved.insert(field.name().to_owned(), value);
        }
    }
    Ok(Value::Object(saved))
}

fn to_json(
    value: &dyn PartialReflect,
    registry: &TypeRegistry,
    ids: &HashMap<Entity, u64>,
) -> Result<Value, String> {
    let processor = EntityToId(ids);
    let serializer = TypedReflectSerializer::with_processor(value, registry, &processor);
    // Through text: `to_value` widens an `f32` to the `f64` nearest it,
    // writing 0.6 as 0.6000000238418579; text keeps the shortest form,
    // which reads back as the same `f32`.
    let text = serde_json::to_string(&serializer).map_err(|error| error.to_string())?;
    serde_json::from_str(&text).map_err(|error| error.to_string())
}
