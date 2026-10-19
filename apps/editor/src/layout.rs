//! Where everything goes in the editor window, as in Godot 4:
//!
//! ```text
//! ┌──────────────── title bar (menus · tabs · play) ────────────────┐
//! │ Hierarchy  │ 3D  2D                        [⛶] │ Inspector       │
//! │ Import     │ toolbar                           │ Signals         │
//! │            │                                   │                 │
//! │────── ↕ ───↔          viewport                 ↔                 │
//! │ FileSystem │────────────────── ↕ ──────────────│                 │
//! │ History    │ Files (the chosen folder)         │                 │
//! │ (folders)  │ [Files] [Output]       0.1 · Bevy │                 │
//! └────────────┴───────────────────────────────────┴─────────────────┘
//! ```
//!
//! Panels sit edge to edge on the window background with 3 px gaps; each gap
//! (`↔`, `↕`) is a drag handle that resizes the panel next to it.
//!
//! Distraction-free mode (the `⛶` button, or Ctrl+Shift+F11 as in Godot)
//! hides everything but the viewport.
//!
//! The docks' tabs and the menus come from what was registered through
//! `oddity_plugin_api` by the time the window opens.

use bevy::{
    feathers::{theme::ThemeBackgroundColor, tokens},
    prelude::*,
};
use oddity_plugin_api::{DockSlot, DockTabs, MenuBar};
use oddity_widgets::split::{ResizeTarget, SplitAxis, split_handle};

use oddity_core::{Args, CommandError, CommandSpec};
use oddity_plugin_api::EditorAppExt;

use crate::{
    actions::{Run, UiAction, ui_state},
    chrome::title_bar,
    docks::{bottom_panel, centre_dock, side_dock},
};

/// The editor window's layout and distraction-free mode.
pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DistractionFree>()
            .register_command::<Run<ToggleDistractionFree>>()
            .add_systems(Startup, spawn)
            .add_systems(
                Update,
                apply_distraction_free.run_if(resource_changed::<DistractionFree>),
            );
    }
}

/// Whether only the viewport is shown.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Deref, DerefMut)]
pub struct DistractionFree(pub bool);

/// `view.toggle_distraction_free`.
pub struct ToggleDistractionFree;

impl UiAction for ToggleDistractionFree {
    const SPEC: CommandSpec = CommandSpec::action(
        "view.toggle_distraction_free",
        "Show only the viewport, or bring the docks back (Godot's Ctrl+Shift+F11).",
        &[],
    );

    fn run(world: &mut World, _args: &Args) -> Result<(), CommandError> {
        let mut focused = ui_state::<DistractionFree>(world, Self::SPEC.id)?;
        focused.0 = !focused.0;
        Ok(())
    }
}

/// Panels (and their handles) hidden in distraction-free mode.
#[derive(Component, Default, Clone)]
struct HiddenWhenFocused;

/// Spawn the UI camera and the whole editor layout.
fn spawn(mut commands: Commands, tabs: Option<Res<DockTabs>>, menus: Option<Res<MenuBar>>) {
    let no_tabs = DockTabs::default();
    let tabs = tabs.as_deref().unwrap_or(&no_tabs);
    let menus = menus.as_deref().cloned().unwrap_or_default();
    commands.spawn(Camera2d);
    commands.spawn_scene(root(tabs, menus));
}

fn root(tabs: &DockTabs, menus: MenuBar) -> impl Scene {
    bsn! {
        // Without the zero minimums a flex root grows to fit its content, so
        // a small window would push the docks and window buttons off screen
        // instead of shrinking the viewport.
        Node {
            width: percent(100),
            height: percent(100),
            min_width: px(0),
            min_height: px(0),
            flex_direction: FlexDirection::Column,
            overflow: Overflow::clip(),
        }
        ThemeBackgroundColor(tokens::WINDOW_BG)
        Children [
            @title_bar(menus)
            --
            Node { flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Row }
            Children [
                // The side docks keep their width; the viewport gives way
                // when the window narrows, as in Godot.
                @left_column(tabs) HiddenWhenFocused
                -- @split_handle(SplitAxis::Horizontal, ResizeTarget::Before, 180.0, 600.0)
                   HiddenWhenFocused
                -- @centre_column(tabs)
                -- @split_handle(SplitAxis::Horizontal, ResizeTarget::After, 240.0, 700.0)
                   HiddenWhenFocused
                -- @side_dock(tabs, DockSlot::Inspector)
                   Node { width: px(300), flex_shrink: 0.0 }
                   HiddenWhenFocused
            ]
        ]
    }
}

/// Hierarchy above, FileSystem's folders below, as in Godot's default layout.
fn left_column(tabs: &DockTabs) -> impl Scene {
    bsn! {
        Node {
            width: px(280),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Column,
            min_height: px(0),
        }
        Children [
            @side_dock(tabs, DockSlot::Scene) Node { flex_grow: 1.0 }
            -- @split_handle(SplitAxis::Vertical, ResizeTarget::After, 100.0, 700.0)
            -- @side_dock(tabs, DockSlot::FileSystem) Node { height: px(280), flex_shrink: 0.0 }
        ]
    }
}

/// The viewport, with the bottom panel under it.
fn centre_column(tabs: &DockTabs) -> impl Scene {
    bsn! {
        Node {
            flex_grow: 1.0,
            min_width: px(0),
            flex_direction: FlexDirection::Column,
            overflow: Overflow::clip(),
        }
        Children [
            // Shrinks first when the window gets shorter, so the bottom
            // panel's tab bar always stays on screen.
            @centre_dock() Node { flex_grow: 1.0, min_height: px(0) }
            -- @split_handle(SplitAxis::Vertical, ResizeTarget::After, 80.0, 600.0)
               HiddenWhenFocused
            // Tall enough for two rows of FileSystem thumbnails.
            -- @bottom_panel(tabs) Node { height: px(280), flex_shrink: 0.0 } HiddenWhenFocused
        ]
    }
}

fn apply_distraction_free(
    focused: Res<DistractionFree>,
    mut panels: Query<&mut Node, With<HiddenWhenFocused>>,
) {
    let display = if focused.0 {
        Display::None
    } else {
        Display::Flex
    };
    for mut node in &mut panels {
        node.display = display;
    }
}
