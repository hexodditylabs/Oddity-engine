//! The window frame around the docks.
//!
//! - [`title_bar`]: Oddity's own title bar replacing the OS one: window
//!   buttons, drag to move, double-click to maximise, edge resizing.
//! - [`top_bar`]: what sits inside it, laid out after jackdaw: menus, the
//!   level/prefab tabs, the level picker and the build/play controls.
//! - [`window_icon`]: the wizard cat in the taskbar.

mod title_bar;
mod top_bar;
mod window_icon;

use bevy::prelude::*;

pub use title_bar::{configure as configure_window, title_bar};

/// Title bar, top bar and window icon.
pub struct ChromePlugin;

impl Plugin for ChromePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            title_bar::TitleBarPlugin,
            top_bar::TopBarPlugin,
            window_icon::WindowIconPlugin,
        ));
    }
}
