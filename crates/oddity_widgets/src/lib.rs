//! Oddity Engine's widget layer.
//!
//! This is our take on `bevy_feathers`: Feathers' widgets and theming system,
//! with Godot's editor palette ([`theme`]), the Oddity brand ([`brand`]) and
//! the editor fonts ([`fonts`]), plus the editor-shaped widgets Feathers
//! doesn't have:
//!
//! - [`button`]: flat icon buttons and toggles for toolbars
//! - [`dock`]: tabbed docks and Godot's bottom panel
//! - [`field`]: search fields with placeholder text
//! - [`split`]: drag handles that resize docks
//! - [`tree`]: the tree view behind the Hierarchy and FileSystem docks
//! - [`hover`]: hover/press feedback for flat controls
//!
//! Widgets live here, not in the editor, so the Hub and editor plugins can
//! use them too.
//!
//! Add [`OddityWidgetsPlugin`] after Bevy's `DefaultPlugins`.

pub mod brand;
pub mod button;
pub mod dock;
pub mod field;
pub mod fonts;
pub mod hover;
pub mod split;
pub mod theme;
pub mod tree;

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
        button::plugin(app);
        dock::plugin(app);
        field::plugin(app);
        tree::plugin(app);
    }
}

/// The palette currently in use, for widgets that need raw colours.
#[derive(Resource, Clone, Copy, Debug, Deref)]
pub struct ActivePalette(pub Palette);
