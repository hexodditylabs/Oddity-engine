//! Reading a level into a World.

use std::sync::Arc;

use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
    reflect::{
        GetPath, PartialReflect, ReflectMut, TypeRegistration, TypeRegistry,
        serde::TypedReflectDeserializer, std_traits::ReflectDefault,
    },
};
use oddity_script::{Prefab, ReflectScript, attributes::Export};
use serde::de::DeserializeSeed;
use serde_json::Value;

use crate::{
    FORMAT, Level, LevelError, LevelNode, NodeId, NodeKind, Override, PREFAB_FORMAT, UnknownData,
    VERSION,
    entities::IdToEntity,
    prefabs::{FromPrefab, PrefabInstance, PrefabLibrary},
};

/// Ids in one file and the entities spawned for them.
type Spawned = HashMap<u64, Entity>;

/// A prefab instance whose nodes are spawned and whose components are next.
struct Instance {
    root: Entity,
    file: Arc<Level>,
    entities: Spawned,
    overrides: Vec<Override>,
}

/// Spawn `level` into `world` and return its root. Nodes are spawned
/// first, then their components, so a `NodeRef` can point at any node in
/// the file. Nothing is spawned if the file is invalid.
///
/// A prefab instance's nodes come from its prefab file, through the
/// [`PrefabLibrary`] resource, with its overrides on top. If the prefab
/// can't be read, the instance is an empty node that still remembers it,
/// so saving the level loses nothing.
///
/// A component that can't be read is kept in the node's [`UnknownData`]
/// and logged; a script field that is gone or no longer reads is left at its
/// default and logged.
pub fn spawn_level(world: &mut World, level: &Level) -> Result<Entity, LevelError> {
    check(level, FORMAT)?;
    let mut entities = Spawned::default();
    let mut instances = Vec::new();
    for node in &level.nodes {
        let parent = node.parent.map(|parent| entities[&parent]);
        let entity = match &node.prefab {
            Some(prefab) => {
                let prefab = Prefab::from(prefab);
                match spawn_instance_nodes(world, &prefab, node, parent) {
                    Ok(instance) => {
                        let root = instance.root;
                        instances.push(instance);
                        root
                    }
                    Err(why) => {
                        warn!("{}: prefab {:?} not loaded: {why}", node.name, prefab.path);
                        let entity = spawn_node(world, node, parent);
                        let mut placeholder = world.entity_mut(entity);
                        placeholder.insert(PrefabInstance {
                            prefab,
                            overrides: node.overrides.clone(),
                        });
                        if !node.components.is_empty() {
                            placeholder
                                .entry::<UnknownData>()
                                .or_default()
                                .get_mut()
                                .components = node.components.clone();
                        }
                        entity
                    }
                }
            }
            None => spawn_node(world, node, parent),
        };
        world.entity_mut(entity).insert(NodeId(node.id));
        entities.insert(node.id, entity);
    }

    let registry = world.resource::<AppTypeRegistry>().clone();
    let registry = registry.read();
    let scripts = script_ids(&registry);
    insert_components(world, level, &entities, &registry, &scripts);
    for instance in instances {
        insert_components(
            world,
            &instance.file,
            &instance.entities,
            &registry,
            &scripts,
        );
        apply_overrides(
            world,
            instance.root,
            &instance.overrides,
            &entities,
            &registry,
            &scripts,
        );
    }
    Ok(entities[&level
        .nodes
        .iter()
        .find(|n| n.parent.is_none())
        .expect("checked")
        .id])
}

/// Spawn the prefab `file` under `parent`, with `overrides` (whose
/// `NodeRef`s are `level` ids) on top, and return its root.
pub(crate) fn spawn_prefab(
    world: &mut World,
    prefab: &Prefab,
    file: &Arc<Level>,
    parent: Option<Entity>,
    overrides: &[Override],
    level: &Spawned,
) -> Result<Entity, LevelError> {
    check(file, PREFAB_FORMAT)?;
    let entities = spawn_prefab_nodes(world, prefab, file, parent, overrides);
    let root = entities[&file.nodes[0].id];
    let registry = world.resource::<AppTypeRegistry>().clone();
    let registry = registry.read();
    let scripts = script_ids(&registry);
    insert_components(world, file, &entities, &registry, &scripts);
    apply_overrides(world, root, overrides, level, &registry, &scripts);
    Ok(root)
}

