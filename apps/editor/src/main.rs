//! The Oddity Engine editor.
//!
//! Phase 0: opens the editor window with Godot's layout sketched out, the
//! Oddity theme, fonts and icons. Docks become real in later phases.

mod screenshot;
mod shell;
mod window_icon;

use bevy::{prelude::*, window::WindowResolution};
use oddity_widgets::OddityWidgetsPlugin;

/// Shown in the title bar and the status line.
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> AppExit {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("Oddity Engine {VERSION}"),
                resolution: WindowResolution::new(1280, 760),
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            OddityWidgetsPlugin::default(),
            window_icon::WindowIconPlugin,
            screenshot::ScreenshotPlugin,
        ))
        .add_systems(Startup, shell::spawn)
        .run()
}
