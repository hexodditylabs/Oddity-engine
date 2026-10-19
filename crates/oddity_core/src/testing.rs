//! The round-trip check: every edit and setting command's example is run on
//! the standard test level, then undone, redone and replayed, and the level
//! must come back exactly each time (`roadmap/commands.md`, "Keeping it
//! honest"). Undo bugs are silent; this makes them loud.
//!
//! ```ignore
//! let failures = round_trip_all(&standard_world);
//! assert!(failures.is_empty(), "{failures:#?}");
//! ```
//!
//! An extension checks its own commands by passing a world that also has its
//! headless plugin: `round_trip_all(&|| standard_world_with(|app| { app.add_plugins(MyPlugin); }))`.
//!
//! Examples write entities as node paths in the standard level
//! (`"Enemies/Goblin"`), since raw ids differ from one copy of a level to the
//! next; the check turns them into ids before running.

use bevy_app::App;
use bevy_ecs::{
    component::Component,
    entity::Entity,
    entity_disabling::Disabled,
    hierarchy::{ChildOf, Children},
    name::Name,
    reflect::ReflectComponent,
    world::World,
};
use bevy_reflect::{Reflect, TypePath, prelude::ReflectDefault};
use serde_json::{Map, Value, json};

use crate::{
    ArgKind, CommandError, CommandKind, CommandRegistry, EditorWorldExt, History, LevelRoot,
    headless_app, level_root, reflect_json,
    scripts::{ScriptInstance, ScriptTypes, Scripts},
};

/// A health bar, for examples to edit on the standard level.
///
/// Registered only in [`standard_world`], never automatically, so it can't
/// turn up in the editor or clash with a game's own `Health`.
#[derive(Component, Reflect, Default, Debug, Clone, PartialEq)]
#[reflect(Component, Default, no_auto_register)]
pub struct Health {
    /// Hit points left.
    pub current: f32,
    /// Hit points when full.
    pub max: f32,
}

/// A speed, for examples to add and remove on the standard level.
/// Registered only in [`standard_world`], like [`Health`].
#[derive(Component, Reflect, Default, Debug, Clone, PartialEq)]
#[reflect(Component, Default, no_auto_register)]
pub struct Speed(pub f32);

/// A headless editor with the standard test level open:
///
/// ```text
/// Main
/// ├── Player   Health 10/10
/// │   └── Body
/// └── Enemies
///     ├── Goblin   Health 4/10, script Mover (speed 3)
///     └── Goblin2
/// ```
///
/// The game's type description knows one script, `game::Mover`, with a
/// `speed: f32` (default 3) and a `target: NodeRef`.
///
/// Nodes have siblings on both sides somewhere, so a command that puts a
/// node back in the wrong place among its siblings is caught.
pub fn standard_world() -> World {
    standard_world_with(|_| {})
}

/// [`standard_world`], after `setup` has added to the app: how an extension
/// checks its own commands, by adding its headless plugin.
pub fn standard_world_with(setup: impl FnOnce(&mut App)) -> World {
    let mut app = headless_app();
    app.register_type::<Health>().register_type::<Speed>();
    setup(&mut app);
    let mut world = std::mem::take(app.world_mut());
    world.insert_resource(standard_script_types());
    standard_level(&mut world);
    world
}

/// Spawn the standard test level (see [`standard_world`]). Returns its root.
pub fn standard_level(world: &mut World) -> Entity {
    let root = world.spawn((Name::new("Main"), LevelRoot)).id();
    let health = |current| Health { current, max: 10.0 };
    let player = world
        .spawn((Name::new("Player"), health(10.0), ChildOf(root)))
        .id();
    world.spawn((Name::new("Body"), ChildOf(player)));
    let enemies = world.spawn((Name::new("Enemies"), ChildOf(root))).id();
    let mover = standard_script_types().scripts[0].clone();
    world.spawn((
        Name::new("Goblin"),
        health(4.0),
        Scripts(vec![ScriptInstance {
            type_path: mover.type_path.clone(),
            enabled: true,
            values: mover.defaults(),
        }]),
        ChildOf(enemies),
    ));
    world.spawn((Name::new("Goblin2"), ChildOf(enemies)));
    root
}

/// The standard world's type description: one script, `game::Mover`.
pub fn standard_script_types() -> ScriptTypes {
    serde_json::from_value(json!({
        "scripts": [{
            "type_path": "game::Mover",
            "name": "Mover",
            "id": "6d0e1f2a",
            "fields": [
                { "name": "speed", "type": "f32", "default": 3.0 },
                { "name": "target", "type": "NodeRef", "default": null }
            ]
        }]
    }))
    .expect("the standard script types are valid")
}

