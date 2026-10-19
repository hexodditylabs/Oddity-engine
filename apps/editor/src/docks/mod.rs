//! The editor's docks, one module each. Each module registers its own tabs
//! through `oddity_plugin_api`, as an extension would; this file builds the
//! docks from what was registered, and `layout.rs` decides where they go.
//!
//! | Dock | Tabs |
//! | --- | --- |
//! | Scene (top left) | Hierarchy, Import |
//! | FileSystem (bottom left) | FileSystem (folders), History |
//! | Centre | 3D, 2D: one viewport, two modes |
//! | Inspector (right) | Inspector, Signals |
//! | Bottom panel | Files (the chosen folder's contents), Output |

mod bottom;
pub(crate) mod filesystem;
mod hierarchy;
mod history;
mod inspector;
pub(crate) mod viewport;

use bevy::{prelude::*, text::FontSourceTemplate, ui_widgets::Activate};
use oddity_icons::Icon;
use oddity_plugin_api::{DockSlot, DockTab, DockTabs, EditorAppExt, EditorMode};
use oddity_widgets::{
    ActivePalette,
    button::{Toggled, tool_button},
    dock::{DockStyle, TabSpec, dock, dock_body, dock_buttons, dock_page, dock_tabs, tab},
    fonts::paths,
};

use crate::{
    actions::{UiAction, run_command},
    layout::{DistractionFree, ToggleDistractionFree},
};

pub use hierarchy::{delete_selection, open_add_node};

/// The order of a dock's first built-in tab. Built-in tabs are 100 apart,
/// so an extension's tab can go between them.
pub(crate) const FIRST_TAB: i32 = 0;
/// The order of a dock's second built-in tab.
pub(crate) const SECOND_TAB: i32 = 100;

/// Every dock and its systems.
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
        if app.editor_mode() == EditorMode::Window {
            app.add_dock_tab(
                DockTab::new(DockSlot::Scene, "Import", || {
                    placeholder("Select an asset in the FileSystem to adjust how it is imported.")
                })
                .order(SECOND_TAB),
            )
            .add_dock_tab(
                DockTab::new(DockSlot::Inspector, "Signals", || {
                    placeholder("Signals and their connections arrive with scripting (Phase 4).")
                })
                .order(SECOND_TAB),
            );
        }
    }
}

/// A side dock: the tabs registered for `slot`, above their pages.
pub fn side_dock(tabs: &DockTabs, slot: DockSlot) -> impl Scene {
    let tabs = tabs.in_slot(slot);
    let titles = titles(&tabs);
    let pages = pages(&tabs);
    bsn! {
        @dock()
        Children [ @dock_tabs(titles) -- {pages} ]
    }
}

fn titles(tabs: &[&DockTab]) -> Vec<TabSpec> {
    tabs.iter().map(|t| tab(t.title.clone())).collect()
}

/// Each tab's page, matched to its tab by index.
fn pages(tabs: &[&DockTab]) -> Vec<impl Scene> {
    tabs.iter()
        .enumerate()
        .map(|(index, tab)| {
            let page = tab.page();
            bsn! { @dock_page(index) Children [ @{page} ] }
        })
        .collect()
}

/// The viewport dock: one view in 3D or 2D, and the distraction-free toggle.
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
                on(|_: On<Activate>, mut commands: Commands| {
                    run_command(&mut commands, ToggleDistractionFree::SPEC.id, serde_json::json!({}));
                })
            ]
            --
            @dock_body() Children [ @viewport::viewport_page() ]
        ]
    }
}

/// Godot's bottom panel: one panel, with its registered tabs as buttons
/// along the bottom and the engine version at the right.
pub fn bottom_panel(tabs: &DockTabs) -> impl Scene {
    let version = format!("{}  ·  Bevy 0.20", crate::VERSION);
    let tabs = tabs.in_slot(DockSlot::Bottom);
    let titles = titles(&tabs);
    let pages = pages(&tabs);
    bsn! {
        @dock()
        DockStyle::Buttons
        Children [
            {pages}
            --
            Node { flex_shrink: 0.0, flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
            Children [
                @dock_buttons(titles) Node { flex_grow: 1.0 }
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

#[cfg(test)]
mod tests {
    use oddity_core::headless_app;

    use super::*;

    #[test]
    fn each_dock_lists_its_tabs_in_godots_order() {
        let mut app = headless_app();
        app.insert_resource(EditorMode::Window)
            .add_plugins(DocksPlugin);
        let tabs = app.world().resource::<DockTabs>();
        let titles = |slot| -> Vec<String> {
            tabs.in_slot(slot)
                .iter()
                .map(|tab| tab.title.to_string())
                .collect()
        };
        assert_eq!(titles(DockSlot::Scene), ["Hierarchy", "Import"]);
        assert_eq!(titles(DockSlot::FileSystem), ["FileSystem", "History"]);
        assert_eq!(titles(DockSlot::Inspector), ["Inspector", "Signals"]);
        assert_eq!(titles(DockSlot::Bottom), ["Files", "Output"]);
    }

    #[test]
    fn headless_adds_no_tabs() {
        let mut app = headless_app();
        app.add_plugins(DocksPlugin);
        assert!(app.world().get_resource::<DockTabs>().is_none());
    }
}
