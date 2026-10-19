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
use serde_json::{Map, Value, json};

use crate::{
    CommandError, EditorCommand, Invocation,
    commands::{
        AddComponent, Batch, DespawnEntity, HistoryGroup, HistoryStep, RemoveComponent,
        RenameEntity, ReparentEntity, SetField, SetSelection, SpawnEntity,
    },
};

/// Arguments: a JSON object.
pub type Args = Map<String, Value>;

/// Builds a command from its arguments.
pub type BuildFn = fn(&mut World, &Args) -> Result<Box<dyn EditorCommand>, CommandError>;

/// Answers a query. It gets the world read-only, so a query cannot change
/// anything and never reaches the history.
pub type QueryFn = fn(&World, &Args) -> Result<Value, CommandError>;

/// What a command does, which decides where it goes in history and how MCP
/// describes it to an agent (`roadmap/commands.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKind {
    /// Changes an open level or prefab; undoable.
    Edit,
    /// Changes project settings (`oddity.toml`); undoable.
    Setting,
    /// Does something outside the documents (save, build, play); not undoable.
    Action,
    /// Reads, never changes anything.
    Query,
}

impl CommandKind {
    /// Lower-case name, as `commands.list` reports it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Edit => "edit",
            Self::Setting => "setting",
            Self::Action => "action",
            Self::Query => "query",
        }
    }
}

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

impl ArgKind {
    /// Lower-case name, as `commands.list` reports it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Entity => "entity",
            Self::Entities => "entities",
            Self::String => "string",
            Self::Json => "json",
        }
    }
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

/// A command's public description. Build one with [`CommandSpec::edit`],
/// [`CommandSpec::setting`], [`CommandSpec::action`] or [`CommandSpec::query`].
#[derive(Clone, Copy, Debug)]
pub struct CommandSpec {
    /// Dotted id, `<feature>.<verb>`, e.g. `entity.spawn`.
    pub id: &'static str,
    /// What it does.
    pub kind: CommandKind,
    /// One line for help text and MCP tool descriptions.
    pub description: &'static str,
    /// Its arguments.
    pub args: &'static [ArgSpec],
    /// An action that overwrites or deletes something it can't bring back.
    pub destructive: bool,
    /// Arguments, as JSON text, that change the standard test level
    /// ([`testing::standard_world`](crate::testing::standard_world)), with
    /// entities written as node paths (`"Enemies/Goblin"`). Every Edit and
    /// Setting command needs one: the round-trip check runs it.
    pub example: Option<&'static str>,
}

impl CommandSpec {
    const fn new(
        kind: CommandKind,
        id: &'static str,
        description: &'static str,
        args: &'static [ArgSpec],
    ) -> Self {
        Self {
            id,
            kind,
            description,
            args,
            destructive: false,
            example: None,
        }
    }

    /// A command that changes an open level or prefab.
    pub const fn edit(
        id: &'static str,
        description: &'static str,
        args: &'static [ArgSpec],
    ) -> Self {
        Self::new(CommandKind::Edit, id, description, args)
    }

    /// A command that changes project settings.
    pub const fn setting(
        id: &'static str,
        description: &'static str,
        args: &'static [ArgSpec],
    ) -> Self {
        Self::new(CommandKind::Setting, id, description, args)
    }

    /// A command that does something outside the documents.
    pub const fn action(
        id: &'static str,
        description: &'static str,
        args: &'static [ArgSpec],
    ) -> Self {
        Self::new(CommandKind::Action, id, description, args)
    }

    /// A read-only query. Register it with [`CommandRegistry::register_query`].
    pub const fn query(
        id: &'static str,
        description: &'static str,
        args: &'static [ArgSpec],
    ) -> Self {
        Self::new(CommandKind::Query, id, description, args)
    }

    /// Mark an action as overwriting or deleting something.
    pub const fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    /// Example arguments as JSON text, e.g. `r#"{"entity": "Player"}"#`.
    /// See [`CommandSpec::example`](Self#structfield.example).
    pub const fn example(mut self, args: &'static str) -> Self {
        self.example = Some(args);
        self
    }

    /// The spec as JSON, as `commands.list` returns it.
    pub fn to_json(&self) -> Value {
        let args: Vec<Value> = self
            .args
            .iter()
            .map(|arg| {
                json!({
                    "name": arg.name,
                    "kind": arg.kind.name(),
                    "required": arg.required,
                    "description": arg.description,
                })
            })
            .collect();
        let example = self
            .example
            .and_then(|text| serde_json::from_str::<Value>(text).ok());
        json!({
            "id": self.id,
            "kind": self.kind.name(),
            "description": self.description,
            "args": args,
            "destructive": self.destructive,
            "example": example,
        })
    }
}

