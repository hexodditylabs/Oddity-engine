//! Where everything goes in the editor window, as in Godot 4:
//!
//! ```text
//! ┌──────────────── title bar (menus · tabs · play) ────────────────┐
//! │ Hierarchy  │ 3D  2D                        [⛶] │ Inspector       │
//! │ Import     │ toolbar                           │ Signals         │
//! │            │                                   │                 │
//! │────── ↕ ───↔          viewport                 ↔                 │
//! │ FileSystem │────────────────── ↕ ──────────────│                 │
//! │ History    │ Output                            │                 │
//! │            │ [Output]             0.1 · Bevy   │                 │
//! └────────────┴───────────────────────────────────┴─────────────────┘
//! ```
//!
//! Panels sit edge to edge on the window background with 3 px gaps; each gap
//! (`↔`, `↕`) is a drag handle that resizes the panel next to it.
//!
//! Distraction-free mode (the `⛶` button, or Ctrl+Shift+F11 as in Godot)
//! hides everything but the viewport.

use bevy::{
    feathers::{theme::ThemeBackgroundColor, tokens},
    prelude::*,
};
use oddity_widgets::split::{ResizeTarget, SplitAxis, split_handle};

use crate::{
    chrome::title_bar,
    docks::{bottom_panel, centre_dock, filesystem_dock, inspector_dock, scene_dock},
};

pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DistractionFree>()
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

/// Panels (and their handles) hidden in distraction-free mode.
#[derive(Component, Default, Clone)]
struct HiddenWhenFocused;

/// Spawn the UI camera and the whole editor layout.
fn spawn(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn_scene(root());
}

fn root() -> impl Scene {
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
        }
        ThemeBackgroundColor(tokens::WINDOW_BG)
        Children [
            @title_bar()
            --
            Node { flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Row }
            Children [
                @left_column() HiddenWhenFocused
                -- @split_handle(SplitAxis::Horizontal, ResizeTarget::Before, 180.0, 600.0)
                   HiddenWhenFocused
                -- @centre_column()
                -- @split_handle(SplitAxis::Horizontal, ResizeTarget::After, 240.0, 700.0)
                   HiddenWhenFocused
                -- @inspector_dock() Node { width: px(300) } HiddenWhenFocused
            ]
        ]
    }
}

/// Hierarchy above, FileSystem below, as in Godot's default layout.
fn left_column() -> impl Scene {
    bsn! {
        Node { width: px(280), flex_direction: FlexDirection::Column, min_height: px(0) }
        Children [
            @scene_dock() Node { flex_grow: 1.0 }
            -- @split_handle(SplitAxis::Vertical, ResizeTarget::After, 100.0, 700.0)
            -- @filesystem_dock() Node { height: px(280), flex_shrink: 0.0 }
        ]
    }
}

/// The viewport, with the bottom panel under it.
fn centre_column() -> impl Scene {
    bsn! {
        Node { flex_grow: 1.0, min_width: px(0), flex_direction: FlexDirection::Column }
        Children [
            @centre_dock() Node { flex_grow: 1.0 }
            -- @split_handle(SplitAxis::Vertical, ResizeTarget::After, 80.0, 600.0)
               HiddenWhenFocused
            -- @bottom_panel() Node { height: px(200), flex_shrink: 0.0 } HiddenWhenFocused
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