/// An instance node's prefab, read and spawned, without components yet.
/// Its root takes the instance's name.
fn spawn_instance_nodes(
    world: &mut World,
    prefab: &Prefab,
    node: &LevelNode,
    parent: Option<Entity>,
) -> Result<Instance, LevelError> {
    let file = world
        .get_resource_mut::<PrefabLibrary>()
        .ok_or_else(|| LevelError::Invalid("there is no PrefabLibrary to read it from".into()))?
        .get(prefab)?;
    check(&file, PREFAB_FORMAT)?;
    let entities = spawn_prefab_nodes(world, prefab, &file, parent, &node.overrides);
    let root = entities[&file.nodes[0].id];
    world.entity_mut(root).insert(Name::new(node.name.clone()));
    if !node.components.is_empty() || !node.other.is_empty() {
        world.entity_mut(root).insert(UnknownData {
            components: node.components.clone(),
            keys: node.other.clone(),
        });
    }
    Ok(Instance {
        root,
        file,
        entities,
        overrides: node.overrides.clone(),
    })
}

/// A checked prefab file's nodes, marked as an instance of `prefab`.
fn spawn_prefab_nodes(
    world: &mut World,
    prefab: &Prefab,
    file: &Level,
    parent: Option<Entity>,
    overrides: &[Override],
) -> Spawned {
    let mut entities = Spawned::default();
    for node in &file.nodes {
        let entity = match node.parent {
            Some(inner) => {
                let entity = spawn_node(world, node, Some(entities[&inner]));
                world.entity_mut(entity).insert(FromPrefab);
                entity
            }
            None => {
                let entity = spawn_node(world, node, parent);
                world.entity_mut(entity).insert(PrefabInstance {
                    prefab: prefab.clone(),
                    overrides: overrides.to_vec(),
                });
                entity
            }
        };
        entities.insert(node.id, entity);
    }
    entities
}

/// A node with its name, kind and unread keys, but no components yet.
fn spawn_node(world: &mut World, node: &LevelNode, parent: Option<Entity>) -> Entity {
    let mut entity = world.spawn_empty();
    if !node.name.is_empty() {
        entity.insert(Name::new(node.name.clone()));
    }
    if let Some(kind) = &node.kind {
        entity.insert(NodeKind(kind.clone()));
    }
    if !node.other.is_empty() {
        entity.insert(UnknownData {
            keys: node.other.clone(),
            ..default()
        });
    }
    if let Some(parent) = parent {
        entity.insert(ChildOf(parent));
    }
    entity.id()
}

/// The file's shape: its format and version, unique ids, one root, parents
/// listed before their children, and no instances in a prefab.
fn check(level: &Level, format: &str) -> Result<(), LevelError> {
    let invalid = |why: String| Err(LevelError::Invalid(why));
    if level.format != format {
        return invalid(format!("format is {:?}, not {format:?}", level.format));
    }
    if level.version > VERSION {
        return invalid(format!(
            "it is version {}, from a newer Oddity; this one reads up to {VERSION}",
            level.version
        ));
    }
    let mut seen = HashSet::new();
    let mut roots = 0;
    for (index, node) in level.nodes.iter().enumerate() {
        match node.parent {
            None if format == PREFAB_FORMAT && index > 0 => {
                return invalid("a prefab's root must be its first node".into());
            }
            None => roots += 1,
            Some(parent) if !seen.contains(&parent) => {
                return invalid(format!(
                    "node {} ({:?}) comes before its parent {parent}, or its parent is missing",
                    node.id, node.name
                ));
            }
            Some(_) => {}
        }
        if !seen.insert(node.id) {
            return invalid(format!("two nodes have the id {}", node.id));
        }
        if format == PREFAB_FORMAT && node.prefab.is_some() {
            return invalid(format!(
                "node {:?} is an instance of another prefab; nested prefabs come later",
                node.name
            ));
        }
    }
    if roots != 1 {
        return invalid(format!("it has {roots} root nodes; a file has exactly one"));
    }
    Ok(())
}

/// Every script type with an `id`, by its id.
fn script_ids(registry: &TypeRegistry) -> HashMap<&'static str, TypeRegistration> {
    registry
        .iter_with_data::<ReflectScript>()
        .filter_map(|(registration, script)| Some((script.id()?, registration.clone())))
        .collect()
}

