//! Undo and redo stacks, and the unsaved-changes flag.
//!
//! Every command runs through [`execute`] here, which records it with its
//! [`Source`] and then announces it with a [`CommandExecuted`] message.
//!
//! One intention is one undo step (`roadmap/commands.md`, rule 5):
//!
//! - during a *gesture* (a drag, a scrub), a command that can merge into the
//!   step before it does ([`History::begin_gesture`]);
//! - a *group* turns every step between its beginning and end into one,
//!   for an agent working over several calls ([`History::begin_group`]).

use std::borrow::Cow;

use bevy_ecs::{message::Messages, resource::Resource, world::World};
use serde_json::{Value, json};

use crate::{
    Source,
    command::{CommandError, CommandExecuted, EditorCommand},
    commands::Batch,
    level::level_root,
};

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
    /// During a gesture: the newest id when it began. Only steps made since
    /// may take merges.
    gesture: Option<u64>,
    /// An open group.
    group: Option<Group>,
}

/// A group being recorded: its label, and the newest id when it began.
struct Group {
    label: String,
    after: u64,
}

struct Entry {
    id: u64,
    command: Box<dyn EditorCommand>,
    source: Source,
}

impl Entry {
    fn step(&self) -> Step<'_> {
        Step {
            label: self.command.label(),
            source: &self.source,
        }
    }
}

/// One history step, as the History dock and `history.list` show it.
#[derive(Clone, Debug, PartialEq)]
pub struct Step<'a> {
    /// What the step did ("Rename to Hero").
    pub label: Cow<'a, str>,
    /// Who did it.
    pub source: &'a Source,
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
            gesture: None,
            group: None,
        }
    }

    /// Start a gesture, such as dragging a gizmo or scrubbing a number:
    /// until [`History::end_gesture`], each command that can merge into the
    /// step before it does, so the whole gesture is one undo step. The UI
    /// ends it when the mouse is released. Undo and redo end it too.
    pub fn begin_gesture(&mut self) {
        self.gesture = Some(self.current());
    }

    /// End the gesture. The next command starts a new step.
    pub fn end_gesture(&mut self) {
        self.gesture = None;
    }

    /// Whether a gesture is going on.
    pub fn in_gesture(&self) -> bool {
        self.gesture.is_some()
    }

    /// Start a group labelled `label`: every step recorded until
    /// [`History::end_group`] becomes one step. This is how an agent makes
    /// several tool calls one undo step ("Agent: build a wall of crates").
    pub fn begin_group(&mut self, label: impl Into<String>) -> Result<(), CommandError> {
        if self.group.is_some() {
            return Err(CommandError::Invalid(
                "a group is already open; end it first".into(),
            ));
        }
        self.group = Some(Group {
            label: label.into(),
            after: self.current(),
        });
        Ok(())
    }

    /// End the open group, turning its steps into one. Steps undone during
    /// the group are left out.
    pub fn end_group(&mut self) -> Result<(), CommandError> {
        let group = self
            .group
            .take()
            .ok_or_else(|| CommandError::Invalid("no group is open".into()))?;
        // Ids only grow along the undo stack, so this survives trimming.
        let start = self.undo.partition_point(|entry| entry.id <= group.after);
        let steps: Vec<Entry> = self.undo.drain(start..).collect();
        let Some(source) = steps.first().map(|entry| entry.source.clone()) else {
            return Ok(());
        };
        let last = steps.last().map_or(0, |entry| entry.id);
        let id = self.next_id;
        self.next_id += 1;
        // States inside the group can't be reached any more; its end can.
        if self.saved == last {
            self.saved = id;
        } else if steps.iter().any(|entry| entry.id == self.saved) {
            self.saved = UNREACHABLE;
        }
        let mut batch = Batch::new(group.label);
        for entry in steps {
            batch.push(entry.command);
        }
        self.undo.push(Entry {
            id,
            command: Box::new(batch),
            source,
        });
        Ok(())
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

    /// Every undo step, oldest first.
    pub fn undo_steps(&self) -> impl Iterator<Item = Step<'_>> {
        self.undo.iter().map(Entry::step)
    }

    /// Every redo step, in the order Redo would re-apply them.
    pub fn redo_steps(&self) -> impl Iterator<Item = Step<'_>> {
        self.redo.iter().rev().map(Entry::step)
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

    /// The history as JSON, for the `history.list` query: undo steps oldest
    /// first, redo steps in the order Redo would re-apply them, each with its
    /// label, source and invocation, and the unsaved-changes flag.
    pub fn to_json(&self) -> Value {
        let step = |entry: &Entry| {
            json!({
                "label": entry.command.label(),
                "source": entry.source.to_json(),
                "invocation": entry.command.invocation().to_json(),
            })
        };
        json!({
            "undo": self.undo.iter().map(step).collect::<Vec<_>>(),
            "redo": self.redo.iter().rev().map(step).collect::<Vec<_>>(),
            "dirty": self.is_dirty(),
        })
    }

    fn current(&self) -> u64 {
        self.undo.last().map_or(0, |e| e.id)
    }
}