/// A command that registers by its type: its spec, and how to build it
/// from `run_command`'s JSON arguments. Extensions register these with
/// `oddity_plugin_api`'s `register_command::<C>()`.
pub trait NamedCommand: EditorCommand + Sized {
    /// Its id, kind, description and arguments.
    const SPEC: CommandSpec;

    /// Build the command from its arguments, as `run_command` passes them.
    fn from_args(world: &mut World, args: &Args) -> Result<Self, CommandError>;
}

/// How a registered id runs.
#[derive(Clone, Copy)]
pub(crate) enum Runner {
    /// Builds a command, which goes through the history.
    Command(BuildFn),
    /// Answers from a read-only world.
    Query(QueryFn),
}

/// Every command and query that can be run by name.
#[derive(Resource, Default)]
pub struct CommandRegistry {
    commands: BTreeMap<&'static str, (CommandSpec, Runner)>,
}

impl CommandRegistry {
    /// The registry with Oddity's built-in commands and queries.
    pub fn with_builtins() -> Self {
        let mut registry = Self::default();
        register_builtins(&mut registry);
        crate::queries::register(&mut registry);
        crate::scripts::register(&mut registry);
        registry
    }

    /// Add or replace a command. Plugins use this to add their own.
    ///
    /// # Panics
    ///
    /// If `spec` is a query: queries go through [`Self::register_query`],
    /// which only ever gives them a read-only world.
    pub fn register(&mut self, spec: CommandSpec, build: BuildFn) {
        assert!(
            spec.kind != CommandKind::Query,
            "`{}` is a query: register it with register_query",
            spec.id
        );
        self.commands
            .insert(spec.id, (spec, Runner::Command(build)));
    }

    /// Add or replace a command by its type.
    ///
    /// # Panics
    ///
    /// If `C::SPEC` is a query, as [`Self::register`] does.
    pub fn register_named<C: NamedCommand>(&mut self) {
        self.register(C::SPEC, |world, args| {
            Ok(Box::new(C::from_args(world, args)?))
        });
    }

    /// Add or replace a query.
    ///
    /// # Panics
    ///
    /// If `spec` is not a query.
    pub fn register_query(&mut self, spec: CommandSpec, run: QueryFn) {
        assert!(
            spec.kind == CommandKind::Query,
            "`{}` is not a query: register it with register",
            spec.id
        );
        self.commands.insert(spec.id, (spec, Runner::Query(run)));
    }

    /// A command's description.
    pub fn spec(&self, id: &str) -> Option<&CommandSpec> {
        self.commands.get(id).map(|(spec, _)| spec)
    }

    /// Every command and query, sorted by id.
    pub fn specs(&self) -> impl Iterator<Item = &CommandSpec> {
        self.commands.values().map(|(spec, _)| spec)
    }

