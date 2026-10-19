//! Actions on the editor's own state: the viewport's tools, distraction-free
//! mode, the play controls. Each is a registered command
//! (`roadmap/commands.md`, rule 1), so a menu, a shortcut, MCP or a script
//! can do what its button does. They change no document, so they aren't
//! undo steps.
//!
//! ```ignore
//! struct ToggleSnap;
//!
//! impl UiAction for ToggleSnap {
//!     const SPEC: CommandSpec = CommandSpec::action("viewport.toggle_snap", "...", &[]);
//!     fn run(world: &mut World, _args: &Args) -> Result<(), CommandError> { .. }
//! }
//!
//! app.register_command::<Run<ToggleSnap>>();
//! ```

use std::{borrow::Cow, marker::PhantomData};

use bevy::prelude::*;
use oddity_core::{
    Args, CommandError, CommandSpec, EditorCommand, EditorWorldExt, Invocation, NamedCommand,
};
use serde_json::Value;

/// One action: its spec, and what it does.
pub trait UiAction: Send + Sync + 'static {
    /// Its id, description and arguments. An Action-kind spec.
    const SPEC: CommandSpec;

    /// Do it, with the arguments it was run with.
    fn run(world: &mut World, args: &Args) -> Result<(), CommandError>;
}

/// The command that runs the action `A`.
pub struct Run<A> {
    args: Args,
    action: PhantomData<fn() -> A>,
}

impl<A: UiAction> EditorCommand for Run<A> {
    fn label(&self) -> Cow<'_, str> {
        Cow::Borrowed(A::SPEC.id)
    }

    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        A::run(world, &self.args)
    }

    fn undo(&mut self, _world: &mut World) -> Result<(), CommandError> {
        Ok(())
    }

    fn invocation(&self) -> Invocation {
        Invocation::new(A::SPEC.id, Value::Object(self.args.clone()))
    }

    fn undoable(&self) -> bool {
        false
    }
}

impl<A: UiAction> NamedCommand for Run<A> {
    const SPEC: CommandSpec = A::SPEC;

    fn from_args(_world: &mut World, args: &Args) -> Result<Self, CommandError> {
        Ok(Self {
            args: args.clone(),
            action: PhantomData,
        })
    }
}

/// Run a command by id as the user, from a button or shortcut, at the next
/// sync point. Problems are logged: the user can't act on them.
pub fn run_command(commands: &mut Commands, id: &'static str, args: Value) {
    commands.queue(move |world: &mut World| {
        if let Err(error) = world.run_command(id, args) {
            warn!("{id}: {error}");
        }
    });
}

/// A resource the action changes, or an error saying the editor has no
/// window (the state these actions change only exists with one).
pub fn ui_state<'w, R: Resource<Mutability = bevy::ecs::component::Mutable>>(
    world: &'w mut World,
    id: &str,
) -> Result<Mut<'w, R>, CommandError> {
    world
        .get_resource_mut::<R>()
        .ok_or_else(|| CommandError::Invalid(format!("{id} needs the editor window")))
}
