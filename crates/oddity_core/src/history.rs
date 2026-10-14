//! Undo and redo stacks, and the unsaved-changes flag.

use std::borrow::Cow;

use bevy_ecs::{resource::Resource, world::World};
use serde_json::Value;

use crate::command::{CommandError, EditorCommand};

/// How many steps are kept before the oldest is dropped.
pub const DEFAULT_LIMIT: usize = 500;

/// No history position matches the saved state any more.
const UNREACHABLE: u64 = u64::MAX;

/// The undo/redo history of the edited world.
#[derive(Resource)]
pub struct History {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    limit: usize,
    next_id: u64,
    /// Id of the newest undo entry when the project was last saved (0 = empty).
    saved: u64,
}

struct Entry {
    id: u64,
    command: Box<dyn EditorCommand>,
}

impl Default for History {
    fn default() -> Self {
        Self::with_limit(DEFAULT_LIMIT)
    }
}

impl History {
    /// A history that keeps at most `limit` undo steps.
    pub fn with_limit(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: limit.max(1),
            next_id: 1,
            saved: 0,
        }
    }

    /// Whether there is something to undo.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether there is something to redo.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Label of the step Undo would reverse.
    pub fn undo_label(&self) -> Option<Cow<'_, str>> {
        self.undo.last().map(|e| e.command.label())
    }

    /// Label of the step Redo would re-apply.
    pub fn redo_label(&self) -> Option<Cow<'_, str>> {
        self.redo.last().map(|e| e.command.label())
    }

    /// Labels of every undo step, oldest first.
    pub fn undo_labels(&self) -> impl Iterator<Item = Cow<'_, str>> {
        self.undo.iter().map(|e| e.command.label())
    }

    /// Labels of every redo step, in the order Redo would re-apply them.
    pub fn redo_labels(&self) -> impl Iterator<Item = Cow<'_, str>> {
        self.redo.iter().rev().map(|e| e.command.label())
    }

    /// Number of undo steps.
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    /// Number of redo steps.
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// Whether the world has changed since [`History::mark_saved`].
    pub fn is_dirty(&self) -> bool {
        self.current() != self.saved
    }

    /// Record that the current state has been saved to disk.
    pub fn mark_saved(&mut self) {
        self.saved = self.current();
    }

    fn current(&self) -> u64 {
        self.undo.last().map_or(0, |e| e.id)
    }
}

/// Run a command and record it. Returns the command's output.
pub(crate) fn execute(
    world: &mut World,
    mut command: Box<dyn EditorCommand>,
) -> Result<Value, CommandError> {
    world.init_resource::<History>();
    command.apply(world)?;
    let output = command.output();
    if !command.undoable() {
        return Ok(output);
    }

    world.resource_scope(
        |world, mut history: bevy_ecs::change_detection::Mut<History>| {
            // A new change makes the redo steps unreachable.
            let dropped: Vec<Entry> = history.redo.drain(..).collect();
            for mut entry in dropped {
                if entry.id == history.saved {
                    history.saved = UNREACHABLE;
                }
                entry.command.discard(world, false);
            }

            let id = history.next_id;
            history.next_id += 1;
            history.undo.push(Entry { id, command });

            while history.undo.len() > history.limit {
                let mut oldest = history.undo.remove(0);
                if oldest.id == history.saved {
                    history.saved = UNREACHABLE;
                }
                oldest.command.discard(world, true);
            }
        },
    );
    Ok(output)
}

/// Reverse the newest step.
pub(crate) fn undo(world: &mut World) -> Result<(), CommandError> {
    world.init_resource::<History>();
    world.resource_scope(
        |world, mut history: bevy_ecs::change_detection::Mut<History>| {
            let mut entry = history.undo.pop().ok_or(CommandError::NothingToUndo)?;
            if let Err(error) = entry.command.undo(world) {
                history.undo.push(entry);
                return Err(error);
            }
            history.redo.push(entry);
            Ok(())
        },
    )
}

/// Re-apply the newest undone step.
pub(crate) fn redo(world: &mut World) -> Result<(), CommandError> {
    world.init_resource::<History>();
    world.resource_scope(
        |world, mut history: bevy_ecs::change_detection::Mut<History>| {
            let mut entry = history.redo.pop().ok_or(CommandError::NothingToRedo)?;
            if let Err(error) = entry.command.apply(world) {
                history.redo.push(entry);
                return Err(error);
            }
            history.undo.push(entry);
            Ok(())
        },
    )
}
