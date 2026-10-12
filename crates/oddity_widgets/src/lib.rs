//! Oddity Engine's widget layer.
//!
//! This is our take on `bevy_feathers`: Feathers' widgets and theming system,
//! with Godot's editor palette ([`theme`]), the Oddity brand ([`brand`]) and
//! the editor fonts ([`fonts`]). Godot-specific widgets (docks, tree view,
//! inspector rows) will live here as the editor grows.
//!
//! Add [`OddityWidgetsPlugin`] after Bevy's `DefaultPlugins`.

pub mod brand;
pub mod fonts;
pub mod theme;

use bevy::{
    feathers::{FeathersPlugins, theme::UiTheme},
    prelude::*,
};

pub use bevy::feathers;
pub use fonts::OddityFonts;
pub use theme::{Palette, ThemeConfig};

/// Feathers, the Godot palette, the editor fonts and Oddity's icons.
#[derive(Default)]
pub struct OddityWidgetsPlugin {
    /// The colours to build the theme from.
    pub theme: ThemeConfig,
}

impl Plugin for OddityWidgetsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<bevy::feathers::FeathersCorePlugin>() {
            app.add_plugins(FeathersPlugins);
        }
        // Must come after Feathers registers its own fonts, so ours win.
        fonts::register(app);

        app.insert_resource(UiTheme(self.theme.theme_props()))
            .insert_resource(ActivePalette(self.theme.palette()))
            .init_resource::<OddityFonts>();

        if !app.is_plugin_added::<oddity_icons::IconsPlugin>() {
            app.add_plugins(oddity_icons::IconsPlugin);
        }
    }
}

/// The palette currently in use, for widgets that need raw colours.
#[derive(Resource, Clone, Copy, Debug, Deref)]
pub struct ActivePalette(pub Palette);
