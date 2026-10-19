//! Oddity Engine's editor core.
//!
//! Every change to the edited world is an [`EditorCommand`]. Running it
//! through [`EditorWorldExt::execute`] records it in the [`History`], so undo
//! and redo work no matter whether the change came from a menu, a shortcut,
//! the inspector, the CLI, MCP or a plugin.
//!
//! Commands can also be run by name with JSON arguments
//! ([`EditorWorldExt::run_command`]); that is how the CLI and MCP drive the
//! editor. Each has a [`CommandKind`]: edits and settings are undoable,
//! actions are not, and queries only read. See [`CommandRegistry`].
//!
//! Every command runs on behalf of a [`Source`] (you, the CLI, an MCP agent,
//! a plugin or a script), which its history step keeps, and is announced
//! afterwards with a [`CommandExecuted`] message.
//!
//! This crate depends only on Bevy's ECS, app and reflection crates, so it
//! runs headless: in tests, in the CLI and behind MCP, with no window.

pub mod command;
pub mod commands;
pub mod history;
pub mod level;
pub mod queries;
pub mod reflect_json;
pub mod registry;
pub mod scripts;
pub mod selection;
pub mod source;
pub mod testing;

use bevy_app::{App, Plugin};
use bevy_ecs::{name::Name, system::Commands, world::World};
use commands::HistoryStep;
use registry::Runner;
use serde_json::Value;

pub use command::{CommandError, CommandExecuted, EditorCommand, Invocation};
pub use history::{History, Step};
pub use level::{LevelRoot, NodeKind, is_in_level, level_root};
pub use registry::{
    ArgKind, ArgSpec, Args, CommandKind, CommandRegistry, CommandSpec, NamedCommand, QueryFn,
};
pub use selection::Selection;
pub use source::Source;

/// Adds the history, selection, command registry and [`CommandExecuted`].
pub struct OddityCorePlugin;

impl Plugin for OddityCorePlugin {
    fn build(&self, app: &mut App) {
        // Kept if an extension registered commands before this plugin.
        if !app.world().contains_resource::<CommandRegistry>() {
            app.insert_resource(CommandRegistry::with_builtins());
        }
        app.init_resource::<History>()
            .init_resource::<Selection>()
            .add_message::<CommandExecuted>()
            .register_type::<Name>()
            .register_type::<LevelRoot>()
            .register_type::<NodeKind>()
            .register_type::<scripts::Scripts>()
            .init_resource::<scripts::ScriptTypes>();
    }
}

/// An app with just the editor core and no window: for tests, the CLI and MCP.
pub fn headless_app() -> App {
    let mut app = App::new();
    app.add_plugins(OddityCorePlugin);
    app
}

/// Running editor commands on a [`World`].
///
/// The plain methods run on behalf of [`Source::User`], since the editor's
/// own UI calls them; the CLI, MCP, plugins and scripts use the `_as` ones.
pub trait EditorWorldExt {
    /// Run a command as the user and record it for undo. Returns its output.
    fn execute(&mut self, command: impl EditorCommand) -> Result<Value, CommandError>;

    /// Run a command on behalf of `source` and record it for undo.
    fn execute_as(
        &mut self,
        command: impl EditorCommand,
        source: Source,
    ) -> Result<Value, CommandError>;

    /// Like [`EditorWorldExt::execute_as`], for an already boxed command.
    fn execute_boxed(
        &mut self,
        command: Box<dyn EditorCommand>,
        source: Source,
    ) -> Result<Value, CommandError>;

    /// Undo the last change, as the user.
    fn undo(&mut self) -> Result<(), CommandError>;

    /// Redo the last undone change, as the user.
    fn redo(&mut self) -> Result<(), CommandError>;

    /// Run a registered command or query by id with JSON arguments (an
    /// object), as the user. Queries read the world and leave the history
    /// untouched.
    fn run_command(&mut self, id: &str, args: Value) -> Result<Value, CommandError>;

    /// Like [`EditorWorldExt::run_command`], on behalf of `source`.
    fn run_command_as(
        &mut self,
        id: &str,
        args: Value,
        source: Source,
    ) -> Result<Value, CommandError>;
}

impl EditorWorldExt for World {
    fn execute(&mut self, command: impl EditorCommand) -> Result<Value, CommandError> {
        self.execute_as(command, Source::User)
    }

    fn execute_as(
        &mut self,
        command: impl EditorCommand,
        source: Source,
    ) -> Result<Value, CommandError> {
        history::execute(self, Box::new(command), source)
    }

    fn execute_boxed(
        &mut self,
        command: Box<dyn EditorCommand>,
        source: Source,
    ) -> Result<Value, CommandError> {
        history::execute(self, command, source)
    }

    // Through the command rather than the history directly, so undo and
    // redo are announced like everything else.
    fn undo(&mut self) -> Result<(), CommandError> {
        self.execute(HistoryStep { redo: false }).map(drop)
    }

    fn redo(&mut self) -> Result<(), CommandError> {
        self.execute(HistoryStep { redo: true }).map(drop)
    }

    fn run_command(&mut self, id: &str, args: Value) -> Result<Value, CommandError> {
        self.run_command_as(id, args, Source::User)
    }

    fn run_command_as(
        &mut self,
        id: &str,
        args: Value,
        source: Source,
    ) -> Result<Value, CommandError> {
        let args = match args {
            Value::Object(map) => map,
            Value::Null => Args::new(),
            _ => {
                return Err(CommandError::BadArgument {
                    name: "args".into(),
                    reason: "expected a JSON object".into(),
                });
            }
        };
        let runner = self
            .get_resource::<CommandRegistry>()
            .and_then(|registry| registry.runner(id))
            .ok_or_else(|| CommandError::UnknownCommand(id.to_owned()))?;
        match runner {
            Runner::Command(build) => {
                let command = build(self, &args)?;
                history::execute(self, command, source)
            }
            Runner::Query(run) => run(self, &args),
        }
    }
}

/// Running editor commands from systems: queued, run at the next sync point.
pub trait EditorCommandsExt {
    /// Queue a command, run as the user. Errors are logged.
    fn execute(&mut self, command: impl EditorCommand);

    /// Queue a command, run on behalf of `source`. Errors are logged.
    fn execute_as(&mut self, command: impl EditorCommand, source: Source);
}

impl EditorCommandsExt for Commands<'_, '_> {
    fn execute(&mut self, command: impl EditorCommand) {
        self.execute_as(command, Source::User);
    }

    fn execute_as(&mut self, command: impl EditorCommand, source: Source) {
        self.queue(move |world: &mut World| {
            if let Err(error) = world.execute_as(command, source) {
                tracing::warn!("command failed: {error}");
            }
        });
    }
}
