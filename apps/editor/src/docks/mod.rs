//! The editor's docks, one module each. This file decides which tabs each
//! dock has; `layout.rs` decides where the docks go.
//!
//! | Dock | Tabs |
//! | --- | --- |
//! | Scene (top left) | Hierarchy, Import |
//! | FileSystem (bottom left) | FileSystem, History |
//! | Centre | 3D, 2D: one viewport, two modes |
//! | Inspector (right) | Inspector, Signals |
//! | Bottom panel | Output |

mod bottom;
mod filesystem;
mod hierarchy;
mod history;
mod inspector;
pub(crate) mod viewport;

use bevy::{prelude::*, text::FontSourceTemplate, ui_widgets::Activate};
use oddity_icons::Icon;
use oddity_widgets::{
    ActivePalette,
    button::{Toggled, tool_button},
    dock::{DockStyle, dock, dock_body, dock_buttons, dock_page, dock_tabs, tab},
    fonts::paths,
};

use crate::layout::DistractionFree;

pub use hierarchy::{add_node, delete_selection};
pub use viewport::ViewportSettings;

/// Systems for every dock.
pub struct DocksPlugin;

impl Plugin for DocksPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            hierarchy::HierarchyPlugin,
            inspector::InspectorPlugin,
            bottom::BottomPanelPlugin,
            filesystem::FileSystemPlugin,
            history::HistoryPlugin,
            viewport::ViewportPlugin,
        ))
        .add_systems(
            Update,
            (style_dim_text, style_insets, show_distraction_free),
        );
    }
}

pub fn scene_dock() -> impl Scene {
    bsn! {
        @dock()
        Children [
            @dock_tabs(vec![tab("Hierarchy"), tab("Import")])
            -- @dock_page(0) Children [ @hierarchy::hierarchy_page() ]
            -- @dock_page(1) Children [
                @placeholder("Select an asset in the FileSystem to adjust how it is imported.")
            ]
        ]
    }
}

pub fn filesystem_dock() -> impl Scene {
    bsn! {
        @dock()
        Children [
            @dock_tabs(vec![tab("FileSystem"), tab("History")])
            -- @dock_page(0) Children [ @filesystem::filesystem_page() ]
            -- @dock_page(1) Children [ @history::history_page() ]
        ]
    }
}

pub fn centre_dock() -> impl Scene {
    bsn! {
        @dock()
        viewport::CentreDock
        Children [
            Node { flex_shrink: 0.0, flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
            Children [
                @dock_tabs(vec![
                    tab("3D").icon(Icon::godot("3D")),
                    tab("2D").icon(Icon::godot("2D")),
                ])
                Node { flex_grow: 1.0 }
                --
                @tool_button(Icon::godot("DistractionFree"))
                Toggled
                DistractionFreeButton
                Node { margin: UiRect { top: px(8), right: px(6) } }
                on(|_: On<Activate>, mut focused: ResMut<DistractionFree>| focused.0 = !focused.0)
            ]
            --
            @dock_body() Children [ @viewport::viewport_page() ]
        ]
    }
}

pub fn inspector_dock() -> impl Scene {
    bsn! {
        @dock()
        Children [
            @dock_tabs(vec![tab("Inspector"), tab("Signals")])
            -- @dock_page(0) Children [ @inspector::inspector_page() ]
            -- @dock_page(1) Children [
                @placeholder("Signals and their connections arrive with scripting (Phase 4).")
            ]
        ]
    }
}

/// Godot's bottom panel: one panel, with its tabs as buttons along the bottom
/// and the engine version at the right.
pub fn bottom_panel() -> impl Scene {
    let version = format!("{}  ·  Bevy 0.20", crate::VERSION);
    bsn! {
        @dock()
        DockStyle::Buttons
        Children [
            @dock_page(0) Children [ @bottom::output_page() ]
            --
            Node { flex_shrink: 0.0, flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
            Children [
                @dock_buttons(vec![tab("Output")]) Node { flex_grow: 1.0 }
                --
                Text(version)
                DimText
                Node { margin: UiRect::right(px(12)) }
                TextFont {
                    font: FontSourceTemplate::Handle(paths::UI),
                    font_size: FontSize::Px(13.0),
                }
            ]
        ]
    }
}

/// Godot's empty-dock message: dim text, centred, wrapped.
fn placeholder(text: &'static str) -> impl Scene {
    bsn! {
        Node {
            flex_grow: 1.0,
            padding: px(16),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        }
        Children [
            Text(text)
            DimText
            TextLayout { justify: Justify::Center }
            TextFont {
                font: FontSourceTemplate::Handle(paths::UI),
                font_size: FontSize::Px(14.0),
            }
        ]
    }
}

/// Secondary text: placeholders, version numbers, hints.
#[derive(Component, Default, Clone)]
pub(crate) struct DimText;

fn style_dim_text(palette: Res<ActivePalette>, mut texts: Query<&mut TextColor, Added<DimText>>) {
    for mut colour in &mut texts {
        colour.0 = palette.font_secondary;
    }
}

/// A darker area inside a panel, for lists and logs (Godot's inset panels).
#[derive(Component, Default, Clone)]
pub(crate) struct InsetPanel;

fn style_insets(
    palette: Res<ActivePalette>,
    mut commands: Commands,
    insets: Query<Entity, Added<InsetPanel>>,
) {
    for inset in &insets {
        commands
            .entity(inset)
            .insert(BackgroundColor(palette.surface_low));
    }
}

/// The distraction-free toggle in the centre dock.
#[derive(Component, Default, Clone)]
struct DistractionFreeButton;

fn show_distraction_free(
    focused: Res<DistractionFree>,
    mut buttons: Query<&mut Toggled, With<DistractionFreeButton>>,
) {
    if focused.is_changed() {
        for mut toggled in &mut buttons {
            toggled.set_if_neq(Toggled(focused.0));
        }
    }
}
