//! Built-in editor commands.

mod component;
mod entity;

use std::borrow::Cow;

use bevy_ecs::{entity::Entity, world::World};

pub(crate) use component::reflect_component;
pub use component::{AddComponent, RemoveComponent, SetField};
pub use entity::{DespawnEntity, RenameEntity, ReparentEntity, SpawnEntity};

use serde_json::{Value, json};

use crate::{
    CommandError, CommandRegistry, EditorCommand, History, Invocation, Selection, history,
    level::require_in_level, registry::Runner,
};

/// Several commands as one undo step, e.g. deleting five selected nodes.
/// If one fails, the ones before it are undone and nothing changes.
pub struct Batch {
    label: String,
    commands: Vec<Box<dyn EditorCommand>>,
    /// Steps given by id and arguments (the `batch` command), built and
    /// applied one at a time on the first apply, so a step can use what the
    /// steps before it made. Empty once they have run.
    pending: Vec<Invocation>,
}

impl Batch {
    /// An empty batch with a label for the history.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            commands: Vec::new(),
            pending: Vec::new(),
        }
    }

    /// A batch of registered commands, by id and arguments, as the `batch`
    /// command runs them. Each must be an undoable command, not a query.
    pub fn from_invocations(label: impl Into<String>, steps: Vec<Invocation>) -> Self {
        Self {
            pending: steps,
            ..Self::new(label)
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
        self.commands.is_empty() && self.pending.is_empty()
    }

    fn apply_pending(&mut self, world: &mut World) -> Result<(), CommandError> {
        let mut done: Vec<Box<dyn EditorCommand>> = Vec::new();
        for step in &self.pending {
            match run_step(world, step) {
                Ok(command) => done.push(command),
                Err(error) => {
                    for mut command in done.into_iter().rev() {
                        let _ = command.undo(world);
                        command.discard(world, false);
                    }
                    return Err(error);
                }
            }
        }
        self.commands = done;
        self.pending.clear();
        Ok(())
    }
}

/// Build one step of a `batch` by name and apply it.
fn run_step(world: &mut World, step: &Invocation) -> Result<Box<dyn EditorCommand>, CommandError> {
    let build = match world
        .get_resource::<CommandRegistry>()
        .and_then(|registry| registry.runner(step.id))
    {
        Some(Runner::Command(build)) => build,
        Some(Runner::Query(_)) => {
            return Err(CommandError::Invalid(format!(
                "`{}` is a query; a batch only runs commands",
                step.id
            )));
        }
        None => return Err(CommandError::UnknownCommand(step.id.to_owned())),
    };
    let mut command = build(world, &step.args)?;
    if !command.undoable() {
        command.discard(world, false);
        return Err(CommandError::Invalid(format!(
            "`{}` can't be undone, so it can't be part of a batch",
            step.id
        )));
    }
    if let Err(error) = command.apply(world) {
        command.discard(world, false);
        return Err(error);
    }
    Ok(command)
}

impl EditorCommand for Batch {
    fn label(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.label)
    }

    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        if !self.pending.is_empty() {
            return self.apply_pending(world);
        }
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

    /// A `batch` of its commands' invocations, or of the steps it was asked
    /// to run if they haven't run.
    fn invocation(&self) -> Invocation {
        let commands: Vec<Value> = if self.pending.is_empty() {
            self.commands
                .iter()
                .map(|command| command.invocation().to_json())
                .collect()
        } else {
            self.pending.iter().map(Invocation::to_json).collect()
        };
        Invocation::new(
            "batch",
            json!({ "label": self.label, "commands": commands }),
        )
    }

    fn output(&self) -> Value {
        Value::Array(self.commands.iter().map(|c| c.output()).collect())
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
            require_in_level(world, *entity)?;
        }
        world
            .get_resource_or_init::<Selection>()
            .set(self.0.iter().copied());
        Ok(())
    }

    fn undo(&mut self, _world: &mut World) -> Result<(), CommandError> {
        Ok(())
    }

    fn invocation(&self) -> Invocation {
        let entities: Vec<u64> = self.0.iter().map(|e| e.to_bits()).collect();
        Invocation::new("selection.set", json!({ "entities": entities }))
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

    fn invocation(&self) -> Invocation {
        let id = if self.redo {
            "history.redo"
        } else {
            "history.undo"
        };
        Invocation::new(id, json!({}))
    }

    fn undoable(&self) -> bool {
        false
    }
}

/// Open or close a history group by name, so an agent's work over several
/// calls is one undo step. Not a step itself.
pub enum HistoryGroup {
    /// Start a group with this label.
    Begin(String),
    /// Turn everything since `Begin` into one step.
    End,
}

impl EditorCommand for HistoryGroup {
    fn label(&self) -> Cow<'_, str> {
        Cow::Borrowed(match self {
            Self::Begin(_) => "Begin group",
            Self::End => "End group",
        })
    }

    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        let mut history = world.get_resource_or_init::<History>();
        match self {
            Self::Begin(label) => history.begin_group(label.clone()),
            Self::End => history.end_group(),
        }
    }

    fn undo(&mut self, _world: &mut World) -> Result<(), CommandError> {
        Ok(())
    }

    fn invocation(&self) -> Invocation {
        match self {
            Self::Begin(label) => Invocation::new("history.group_begin", json!({ "label": label })),
            Self::End => Invocation::new("history.group_end", json!({})),
        }
    }

    fn undoable(&self) -> bool {
        false
    }
}