/// What the round-trip check found wrong with one command. Every message
/// starts with the command's id.
#[derive(Debug, thiserror::Error)]
pub enum RoundTripError {
    /// Edits and settings must carry an example.
    #[error("`{0}` has no example: give its CommandSpec one with .example(..)")]
    NoExample(String),
    /// The example isn't a JSON object, or names a node that isn't there.
    #[error("`{id}`: bad example: {reason}")]
    BadExample {
        /// The command.
        id: String,
        /// What is wrong with it.
        reason: String,
    },
    /// The example failed on the standard level.
    #[error("`{id}` failed on the standard level: {error}")]
    Failed {
        /// The command.
        id: String,
        /// Why.
        error: CommandError,
    },
    /// The example changed nothing, so it checks nothing.
    #[error("`{0}`: its example changed nothing on the standard level")]
    NoChange(String),
    /// Undo or redo returned an error.
    #[error("`{id}`: {step} failed: {error}")]
    StepFailed {
        /// The command.
        id: String,
        /// "undo" or "redo".
        step: &'static str,
        /// Why.
        error: CommandError,
    },
    /// After a step, the level differs from what it should be.
    #[error(
        "`{id}`: after {step} the level is not what it should be\n  expected: {expected}\n  found:    {found}"
    )]
    Mismatch {
        /// The command.
        id: String,
        /// "undo", "redo" or "replay".
        step: &'static str,
        /// The snapshot it should match.
        expected: Box<Value>,
        /// The snapshot it has.
        found: Box<Value>,
    },
    /// A query changed the level or the history.
    #[error("`{0}` is a query but changed something")]
    QueryChanged(String),
}

/// Run the round-trip check on every registered command. `new_world` makes
/// a fresh copy of the test level each time it is called; use
/// [`standard_world`], plus an extension's plugin to check its commands.
pub fn round_trip_all(new_world: &dyn Fn() -> World) -> Vec<RoundTripError> {
    let world = new_world();
    let specs: Vec<(&'static str, CommandKind)> = world
        .resource::<CommandRegistry>()
        .specs()
        .map(|spec| (spec.id, spec.kind))
        .collect();
    specs
        .into_iter()
        .filter_map(|(id, kind)| match kind {
            CommandKind::Edit | CommandKind::Setting => round_trip(new_world, id).err(),
            CommandKind::Query => query_changes_nothing(new_world, id).err(),
            // Not undoable, and many reach outside the level (save, play).
            CommandKind::Action => None,
        })
        .collect()
}

/// Run one command's example on a fresh level, then check that undo, redo
/// and replaying its invocation on another fresh copy all give the level
/// they should.
pub fn round_trip(new_world: &dyn Fn() -> World, id: &str) -> Result<(), RoundTripError> {
    let mut world = new_world();
    let args = example_args(&mut world, id)?;
    let before = snapshot(&mut world);

    world
        .run_command(id, Value::Object(args))
        .map_err(|error| RoundTripError::Failed {
            id: id.to_owned(),
            error,
        })?;
    let after = snapshot(&mut world);
    if after == before {
        return Err(RoundTripError::NoChange(id.to_owned()));
    }

    let step = |world: &mut World, step: &'static str, expected: &Value| {
        let result = if step == "undo" {
            world.undo()
        } else {
            world.redo()
        };
        result.map_err(|error| RoundTripError::StepFailed {
            id: id.to_owned(),
            step,
            error,
        })?;
        matches(id, step, expected, snapshot(world))
    };
    step(&mut world, "undo", &before)?;
    step(&mut world, "redo", &after)?;

    // The invocation names the same nodes by id, and a fresh copy is built
    // the same way, so its ids match.
    let (replay_id, replay_args) =
        newest_invocation(&world).ok_or_else(|| RoundTripError::NoChange(id.to_owned()))?;
    let mut fresh = new_world();
    fresh
        .run_command(replay_id, Value::Object(replay_args))
        .map_err(|error| RoundTripError::Failed {
            id: id.to_owned(),
            error,
        })?;
    matches(id, "replay", &after, snapshot(&mut fresh))
}

fn newest_invocation(world: &World) -> Option<(&'static str, Map<String, Value>)> {
    let list = world.resource::<History>().to_json();
    let newest = list["undo"].as_array()?.last()?;
    let id = newest["invocation"]["id"].as_str()?;
    let id = world.resource::<CommandRegistry>().spec(id)?.id;
    let args = newest["invocation"]["args"].as_object()?.clone();
    Some((id, args))
}

