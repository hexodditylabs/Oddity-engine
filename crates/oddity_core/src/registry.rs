//! Commands by name, with JSON arguments, for the CLI, MCP and plugins.
//!
//! ```ignore
//! world.run_command("entity.spawn", json!({ "name": "Player" }))?;   // {"entity": 4294967297}
//! world.run_command("component.set", json!({
//!     "entity": 4294967297,
//!     "component": "Transform",
//!     "path": "translation.x",
//!     "value": 3.0,
//! }))?;
//! world.run_command("history.undo", json!({}))?;
//! ```

use std::collections::BTreeMap;

use bevy_ecs::{entity::Entity, resource::Resource, world::World};
use serde_json::{Map, Value};

use crate::{
    CommandError, EditorCommand,
    commands::{
        AddComponent, DespawnEntity, HistoryStep, RemoveComponent, RenameEntity, ReparentEntity,
        SetField, SetSelection, SpawnEntity,
    },
};

/// Arguments: a JSON object.
pub type Args = Map<String, Value>;

/// Builds a command from its arguments.
pub type BuildFn = fn(&mut World, &Args) -> Result<Box<dyn EditorCommand>, CommandError>;

/// What kind of value an argument takes. Entities are their `u64` bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgKind {
    /// One entity, as a number.
    Entity,
    /// A list of entities.
    Entities,
    /// Text.
    String,
    /// Any JSON value (read using the target field's type).
    Json,
}

/// One argument of a command.
#[derive(Clone, Copy, Debug)]
pub struct ArgSpec {
    /// Argument name in the JSON object.
    pub name: &'static str,
    /// Value kind.
    pub kind: ArgKind,
    /// Whether it must be given.
    pub required: bool,
    /// One line for help text and MCP tool descriptions.
    pub description: &'static str,
}

/// A command's public description.
#[derive(Clone, Copy, Debug)]
pub struct CommandSpec {
    /// Dotted id, e.g. `entity.spawn`.
    pub id: &'static str,
    /// One line for help text and MCP tool descriptions.
    pub description: &'static str,
    /// Its arguments.
    pub args: &'static [ArgSpec],
}

/// Every command that can be run by name.
#[derive(Resource, Default)]
pub struct CommandRegistry {
    commands: BTreeMap<&'static str, (CommandSpec, BuildFn)>,
}

impl CommandRegistry {
    /// The registry with Oddity's built-in commands.
    pub fn with_builtins() -> Self {
        let mut registry = Self::default();
        register_builtins(&mut registry);
        registry
    }

    /// Add or replace a command. Plugins use this to add their own.
    pub fn register(&mut self, spec: CommandSpec, build: BuildFn) {
        self.commands.insert(spec.id, (spec, build));
    }

    /// A command's description.
    pub fn spec(&self, id: &str) -> Option<&CommandSpec> {
        self.commands.get(id).map(|(spec, _)| spec)
    }

    /// Every command, sorted by id.
    pub fn specs(&self) -> impl Iterator<Item = &CommandSpec> {
        self.commands.values().map(|(spec, _)| spec)
    }

    pub(crate) fn build_fn(&self, id: &str) -> Option<BuildFn> {
        self.commands.get(id).map(|(_, build)| *build)
    }
}

/// An entity argument.
pub fn entity_arg(args: &Args, name: &str) -> Result<Entity, CommandError> {
    let value = args
        .get(name)
        .ok_or_else(|| CommandError::MissingArgument(name.to_owned()))?;
    entity_from_json(name, value)
}

/// An optional entity argument (`null` or missing = none).
pub fn opt_entity_arg(args: &Args, name: &str) -> Result<Option<Entity>, CommandError> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => entity_from_json(name, value).map(Some),
    }
}

/// A list-of-entities argument.
pub fn entities_arg(args: &Args, name: &str) -> Result<Vec<Entity>, CommandError> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => items.iter().map(|v| entity_from_json(name, v)).collect(),
        Some(_) => Err(CommandError::BadArgument {
            name: name.to_owned(),
            reason: "expected a list of entities".into(),
        }),
    }
}

/// A text argument.
pub fn string_arg<'a>(args: &'a Args, name: &str) -> Result<&'a str, CommandError> {
    args.get(name)
        .ok_or_else(|| CommandError::MissingArgument(name.to_owned()))?
        .as_str()
        .ok_or_else(|| CommandError::BadArgument {
            name: name.to_owned(),
            reason: "expected text".into(),
        })
}

/// An optional text argument.
pub fn opt_string_arg<'a>(args: &'a Args, name: &str) -> Result<Option<&'a str>, CommandError> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => string_arg(args, name).map(Some),
    }
}

fn entity_from_json(name: &str, value: &Value) -> Result<Entity, CommandError> {
    value
        .as_u64()
        .and_then(Entity::try_from_bits)
        .ok_or_else(|| CommandError::BadArgument {
            name: name.to_owned(),
            reason: "expected an entity id (a number)".into(),
        })
}