/// Run a command, record it if it is undoable, and announce it with
/// [`CommandExecuted`]. Returns the command's output.
pub(crate) fn execute(
    world: &mut World,
    mut command: Box<dyn EditorCommand>,
    source: Source,
) -> Result<Value, CommandError> {
    world.init_resource::<History>();
    let document = level_root(world);
    let result = command.apply(world).map(|()| command.output());
    let invocation = command.invocation();
    match result {
        // The command never reaches the history, so this is its only chance
        // to clean up (a spawn's entity is created before it runs).
        Err(_) => command.discard(world, false),
        Ok(_) if command.undoable() => record(world, command, source.clone()),
        Ok(_) => {}
    }
    // After recording, so listeners read the history as it now is. A bare
    // World without OddityCorePlugin has no listeners to tell.
    if world.contains_resource::<Messages<CommandExecuted>>() {
        world.write_message(CommandExecuted {
            id: invocation.id,
            args: invocation.args,
            source,
            document,
            result: result.clone(),
        });
    }
    result
}

fn record(world: &mut World, command: Box<dyn EditorCommand>, source: Source) {
    world.resource_scope(
        |world, mut history: bevy_ecs::change_detection::Mut<History>| {
            if let Some(merged) = merge_into_newest(&mut history, &*command, &source) {
                // The step now ends somewhere new, not where it was saved.
                if history.saved == merged {
                    history.saved = UNREACHABLE;
                }
                return;
            }

            // A new change makes the redo steps unreachable.
            let dropped = std::mem::take(&mut history.redo);
            for mut entry in dropped {
                if entry.id == history.saved {
                    history.saved = UNREACHABLE;
                }
                entry.command.discard(world, false);
            }

            let id = history.next_id;
            history.next_id += 1;
            history.undo.push(Entry {
                id,
                command,
                source,
            });

            while history.undo.len() > history.limit {
                let mut oldest = history.undo.remove(0);
                // An empty undo stack now means "after `oldest`", and the
                // state before it can no longer be reached.
                if history.saved == 0 {
                    history.saved = UNREACHABLE;
                } else if history.saved == oldest.id {
                    history.saved = 0;
                }
                oldest.command.discard(world, true);
            }
        },
    );
}

/// During a gesture, fold `command` into the newest step if that step was
/// made during the gesture by the same source and accepts it. Returns the
/// step's id.
fn merge_into_newest(
    history: &mut History,
    command: &dyn EditorCommand,
    source: &Source,
) -> Option<u64> {
    let after = history.gesture?;
    if !history.redo.is_empty() {
        return None;
    }
    let newest = history.undo.last_mut()?;
    (newest.id > after && newest.source == *source && newest.command.merge(command))
        .then_some(newest.id)
}

/// Reverse the newest step.
pub(crate) fn undo(world: &mut World) -> Result<(), CommandError> {
    world.init_resource::<History>();
    world.resource_scope(
        |world, mut history: bevy_ecs::change_detection::Mut<History>| {
            history.gesture = None;
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
            history.gesture = None;
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
