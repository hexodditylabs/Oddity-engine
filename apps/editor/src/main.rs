//! The Oddity Engine editor.
//!
//! This file only assembles the app. Where things live:
//!
//! - `chrome/`: the window frame (title bar, top bar, window icon)
//! - `docks/`: one module per dock (Hierarchy, Inspector, Viewport, bottom)
//! - `build.rs`: building the game when its code changes
//! - `layout.rs`: where the docks sit and how they resize
//! - `level.rs`: the level being edited
//! - `nodes.rs`: the built-in node types (Node3D, Camera3D, the lights...)
//! - `project.rs`: the project from the command line
//! - `shortcuts.rs`: editor-wide keys (undo, redo, delete)
//!
//! Editing logic (commands, undo, selection) is in the `oddity_core` crate;
//! reusable widgets are in `oddity_widgets`. Docks, tabs and menu items are
//! registered through `oddity_plugin_api`, the same way extensions add theirs.
//!
//! Two ways to run it:
//!
//! - `oddity-editor [project]`: the editor window.
//! - `oddity-editor mcp [project]`: no window; an MCP server on stdin and
//!   stdout, so an AI agent can drive the same commands (`oddity_mcp`).

mod actions;
mod build;
mod chrome;
mod docks;
mod layout;
mod level;
mod nodes;
mod project;
mod screenshot;
mod shortcuts;

use std::{num::NonZero, time::Duration};

use bevy::{
    app::ScheduleRunnerPlugin, prelude::*, render::pipelined_rendering::PipelinedRenderingPlugin,
    window::WindowResolution,
};
use oddity_core::OddityCorePlugin;
use oddity_mcp::McpPlugin;
use oddity_plugin_api::EditorMode;
use oddity_widgets::OddityWidgetsPlugin;
use project::CurrentProject;

/// Shown in the window title and the status line.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// How often the headless editor checks for MCP requests: often enough that
/// a tool call feels instant, rarely enough to stay idle between them.
const HEADLESS_TICK: Duration = Duration::from_millis(5);

fn main() -> AppExit {
    let mut args = std::env::args().skip(1);
    match args.next() {
        Some(command) if command == "mcp" => run_mcp(args.next().as_deref()),
        path => run_window(path.as_deref()),
    }
}

/// The editor's features: commands, queries, node types and the open level.
/// The window adds its UI on top, and `mcp` runs them alone. A built-in
/// extension's plugin goes here too, so the window, the CLI and MCP all
/// see it.
fn add_features(app: &mut App) {
    app.add_plugins((
        OddityCorePlugin,
        level::LevelPlugin,
        nodes::NodesPlugin,
        build::BuildPlugin,
    ));
}

/// No window: an MCP server on stdin and stdout until the client hangs up.
fn run_mcp(path: Option<&str>) -> AppExit {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(HEADLESS_TICK)))
        .insert_resource(CurrentProject::open(path))
        .insert_resource(EditorMode::Headless);
    add_features(&mut app);
    app.add_plugins(McpPlugin::stdio()).run()
}

fn run_window(path: Option<&str>) -> AppExit {
    let project = CurrentProject::open(path);
    let title = match project.name() {
        Some(name) => format!("{name} - Oddity Engine {VERSION}"),
        None => format!("Oddity Engine {VERSION}"),
    };
    let mut window = Window {
        title,
        resolution: WindowResolution::new(1280, 760),
        // An editor should react on the next frame: let the GPU queue only
        // one frame instead of the default two or three.
        desired_maximum_frame_latency: NonZero::new(1),
        ..default()
    };
    chrome::configure_window(&mut window);

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(window),
                ..default()
            })
            // Pipelined rendering draws one frame behind the app to gain
            // throughput. Great for games, but it adds input lag to an editor.
            .disable::<PipelinedRenderingPlugin>(),
    )
    .insert_resource(project)
    // Before the editor's plugins: they add their docks and menus only
    // when there is a window.
    .insert_resource(EditorMode::Window);
    add_features(&mut app);
    app.add_plugins((
        OddityWidgetsPlugin::default(),
        chrome::ChromePlugin,
        layout::LayoutPlugin,
        docks::DocksPlugin,
        shortcuts::ShortcutsPlugin,
        screenshot::ScreenshotPlugin,
    ))
    .run()
}
