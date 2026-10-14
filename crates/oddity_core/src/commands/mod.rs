//! Built-in editor commands.

mod component;
mod entity;

use std::borrow::Cow;

use bevy_ecs::{entity::Entity, world::World};

pub(crate) use component::reflect_component;
pub use component::{AddComponent, RemoveComponent, SetField};
pub use entity::{DespawnEntity, RenameEntity, ReparentEntity, SpawnEntity};

use crate::{CommandError, EditorCommand, Selection, history};

/// Several commands as one undo step, e.g. deleting five selected nodes.
/// If one fails, the ones before it are undone and nothing changes.
pub struct Batch {
    label: String,
    commands: Vec<Box<dyn EditorCommand>>,
}

impl Batch {
    /// An empty batch with a label for the history.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            commands: Vec::new(),
        }
    }

    /// Add a command to the batch.
    pub fn with(mut self, command: impl EditorCommand) -> Self {
        self.commands.push(Box::new(command));
        self
    }

    /// Add a boxed command to the batch.
    pub fn push(&mut self, command: Box<dyn EditorCommand>) {
        self.commands.push(command);
    }

    /// Whether the batch has no commands.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

impl EditorCommand for Batch {
    fn label(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.label)
    }

    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        for i in 0..self.commands.len() {
            if let Err(error) = self.commands[i].apply(world) {
                for done in self.commands[..i].iter_mut().rev() {
                    let _ = done.undo(world);
                }
                return Err(error);
            }
        }
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        for command in self.commands.iter_mut().rev() {
            command.undo(world)?;
        }
        Ok(())
    }

    fn output(&self) -> serde_json::Value {
        serde_json::Value::Array(self.commands.iter().map(|c| c.output()).collect())
    }

    fn discard(&mut self, world: &mut World, applied: bool) {
        for command in &mut self.commands {
            command.discard(world, applied);
        }
    }
}

/// Replace the selection. Not an undo step, like in Godot.
pub struct SetSelection(pub Vec<Entity>);

impl EditorCommand for SetSelection {
    fn label(&self) -> Cow<'_, str> {
        Cow::Borrowed("Select")
    }

    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        for entity in &self.0 {
            if world.get_entity(*entity).is_err() {
                return Err(CommandError::NoEntity(*entity));
            }
        }
        world
            .get_resource_or_init::<Selection>()
            .set(self.0.iter().copied());
        Ok(())
    }

    fn undo(&mut self, _world: &mut World) -> Result<(), CommandError> {
        Ok(())
    }

    fn undoable(&self) -> bool {
        false
    }
}

/// Undo or redo, as a command, so the CLI and MCP can trigger them.
pub struct HistoryStep {
    /// `true` for redo, `false` for undo.
    pub redo: bool,
}

impl EditorCommand for HistoryStep {
    fn label(&self) -> Cow<'_, str> {
        Cow::Borrowed(if self.redo { "Redo" } else { "Undo" })
    }

    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        if self.redo {
            history::redo(world)
        } else {
            history::undo(world)
        }
    }

    fn undo(&mut self, _world: &mut World) -> Result<(), CommandError> {
        Ok(())
    }

    fn undoable(&self) -> bool {
        false
    }
}
