//! Built-in queries: reading the level, components, selection, history and
//! the command list by name, for the CLI, MCP and tests.
//!
//! A query gets the world read-only ([`QueryFn`](crate::registry::QueryFn)),
//! so it cannot change anything and never appears in the history.
//!
//! ```ignore
//! world.run_command("level.tree", json!({}))?;     // {"root": {"entity": .., "name": "Main", "children": [..]}}
//! world.run_command("component.get", json!({"entity": 4294967297, "component": "Transform", "path": "translation"}))?;
//! ```

use bevy_ecs::{
    entity::Entity, entity_disabling::Disabled, hierarchy::Children, name::Name, query::With,
    world::World,
};
use serde_json::{Value, json};

use crate::{
    ArgKind, ArgSpec, Args, CommandError, CommandRegistry, CommandSpec, History, LevelRoot,
    Selection, reflect_json,
    registry::{entity_arg, opt_string_arg, string_arg},
};

const ENTITY: ArgSpec = ArgSpec {
    name: "entity",
    kind: ArgKind::Entity,
    required: true,
    description: "The entity to read",
};

pub(crate) fn register(registry: &mut CommandRegistry) {
    registry.register_query(
        CommandSpec::query(
            "level.tree",
            "The level's node tree: {\"root\": {entity, name, children: [...]}}, or a null root when no level is open.",
            &[],
        ),
        level_tree,
    );
    registry.register_query(
        CommandSpec::query(
            "component.get",
            "A component's value, or one field of it, as JSON: {\"value\": ...}.",
            &[
                ENTITY,
                ArgSpec {
                    name: "component",
                    kind: ArgKind::String,
                    required: true,
                    description: "Component type path, or its short name if unambiguous (e.g. Transform)",
                },
                ArgSpec {
                    name: "path",
                    kind: ArgKind::String,
                    required: false,
                    description: "Field path inside the component, e.g. translation.x; empty for the whole component",
                },
            ],
        ),
        component_get,
    );
    registry.register_query(
        CommandSpec::query(
            "component.list",
            "The type paths of an entity's editable components: {\"components\": [...]}.",
            &[ENTITY],
        ),
        component_list,
    );
    registry.register_query(
        CommandSpec::query(
            "selection.get",
            "The selected entities, oldest first, and the primary one (shown in the Inspector).",
            &[],
        ),
        selection_get,
    );
    registry.register_query(
        CommandSpec::query(
            "history.list",
            "The undo and redo steps, each with its label and invocation, and whether there are unsaved changes.",
            &[],
        ),
        history_list,
    );
    registry.register_query(
        CommandSpec::query(
            "commands.list",
            "Every command and query: id, kind, description, arguments, destructive, example.",
            &[],
        ),
        commands_list,
    );
}

/// An entity that exists and isn't deleted (deleted nodes are only disabled,
/// waiting for undo).
fn live_entity(world: &World, args: &Args) -> Result<Entity, CommandError> {
    let entity = entity_arg(args, "entity")?;
    match world.get_entity(entity) {
        Ok(found) if !found.contains::<Disabled>() => Ok(entity),
        _ => Err(CommandError::NoEntity(entity)),
    }
}

fn level_tree(world: &World, _args: &Args) -> Result<Value, CommandError> {
    let root = world
        .try_query_filtered::<Entity, With<LevelRoot>>()
        .and_then(|query| query.iter_manual(world).next());
    Ok(json!({ "root": root.map(|root| node_json(world, root)) }))
}

/// A node and its live children, recursively.
fn node_json(world: &World, entity: Entity) -> Value {
    let children: Vec<Value> = world
        .get::<Children>(entity)
        .into_iter()
        .flat_map(|children| children.iter().copied())
        .filter(|child| world.get::<Disabled>(*child).is_none())
        .map(|child| node_json(world, child))
        .collect();
    json!({
        "entity": entity.to_bits(),
        "name": world.get::<Name>(entity).map(Name::as_str),
        "children": children,
    })
}

fn component_get(world: &World, args: &Args) -> Result<Value, CommandError> {
    let entity = live_entity(world, args)?;
    let component = string_arg(args, "component")?;
    let path = opt_string_arg(args, "path")?.unwrap_or("");
    let value = reflect_json::field_json(world, entity, component, path)?;
    Ok(json!({ "value": value }))
}

fn component_list(world: &World, args: &Args) -> Result<Value, CommandError> {
    let entity = live_entity(world, args)?;
    Ok(json!({ "components": reflect_json::components_of(world, entity)? }))
}

fn selection_get(world: &World, _args: &Args) -> Result<Value, CommandError> {
    let selection = world.get_resource::<Selection>();
    let entities: Vec<u64> = selection
        .map(|s| s.entities().iter().map(|e| e.to_bits()).collect())
        .unwrap_or_default();
    let primary = selection.and_then(Selection::primary).map(Entity::to_bits);
    Ok(json!({ "entities": entities, "primary": primary }))
}

fn history_list(world: &World, _args: &Args) -> Result<Value, CommandError> {
    Ok(world
        .get_resource::<History>()
        .map_or_else(|| History::default().to_json(), History::to_json))
}

fn commands_list(world: &World, _args: &Args) -> Result<Value, CommandError> {
    let commands: Vec<Value> = world
        .get_resource::<CommandRegistry>()
        .into_iter()
        .flat_map(CommandRegistry::specs)
        .map(CommandSpec::to_json)
        .collect();
    Ok(json!({ "commands": commands }))
}