fn matches(
    id: &str,
    step: &'static str,
    expected: &Value,
    found: Value,
) -> Result<(), RoundTripError> {
    if found == *expected {
        Ok(())
    } else {
        Err(RoundTripError::Mismatch {
            id: id.to_owned(),
            step,
            expected: Box::new(expected.clone()),
            found: Box::new(found),
        })
    }
}

/// A query gets the world read-only, so it can't change anything; this
/// checks that stays true.
fn query_changes_nothing(new_world: &dyn Fn() -> World, id: &str) -> Result<(), RoundTripError> {
    let mut world = new_world();
    let has_example = world
        .resource::<CommandRegistry>()
        .spec(id)
        .is_some_and(|spec| spec.example.is_some());
    let args = if has_example {
        example_args(&mut world, id)?
    } else {
        Map::new()
    };
    let before = (snapshot(&mut world), world.resource::<History>().to_json());
    // Whether it answers doesn't matter here, only that it changes nothing.
    let _ = world.run_command(id, Value::Object(args));
    let after = (snapshot(&mut world), world.resource::<History>().to_json());
    if before == after {
        Ok(())
    } else {
        Err(RoundTripError::QueryChanged(id.to_owned()))
    }
}

/// The command's example, with node paths turned into entity ids.
fn example_args(world: &mut World, id: &str) -> Result<Map<String, Value>, RoundTripError> {
    let bad = |reason: String| RoundTripError::BadExample {
        id: id.to_owned(),
        reason,
    };
    let spec = *world
        .resource::<CommandRegistry>()
        .spec(id)
        .ok_or_else(|| bad("not registered".into()))?;
    let text = spec
        .example
        .ok_or_else(|| RoundTripError::NoExample(id.to_owned()))?;
    let mut args = match serde_json::from_str(text) {
        Ok(Value::Object(args)) => args,
        Ok(_) => return Err(bad("not a JSON object".into())),
        Err(error) => return Err(bad(error.to_string())),
    };
    let root = level_root(world).ok_or_else(|| bad("no level is open".into()))?;
    for arg in spec.args {
        let Some(value) = args.get_mut(arg.name) else {
            continue;
        };
        match (arg.kind, value) {
            (ArgKind::Entity, value @ Value::String(_)) => {
                *value = node_id(world, root, value).map_err(bad)?;
            }
            (ArgKind::Entities, Value::Array(items)) => {
                for item in items.iter_mut().filter(|item| item.is_string()) {
                    *item = node_id(world, root, item).map_err(bad)?;
                }
            }
            _ => {}
        }
    }
    Ok(args)
}

/// The id of the node at `path` (`"Enemies/Goblin"`, or `""` for the root).
fn node_id(world: &World, root: Entity, path: &Value) -> Result<Value, String> {
    let path = path.as_str().unwrap_or_default();
    let mut node = root;
    for name in path.split('/').filter(|name| !name.is_empty()) {
        node = live_children(world, node)
            .find(|child| {
                world
                    .get::<Name>(*child)
                    .is_some_and(|n| n.as_str() == name)
            })
            .ok_or_else(|| format!("no node at `{path}` in the standard level"))?;
    }
    Ok(json!(node.to_bits()))
}

fn live_children(world: &World, entity: Entity) -> impl Iterator<Item = Entity> + '_ {
    world
        .get::<Children>(entity)
        .into_iter()
        .flat_map(|children| children.iter().copied())
        .filter(|child| world.get::<Disabled>(*child).is_none())
}

/// The level as data: each live node's name, its components' values and its
/// children, in order. Entity ids are left out, so two copies of a level
/// compare equal; deleted nodes, waiting for undo, are left out too.
pub fn snapshot(world: &mut World) -> Value {
    match level_root(world) {
        Some(root) => node_snapshot(world, root),
        None => Value::Null,
    }
}

fn node_snapshot(world: &World, node: Entity) -> Value {
    // The hierarchy is the snapshot's shape and the name has its own field;
    // `Name` can't be read through reflection anyway (Bevy 0.20 registers no
    // serializer for the hashed string inside it).
    let covered = [
        ChildOf::type_path(),
        Children::type_path(),
        Name::type_path(),
    ];
    let components: Map<String, Value> = reflect_json::components_of(world, node)
        .unwrap_or_default()
        .into_iter()
        .filter(|path| !covered.contains(&path.as_str()))
        .map(|path| {
            let value = reflect_json::component_json(world, node, &path)
                .unwrap_or_else(|error| json!({ "unreadable": error.to_string() }));
            (path, value)
        })
        .collect();
    let children: Vec<Value> = live_children(world, node)
        .map(|child| node_snapshot(world, child))
        .collect();
    json!({
        "name": world.get::<Name>(node).map(Name::as_str),
        "components": components,
        "children": children,
    })
}
