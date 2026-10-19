//! The game program (`roadmap/scripting.md`, "The game program"): what a
//! game's three-line runner calls.
//!
//! ```ignore
//! fn main() -> AppExit {
//!     oddity::run::<my_game::GamePlugin>()
//! }
//! ```
//!
//! - no flags: play the project's `[editor] main_level`;
//! - `--oddity-play <level>`: play that level (a path in the project);
//! - `--oddity-describe`: print the game's type description and exit.
//!
//! Other arguments are left for the game.

use std::path::PathBuf;

use bevy::{asset::AssetPlugin, prelude::*, window::WindowPlugin};
use oddity_level::{Level, LevelPlugin, spawn_level};
use oddity_project::Project;
use oddity_script::ScriptPlugin;

use crate::{describe::describe, input::input_map};

/// What the command line asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// `--oddity-play <level>`.
    pub play: Option<String>,
    /// `--oddity-describe`.
    pub describe: bool,
}

impl Options {
    /// Read the `--oddity-*` flags from `args` (without the program name),
    /// ignoring the rest.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--oddity-play" => {
                    options.play =
                        Some(args.next().ok_or(
                            "--oddity-play needs a level: --oddity-play levels/main.level",
                        )?);
                }
                "--oddity-describe" => options.describe = true,
                other if other.starts_with("--oddity-") => {
                    return Err(format!(
                        "unknown flag {other}; the flags are --oddity-play <level> and --oddity-describe"
                    ));
                }
                _ => {}
            }
        }
        Ok(options)
    }
}

/// Run the game with plugin `G`: the window, the scripts, the project's
/// input map, and the level the command line names.
pub fn run<G: Plugin + Default>() -> AppExit {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(error) => return fail(&error),
    };
    if options.describe {
        let mut app = App::new();
        app.add_plugins((ScriptPlugin, G::default()));
        let registry = app.world().resource::<AppTypeRegistry>().read();
        println!(
            "{}",
            serde_json::to_string_pretty(&describe(&registry)).expect("JSON values serialize")
        );
        return AppExit::Success;
    }
    let project = match Project::find(".") {
        Ok(project) => project,
        Err(error) => return fail(&error.to_string()),
    };
    let Some(level) = options.play.or(project.file.editor.main_level.clone()) else {
        return fail(
            "no level to play: pass --oddity-play <level>, or set [editor] main_level in oddity.toml",
        );
    };
    let input = match input_map(&project.file.input) {
        Ok(input) => input,
        Err(error) => return fail(&error.to_string()),
    };
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: project.name().to_owned(),
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    // Asset paths are relative to the project, as levels
                    // write them.
                    file_path: project.root.display().to_string(),
                    ..default()
                }),
        )
        .add_plugins((ScriptPlugin, LevelPlugin::new(project.root.clone())))
        .insert_resource(input)
        .insert_resource(Start(project.root.join(level)))
        .add_plugins(G::default())
        .add_systems(Startup, start)
        .run()
}

fn fail(error: &str) -> AppExit {
    eprintln!("error: {error}");
    AppExit::error()
}

/// The level file to start on.
#[derive(Resource)]
struct Start(PathBuf);

/// Spawn the level, and look through its first camera: levels save their
/// cameras switched off, so the editor's viewport isn't covered by them.
fn start(world: &mut World) {
    let path = world.resource::<Start>().0.clone();
    let root = Level::read(&path).and_then(|level| spawn_level(world, &level));
    let root = match root {
        Ok(root) => root,
        Err(error) => {
            error!("can't play {}: {error}", path.display());
            world.write_message(AppExit::error());
            return;
        }
    };
    let mut cameras = world.query::<&Camera>();
    if cameras.iter(world).any(|camera| camera.is_active) {
        return;
    }
    if let Some(first) = first_camera(world, root)
        && let Some(mut camera) = world.get_mut::<Camera>(first)
    {
        camera.is_active = true;
    }
}

/// The first node with a camera, in Hierarchy order.
fn first_camera(world: &World, node: Entity) -> Option<Entity> {
    if world.get::<Camera>(node).is_some() {
        return Some(node);
    }
    world
        .get::<Children>(node)?
        .iter()
        .find_map(|child| first_camera(world, child))
}
