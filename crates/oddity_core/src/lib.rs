//! Oddity Engine's editor core.
//!
//! Every change to the edited world is an [`EditorCommand`]. Running it
//! through [`EditorWorldExt::execute`] records it in the [`History`], so undo
//! and redo work no matter whether the change came from a menu, a shortcut,
//! the inspector, the CLI, MCP or a plugin.
//!
//! Commands can also be run by name with JSON arguments
//! ([`EditorWorldExt::run_command`]); that is how the CLI and MCP drive the
//! editor. See [`CommandRegistry`].
//!
//! This crate depends only on Bevy's ECS, app and reflection crates, so it
//! runs headless: in tests, in the CLI and behind MCP, with no window.

pub mod command;
pub mod commands;
pub mod history;
pub mod level;
pub mod reflect_json;
pub mod registry;
pub mod selection;

use bevy_app::{App, Plugin};
use bevy_ecs::{name::Name, system::Commands, world::World};
use serde_json::Value;

pub use command::{CommandError, EditorCommand};
pub use history::History;
pub use level::{LevelRoot, is_in_level, level_root};
pub use registry::{ArgKind, ArgSpec, Args, CommandRegistry, CommandSpec};
pub use selection::Selection;

/// Adds the history, selection and command registry.
pub struct OddityCorePlugin;

impl Plugin for OddityCorePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<History>()
            .init_resource::<Selection>()
            .insert_resource(CommandRegistry::with_builtins())
            .register_type::<Name>()
            .register_type::<LevelRoot>();
    }
}

/// An app with just the editor core and no window: for tests, the CLI and MCP.
pub fn headless_app() -> App {
    let mut app = App::new();
    app.add_plugins(OddityCorePlugin);
    app
}

/// Running editor commands on a [`World`].
pub trait EditorWorldExt {
    /// Run a command and record it for undo. Returns its output.
    fn execute(&mut self, command: impl EditorCommand) -> Result<Value, CommandError>;

    /// Like [`EditorWorldExt::execute`], for an already boxed command.
    fn execute_boxed(&mut self, command: Box<dyn EditorCommand>) -> Result<Value, CommandError>;

    /// Undo the last change.
    fn undo(&mut self) -> Result<(), CommandError>;

    /// Redo the last undone change.
    fn redo(&mut self) -> Result<(), CommandError>;

    /// Run a registered command by id with JSON arguments (an object).
    fn run_command(&mut self, id: &str, args: Value) -> Result<Value, CommandError>;
}

impl EditorWorldExt for World {
    fn execute(&mut self, command: impl EditorCommand) -> Result<Value, CommandError> {
        history::execute(self, Box::new(command))
    }

    fn execute_boxed(&mut self, command: Box<dyn EditorCommand>) -> Result<Value, CommandError> {
        history::execute(self, command)
    }

    fn undo(&mut self) -> Result<(), CommandError> {
        history::undo(self)
    }

    fn redo(&mut self) -> Result<(), CommandError> {
        history::redo(self)
    }

    fn run_command(&mut self, id: &str, args: Value) -> Result<Value, CommandError> {
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
        let build = self
            .get_resource::<CommandRegistry>()
            .and_then(|registry| registry.build_fn(id))
            .ok_or_else(|| CommandError::UnknownCommand(id.to_owned()))?;
        let command = build(self, &args)?;
        history::execute(self, command)
    }
}

/// Running editor commands from systems: queued, run at the next sync point.
pub trait EditorCommandsExt {
    /// Queue a command. Errors are logged.
    fn execute(&mut self, command: impl EditorCommand);
}

impl EditorCommandsExt for Commands<'_, '_> {
    fn execute(&mut self, command: impl EditorCommand) {
        self.queue(move |world: &mut World| {
            if let Err(error) = world.execute(command) {
                tracing::warn!("command failed: {error}");
            }
        });
    }
}
