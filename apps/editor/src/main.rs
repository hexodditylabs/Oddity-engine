//! The Oddity Engine editor.
//!
//! This file only assembles the app. Where things live:
//!
//! - `chrome/`: the window frame (title bar, top bar, window icon)
//! - `docks/`: one module per dock (Hierarchy, Inspector, Viewport, bottom)
//! - `layout.rs`: where the docks sit and how they resize
//! - `level.rs`: the level being edited
//! - `project.rs`: the project from the command line
//! - `shortcuts.rs`: editor-wide keys (undo, redo, delete)
//!
//! Editing logic (commands, undo, selection) is in the `oddity_core` crate;
//! reusable widgets are in `oddity_widgets`.

mod chrome;
mod docks;
mod layout;
mod level;
mod project;
mod screenshot;
mod shortcuts;

use std::num::NonZero;

use bevy::{
    prelude::*, render::pipelined_rendering::PipelinedRenderingPlugin, window::WindowResolution,
};
use oddity_core::OddityCorePlugin;
use oddity_widgets::OddityWidgetsPlugin;
use project::CurrentProject;

/// Shown in the window title and the status line.
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> AppExit {
    let project = CurrentProject::from_args();
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

    App::new()
        .add_plugins(
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
        .add_plugins((
            OddityCorePlugin,
            OddityWidgetsPlugin::default(),
            chrome::ChromePlugin,
            layout::LayoutPlugin,
            docks::DocksPlugin,
            level::LevelPlugin,
            shortcuts::ShortcutsPlugin,
            screenshot::ScreenshotPlugin,
        ))
        .run()
}
