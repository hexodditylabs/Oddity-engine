//! The [`EditorCommand`] trait: every change to a project goes through one.

use std::borrow::Cow;

use bevy_ecs::{entity::Entity, world::World};
use serde_json::Value;

/// Why a command could not run.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    /// The entity does not exist (or was deleted).
    #[error("entity {0} does not exist")]
    NoEntity(Entity),
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

/// One undoable change to the edited world.
///
/// Menus, shortcuts, the inspector, the CLI, MCP and plugins all change the
/// project through commands, so undo/redo works everywhere for free.
///
/// `apply` runs once when the command is executed and again on every redo,
/// so it must work when called after `undo`.
pub trait EditorCommand: Send + Sync + 'static {
    /// Short name shown in the history ("Rename Player", "Delete Enemy").
    fn label(&self) -> Cow<'_, str>;

    /// Make the change.
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError>;

    /// Reverse the change.
    fn undo(&mut self, world: &mut World) -> Result<(), CommandError>;

    /// Whether the command goes on the undo stack. Selection changes and
    /// undo/redo themselves do not.
    fn undoable(&self) -> bool {
        true
    }

    /// The command's result for the CLI and MCP, e.g. `{"entity": 42}`.
    /// Called right after the first `apply`.
    fn output(&self) -> Value {
        Value::Null
    }

    /// Called when the command leaves the history for good (the redo stack
    /// is cleared, or the undo stack is trimmed). `applied` says which state
    /// it was left in. Commands that keep deleted entities around for undo
    /// despawn them here.
    fn discard(&mut self, _world: &mut World, _applied: bool) {}
}