/// The type a component key names: `script:<id>` or a type path.
fn registration_for<'a>(
    key: &str,
    registry: &'a TypeRegistry,
    scripts: &'a HashMap<&'static str, TypeRegistration>,
) -> Option<&'a TypeRegistration> {
    match key.strip_prefix("script:") {
        Some(id) => scripts.get(id),
        None => registry.get_with_type_path(key),
    }
}

/// Insert the components of `file`'s nodes, an instance's own components
/// excepted (its prefab's are inserted for it).
fn insert_components(
    world: &mut World,
    file: &Level,
    entities: &Spawned,
    registry: &TypeRegistry,
    scripts: &HashMap<&'static str, TypeRegistration>,
) {
    for node in file.nodes.iter().filter(|node| node.prefab.is_none()) {
        let entity = entities[&node.id];
        let mut components: Vec<(&String, &Value, Option<&TypeRegistration>)> = node
            .components
            .iter()
            .map(|(key, value)| (key, value, registration_for(key, registry, scripts)))
            .collect();
        // A component that requires another goes first, so what it brings in
        // is what it wants (`Camera3d` brings `Camera` with a 3D render
        // graph); the saved value of the required one then replaces it. A
        // component requires more than any component it requires.
        components.sort_by_cached_key(|(_, _, registration)| {
            std::cmp::Reverse(
                registration.map_or(0, |registration| required_count(world, registration)),
            )
        });
        for (key, value, registration) in components {
            let read = registration
                .ok_or_else(|| "no such type here".to_owned())
                .and_then(|registration| {
                    insert(
                        world,
                        entity,
                        registration,
                        value,
                        registry,
                        entities,
                        &node.name,
                    )
                });
            if let Err(why) = read {
                // The editor reads scripts this way on purpose: it never
                // has the game's types.
                if registration.is_some() {
                    warn!("{}: kept {key} as written, because {why}", node.name);
                }
                keep_unread(world, entity, key, value);
            }
        }
    }
}

/// How many components `registration`'s component brings in, directly or
/// not.
fn required_count(world: &mut World, registration: &TypeRegistration) -> usize {
    let Some(component) = registration.data::<ReflectComponent>() else {
        return 0;
    };
    let id = component.register_component(world);
    world
        .components()
        .get_info(id)
        .map_or(0, |info| info.required_components().iter_ids().count())
}

fn keep_unread(world: &mut World, entity: Entity, key: &str, value: &Value) {
    world
        .entity_mut(entity)
        .entry::<UnknownData>()
        .or_default()
        .get_mut()
        .components
        .insert(key.to_owned(), value.clone());
}

/// Apply an instance's overrides over its prefab's values. One that can't
/// be applied is logged and stays in the instance, to be saved again.
fn apply_overrides(
    world: &mut World,
    root: Entity,
    overrides: &[Override],
    level: &Spawned,
    registry: &TypeRegistry,
    scripts: &HashMap<&'static str, TypeRegistration>,
) {
    for change in overrides {
        if let Err(why) = apply_override(world, root, change, level, registry, scripts) {
            let name = world.get::<Name>(root).map_or("", Name::as_str).to_owned();
            warn!(
                "{name}: override of {}.{} on {:?} not applied: {why}",
                change.component, change.field, change.node
            );
        }
    }
}

fn apply_override(
    world: &mut World,
    root: Entity,
    change: &Override,
    level: &Spawned,
    registry: &TypeRegistry,
    scripts: &HashMap<&'static str, TypeRegistration>,
) -> Result<(), String> {
    let node = node_at(world, root, &change.node).ok_or("the prefab has no such node")?;
    let Some(registration) = registration_for(&change.component, registry, scripts) else {
        return override_unread(world, node, change);
    };
    let name = world.get::<Name>(node).map_or("", Name::as_str).to_owned();
    if change.field.is_empty() {
        return insert(
            world,
            node,
            registration,
            &change.value,
            registry,
            level,
            &name,
        );
    }
    if registration.data::<ReflectScript>().is_some() {
        let first = change.field.split(['.', '[']).next().unwrap_or_default();
        let saved = registration
            .type_info()
            .as_struct()
            .ok()
            .and_then(|info| info.field(first))
            .is_some_and(|field| field.has_attribute::<Export>());
        if !saved {
            return Err(format!("`{first}` isn't a saved field of the script"));
        }
    }
    let component = registration
        .data::<ReflectComponent>()
        .ok_or("it isn't a component")?;
    let mut entity = world.entity_mut(node);
    let mut target = component
        .reflect_mut(&mut entity)
        .ok_or("the prefab's node doesn't have that component")?;
    let field = target
        .reflect_path_mut(change.field.as_str())
        .map_err(|error| error.to_string())?;
    let field_type = field
        .get_represented_type_info()
        .and_then(|info| registry.get(info.type_id()))
        .ok_or("the field's type isn't registered")?;
    let value = from_json(field_type, &change.value, registry, level)?;
    field.try_apply(&*value).map_err(|error| error.to_string())
}