    pub(crate) fn runner(&self, id: &str) -> Option<Runner> {
        self.commands.get(id).map(|(_, runner)| *runner)
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
        CommandSpec::edit(
            "entity.spawn",
            "Create a named node in the open level, at its top or under a parent. Returns {\"entity\": id}.",
            &[
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
                    description: "Parent entity; omit to put it at the top of the level",
                },
            ],
        )
        .example(r#"{"name": "Crate", "parent": "Enemies"}"#),
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
        CommandSpec::edit(
            "entity.despawn",
            "Delete a node and its children (undoable). The level root cannot be deleted.",
            &[ENTITY],
        )
        .example(r#"{"entity": "Player"}"#),
        |world, args| {
            let entity = entity_arg(args, "entity")?;
            Ok(Box::new(DespawnEntity::new(world, entity)))
        },
    );
    registry.register(
        CommandSpec::edit(
            "entity.rename",
            "Rename an entity.",
            &[
                ENTITY,
                ArgSpec {
                    name: "name",
                    kind: ArgKind::String,
                    required: true,
                    description: "The new name",
                },
            ],
        )
        .example(r#"{"entity": "Enemies/Goblin", "name": "Orc"}"#),
        |_, args| {
            Ok(Box::new(RenameEntity::new(
                entity_arg(args, "entity")?,
                string_arg(args, "name")?,
            )))
        },
    );
    registry.register(
        CommandSpec::edit(
            "entity.reparent",
            "Move a node under a new parent, or to the top of the level.",
            &[
                ENTITY,
                ArgSpec {
                    name: "parent",
                    kind: ArgKind::Entity,
                    required: false,
                    description: "New parent; omit or null for the top of the level",
                },
            ],
        )
        .example(r#"{"entity": "Enemies/Goblin", "parent": "Player"}"#),
        |_, args| {
            Ok(Box::new(ReparentEntity::new(
                entity_arg(args, "entity")?,
                opt_entity_arg(args, "parent")?,
            )))
        },
    );
    registry.register(
        CommandSpec::edit(
            "component.set",
            "Set a component field through reflection, e.g. path \"translation.x\".",
            &[
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
        )
        .example(r#"{"entity": "Player", "component": "Health", "path": "current", "value": 3.5}"#),
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
        CommandSpec::edit(
            "component.add",
            "Add a component with its default value.",
            &[ENTITY, COMPONENT],
        )
        .example(r#"{"entity": "Enemies", "component": "Speed"}"#),
        |_, args| {
            Ok(Box::new(AddComponent::new(
                entity_arg(args, "entity")?,
                string_arg(args, "component")?,
            )))
        },
    );
    registry.register(
        CommandSpec::edit(
            "component.remove",
            "Remove a component (undo restores its value).",
            &[ENTITY, COMPONENT],
        )
        .example(r#"{"entity": "Enemies/Goblin", "component": "Health"}"#),
        |_, args| {
            Ok(Box::new(RemoveComponent::new(
                entity_arg(args, "entity")?,
                string_arg(args, "component")?,
            )))
        },
    );
    registry.register(
        CommandSpec::action(
            "selection.set",
            "Select these entities (not an undo step).",
            &[ArgSpec {
                name: "entities",
                kind: ArgKind::Entities,
                required: true,
                description: "Entities to select; empty clears the selection",
            }],
        ),
        |_, args| Ok(Box::new(SetSelection(entities_arg(args, "entities")?))),
    );
    registry.register(
        CommandSpec::edit(
            "batch",
            "Run several commands as one undo step, all or nothing. Returns each one's output, in order.",
            &[
                ArgSpec {
                    name: "commands",
                    kind: ArgKind::Json,
                    required: true,
                    description: "The steps, in order: [{\"id\": \"entity.spawn\", \"args\": {...}}, ...]",
                },
                ArgSpec {
                    name: "label",
                    kind: ArgKind::String,
                    required: false,
                    description: "What the step is called in History",
                },
            ],
        )
        .example(r#"{"label": "Two crates", "commands": [{"id": "entity.spawn", "args": {"name": "Crate"}}, {"id": "entity.spawn", "args": {"name": "Crate2"}}]}"#),
        |world, args| {
            let label = opt_string_arg(args, "label")?.unwrap_or("Batch").to_owned();
            Ok(Box::new(Batch::from_invocations(label, batch_steps(world, args)?)))
        },
    );
    registry.register(
        CommandSpec::action(
            "history.group_begin",
            "Start a group: every change until history.group_end becomes one undo step.",
            &[ArgSpec {
                name: "label",
                kind: ArgKind::String,
                required: true,
                description: "What the step is called in History, e.g. \"Agent: build a wall of crates\"",
            }],
        ),
        |_, args| Ok(Box::new(HistoryGroup::Begin(string_arg(args, "label")?.to_owned()))),
    );
    registry.register(
        CommandSpec::action(
            "history.group_end",
            "End the group, turning its changes into one undo step.",
            &[],
        ),
        |_, _| Ok(Box::new(HistoryGroup::End)),
    );
    registry.register(
        CommandSpec::action("history.undo", "Undo the last change.", &[]),
        |_, _| Ok(Box::new(HistoryStep { redo: false })),
    );
    registry.register(
        CommandSpec::action("history.redo", "Redo the last undone change.", &[]),
        |_, _| Ok(Box::new(HistoryStep { redo: true })),
    );
}

/// The `commands` of a `batch`: registered ids and their arguments.
fn batch_steps(world: &World, args: &Args) -> Result<Vec<Invocation>, CommandError> {
    let bad = |reason: &str| CommandError::BadArgument {
        name: "commands".into(),
        reason: reason.into(),
    };
    let Some(Value::Array(steps)) = args.get("commands") else {
        return Err(bad("expected a list of {\"id\": ..., \"args\": {...}}"));
    };
    if steps.is_empty() {
        return Err(bad("a batch needs at least one command"));
    }
    let registry = world.resource::<CommandRegistry>();
    steps
        .iter()
        .map(|step| {
            let id = step
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| bad("each step needs an \"id\""))?;
            // The registry's own copy of the id lives for the whole program.
            let id = registry
                .spec(id)
                .map(|spec| spec.id)
                .ok_or_else(|| CommandError::UnknownCommand(id.to_owned()))?;
            let args = match step.get("args") {
                None | Some(Value::Null) => Value::Object(Args::new()),
                Some(args @ Value::Object(_)) => args.clone(),
                Some(_) => return Err(bad("a step's \"args\" must be an object")),
            };
            Ok(Invocation::new(id, args))
        })
        .collect()
}