const ENTITY: ArgSpec = ArgSpec {
    name: "entity",
    kind: ArgKind::Entity,
    required: true,
    description: "The entity to change",
};
const COMPONENT: ArgSpec = ArgSpec {
    name: "component",
    kind: ArgKind::String,
    required: true,
    description: "Component type path, or its short name if unambiguous (e.g. Transform)",
};

fn register_builtins(registry: &mut CommandRegistry) {
    registry.register(
        CommandSpec {
            id: "entity.spawn",
            description: "Create a named entity (node), optionally as a child. Returns {\"entity\": id}.",
            args: &[
                ArgSpec {
                    name: "name",
                    kind: ArgKind::String,
                    required: true,
                    description: "The node's name",
                },
                ArgSpec {
                    name: "parent",
                    kind: ArgKind::Entity,
                    required: false,
                    description: "Parent entity; omit for a top-level node",
                },
            ],
        },
        |world, args| {
            let name = string_arg(args, "name")?.to_owned();
            let parent = opt_entity_arg(args, "parent")?;
            let mut command = SpawnEntity::new(world, name);
            if let Some(parent) = parent {
                command = command.child_of(parent);
            }
            Ok(Box::new(command))
        },
    );
    registry.register(
        CommandSpec {
            id: "entity.despawn",
            description: "Delete an entity and its children (undoable).",
            args: &[ENTITY],
        },
        |world, args| {
            let entity = entity_arg(args, "entity")?;
            Ok(Box::new(DespawnEntity::new(world, entity)))
        },
    );
    registry.register(
        CommandSpec {
            id: "entity.rename",
            description: "Rename an entity.",
            args: &[
                ENTITY,
                ArgSpec {
                    name: "name",
                    kind: ArgKind::String,
                    required: true,
                    description: "The new name",
                },
            ],
        },
        |_, args| {
            Ok(Box::new(RenameEntity::new(
                entity_arg(args, "entity")?,
                string_arg(args, "name")?,
            )))
        },
    );
    registry.register(
        CommandSpec {
            id: "entity.reparent",
            description: "Move an entity under a new parent, or to the top level.",
            args: &[
                ENTITY,
                ArgSpec {
                    name: "parent",
                    kind: ArgKind::Entity,
                    required: false,
                    description: "New parent; omit or null for the top level",
                },
            ],
        },
        |_, args| {
            Ok(Box::new(ReparentEntity::new(
                entity_arg(args, "entity")?,
                opt_entity_arg(args, "parent")?,
            )))
        },
    );
    registry.register(
        CommandSpec {
            id: "component.set",
            description: "Set a component field through reflection, e.g. path \"translation.x\".",
            args: &[
                ENTITY,
                COMPONENT,
                ArgSpec {
                    name: "path",
                    kind: ArgKind::String,
                    required: false,
                    description: "Field path inside the component; empty sets the whole component",
                },
                ArgSpec {
                    name: "value",
                    kind: ArgKind::Json,
                    required: true,
                    description: "New value as JSON, in the field's own shape",
                },
            ],
        },
        |world, args| {
            let entity = entity_arg(args, "entity")?;
            let component = string_arg(args, "component")?;
            let path = opt_string_arg(args, "path")?.unwrap_or("");
            let value = args
                .get("value")
                .cloned()
                .ok_or_else(|| CommandError::MissingArgument("value".into()))?;
            Ok(Box::new(SetField::from_json(
                world, entity, component, path, value,
            )?))
        },
    );
    registry.register(
        CommandSpec {
            id: "component.add",
            description: "Add a component with its default value.",
            args: &[ENTITY, COMPONENT],
        },
        |_, args| {
            Ok(Box::new(AddComponent::new(
                entity_arg(args, "entity")?,
                string_arg(args, "component")?,
            )))
        },
    );
    registry.register(
        CommandSpec {
            id: "component.remove",
            description: "Remove a component (undo restores its value).",
            args: &[ENTITY, COMPONENT],
        },
        |_, args| {
            Ok(Box::new(RemoveComponent::new(
                entity_arg(args, "entity")?,
                string_arg(args, "component")?,
            )))
        },
    );
    registry.register(
        CommandSpec {
            id: "selection.set",
            description: "Select these entities (not an undo step).",
            args: &[ArgSpec {
                name: "entities",
                kind: ArgKind::Entities,
                required: true,
                description: "Entities to select; empty clears the selection",
            }],
        },
        |_, args| Ok(Box::new(SetSelection(entities_arg(args, "entities")?))),
    );
    registry.register(
        CommandSpec {
            id: "history.undo",
            description: "Undo the last change.",
            args: &[],
        },
        |_, _| Ok(Box::new(HistoryStep { redo: false })),
    );
    registry.register(
        CommandSpec {
            id: "history.redo",
            description: "Redo the last undone change.",
            args: &[],
        },
        |_, _| Ok(Box::new(HistoryStep { redo: true })),
    );
}