/// An override of a component kept as written: change the written value.
fn override_unread(world: &mut World, node: Entity, change: &Override) -> Result<(), String> {
    let mut entity = world.entity_mut(node);
    let mut unknown = entity.entry::<UnknownData>().or_default();
    let components = &mut unknown.get_mut().components;
    if change.field.is_empty() {
        components.insert(change.component.clone(), change.value.clone());
        return Ok(());
    }
    let mut at = components
        .get_mut(&change.component)
        .ok_or("the prefab's node doesn't have that component")?;
    for step in change.field.split('.') {
        at = at
            .as_object_mut()
            .ok_or("the field isn't inside an object")?
            .entry(step)
            .or_insert(Value::Null);
    }
    *at = change.value.clone();
    Ok(())
}

/// The node at `path` below `root`, by names; `.` is `root` itself.
fn node_at(world: &World, root: Entity, path: &str) -> Option<Entity> {
    let mut at = root;
    for name in path
        .split('/')
        .filter(|step| !step.is_empty() && *step != ".")
    {
        at = world
            .get::<Children>(at)?
            .iter()
            .find(|&child| world.get::<Name>(child).map(Name::as_str) == Some(name))?;
    }
    Some(at)
}

fn insert(
    world: &mut World,
    entity: Entity,
    registration: &TypeRegistration,
    value: &Value,
    registry: &TypeRegistry,
    entities: &Spawned,
    node: &str,
) -> Result<(), String> {
    let component = registration
        .data::<ReflectComponent>()
        .ok_or("it isn't a component")?;
    let read = match registration.data::<ReflectScript>() {
        Some(_) => script(registration, value, registry, entities, node)?,
        None => from_json(registration, value, registry, entities)?,
    };
    component.insert(&mut world.entity_mut(entity), &*read, registry);
    Ok(())
}

/// A script from its saved fields: its defaults, with every saved field
/// that still exists read over them.
fn script(
    registration: &TypeRegistration,
    value: &Value,
    registry: &TypeRegistry,
    entities: &Spawned,
    node: &str,
) -> Result<Box<dyn PartialReflect>, String> {
    let saved = value.as_object().ok_or("a script is saved as an object")?;
    let mut script = registration
        .data::<ReflectDefault>()
        .ok_or("the script has no Default")?
        .default();
    let type_path = registration.type_info().type_path();
    let info = registration
        .type_info()
        .as_struct()
        .map_err(|_| "a script must be a struct with named fields")?;
    let ReflectMut::Struct(fields) = script.reflect_mut() else {
        return Err("a script must be a struct with named fields".into());
    };
    for (name, value) in saved {
        let Some(field) = info
            .field(name)
            .filter(|field| field.has_attribute::<Export>())
        else {
            warn!("{node}: {type_path} has no saved field `{name}` any more; its value is dropped");
            continue;
        };
        let read = registry
            .get(field.type_id())
            .ok_or_else(|| format!("{} isn't registered", field.type_path()))
            .and_then(|field_type| from_json(field_type, value, registry, entities));
        match (read, fields.field_mut(name)) {
            (Ok(read), Some(target)) => {
                if let Err(error) = target.try_apply(&*read) {
                    warn!("{node}: {type_path}.{name} kept its default: {error}");
                }
            }
            (Err(why), _) => warn!("{node}: {type_path}.{name} kept its default: {why}"),
            (Ok(_), None) => {}
        }
    }
    Ok(script.into_partial_reflect())
}

fn from_json(
    registration: &TypeRegistration,
    value: &Value,
    registry: &TypeRegistry,
    entities: &Spawned,
) -> Result<Box<dyn PartialReflect>, String> {
    let mut processor = IdToEntity(entities);
    TypedReflectDeserializer::with_processor(registration, registry, &mut processor)
        .deserialize(value)
        .map_err(|error| error.to_string())
}
