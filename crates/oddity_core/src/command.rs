//! The [`EditorCommand`] trait: every change to a project goes through one.
//! [`CommandExecuted`] announces each one after it runs.

use std::{any::Any, borrow::Cow};

use bevy_ecs::{entity::Entity, message::Message, world::World};
use serde_json::{Value, json};

use crate::{Args, Source};

/// Why a command could not run.
#[derive(Clone, Debug, thiserror::Error)]
pub enum CommandError {
    /// The entity does not exist (or was deleted).
    #[error("entity {0} does not exist")]
    NoEntity(Entity),
    /// The entity exists but is not part of the open level (editor UI, say).
    #[error("entity {0} is not part of the open level")]
    NotInLevel(Entity),
    /// The command needs an open level and none is open.
    #[error("no level is open")]
    NoLevel,
    /// No command with this id is registered.
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    /// No reflected type with this type path is registered.
    #[error("unknown component type `{0}`")]
    UnknownComponent(String),
    /// The entity does not have the component.
    #[error("entity {entity} has no `{component}` component")]
    MissingComponent {
        /// The entity.
        entity: Entity,
        /// The component's type path.
        component: String,
    },
    /// The entity already has the component.
    #[error("entity {entity} already has a `{component}` component")]
    AlreadyHasComponent {
        /// The entity.
        entity: Entity,
        /// The component's type path.
        component: String,
    },
    /// The type is registered but lacks the reflection data needed.
    #[error("`{0}` cannot be edited: register it with #[reflect(Component, Default)]")]
    NotReflectable(String),
    /// A field path such as `translation.x` does not exist.
    #[error("invalid field path `{path}`: {reason}")]
    BadPath {
        /// The path that was asked for.
        path: String,
        /// What went wrong.
        reason: String,
    },
    /// A value has the wrong shape or type.
    #[error("invalid value: {0}")]
    BadValue(String),
    /// A command argument is missing.
    #[error("missing argument `{0}`")]
    MissingArgument(String),
    /// A command argument has the wrong type.
    #[error("invalid argument `{name}`: {reason}")]
    BadArgument {
        /// The argument.
        name: String,
        /// What went wrong.
        reason: String,
    },
    /// The change would not make sense, e.g. parenting a node to its own child.
    #[error("{0}")]
    Invalid(String),
    /// The undo stack is empty.
    #[error("nothing to undo")]
    NothingToUndo,
    /// The redo stack is empty.
    #[error("nothing to redo")]
    NothingToRedo,
}

/// The registered command id and JSON arguments that recreate a command.
///
/// Every history step can be shown, replayed or turned into a script this
/// way, however it was started (`roadmap/commands.md`, rule 4).
#[derive(Clone, Debug, PartialEq)]
pub struct Invocation {
    /// The command's id in the [`CommandRegistry`](crate::CommandRegistry).
    pub id: &'static str,
    /// Its arguments, as `run_command` takes them.
    pub args: Args,
}

impl Invocation {
    /// An invocation of `id` with `args`, which must be a JSON object.
    pub fn new(id: &'static str, args: Value) -> Self {
        let args = match args {
            Value::Object(map) => map,
            _ => Args::new(),
        };
        Self { id, args }
    }

    /// `{"id": ..., "args": {...}}`, the shape of a step in a command script.
    pub fn to_json(&self) -> Value {
        json!({ "id": self.id, "args": self.args })
    }
}

/// Sent after every command runs, whether it worked or not, and whoever
/// started it. The History dock, the unsaved-changes dots, MCP notifications
/// and script recording listen to this instead of each hooking into the
/// editor. Queries change nothing, so they are not announced.
#[derive(Message, Clone, Debug)]
pub struct CommandExecuted {
    /// The command's id, as in its [`Invocation`].
    pub id: &'static str,
    /// Its arguments, as in its [`Invocation`].
    pub args: Args,
    /// Who started it.
    pub source: Source,
    /// The root of the level that was open when it ran. Until each document
    /// gets its own history (task K5), every command acts on that level.
    pub document: Option<Entity>,
    /// Its output, or why it failed.
    pub result: Result<Value, CommandError>,
}

/// One undoable change to the edited world.
///
/// Menus, shortcuts, the inspector, the CLI, MCP and plugins all change the
/// project through commands, so undo/redo works everywhere for free.
///
/// `apply` runs once when the command is executed and again on every redo,
/// so it must work when called after `undo`.
///
/// `Any` lets [`EditorCommand::merge`] recognise a command of its own type.
pub trait EditorCommand: Any + Send + Sync {
    /// Short name shown in the history ("Rename Player", "Delete Enemy").
    fn label(&self) -> Cow<'_, str>;

    /// Make the change.
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError>;

    /// Reverse the change.
    fn undo(&mut self, world: &mut World) -> Result<(), CommandError>;

    /// The id and arguments that would run this same command by name.
    /// Required, so no command can be left out of scripts and replays.
    fn invocation(&self) -> Invocation;

    /// Whether the command goes on the undo stack. Selection changes and
    /// undo/redo themselves do not.
    fn undoable(&self) -> bool {
        true
    }

    /// Fold `next`, which has just been applied, into this step, so a
    /// continuous edit (dragging a gizmo, scrubbing a number) is one undo
    /// step. Return `true` if it was folded in; `next` is then dropped, and
    /// this step's undo must now reverse both. Only asked during a gesture
    /// ([`History::begin_gesture`](crate::History::begin_gesture)), and only
    /// for steps from the same source.
    fn merge(&mut self, _next: &dyn EditorCommand) -> bool {
        false
    }

    /// The command's result for the CLI and MCP, e.g. `{"entity": 42}`.
    /// Called right after the first `apply`.
    fn output(&self) -> Value {
        Value::Null
    }

    /// Called when the command leaves the history for good (the redo stack
    /// is cleared, or the undo stack is trimmed), and when its first `apply`
    /// fails, with `applied` false. `applied` says which state it was left
    /// in. Commands that keep entities around for undo despawn them here.
    fn discard(&mut self, _world: &mut World, _applied: bool) {}
}
