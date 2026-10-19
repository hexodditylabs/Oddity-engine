//! The bottom panel's pages. Output shows a status line (project, undo
//! history, unsaved changes) above the build's log: cargo's progress, dim,
//! and the compiler's messages, which open their file at their line when
//! clicked (`code.open`). The game's own log arrives with Play.

use bevy::{prelude::*, text::FontSourceTemplate};
use oddity_build::Level;
use oddity_core::{CommandExecuted, History};
use oddity_icons::Icon;
use oddity_plugin_api::{DockSlot, DockTab, EditorAppExt, EditorMode};
use oddity_widgets::{
    ActivePalette,
    dock::{Dock, DockPage},
    fonts::paths,
    tree::{TreeEvent, TreeItem, TreeView},
};
use serde_json::json;

use super::{InsetPanel, SECOND_TAB};
use crate::{
    actions::run_command,
    build::{BuildStatus, Builds},
    project::CurrentProject,
};

/// The Output tab of the bottom panel, and its status line.
pub struct BottomPanelPlugin;

impl Plugin for BottomPanelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, update_status_line);
        if app.editor_mode() == EditorMode::Window {
            app.add_systems(
                Update,
                (open_clicked_message, show_build_log, show_output_on_failure).chain(),
            );
            app.add_dock_tab(
                DockTab::new(DockSlot::Bottom, "Output", output_page).order(SECOND_TAB),
            );
        }
    }
}

/// The Output page: the log in a darker inset, as Godot draws it.
fn output_page() -> impl Scene {
    bsn! {
        Node { flex_grow: 1.0, padding: UiRect { left: px(6), right: px(6), top: px(6) } }
        Children [
            Node {
                flex_grow: 1.0,
                min_height: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                padding: px(8),
                border_radius: BorderRadius::all(px(4)),
            }
            InsetPanel
            Children [
                Text("")
                StatusLine
                TextFont {
                    font: FontSourceTemplate::Handle(paths::MONO),
                    font_size: FontSize::Px(13.0),
                }
                --
                Node {
                    flex_grow: 1.0,
                    min_height: px(0),
                    flex_direction: FlexDirection::Column,
                    overflow: Overflow::scroll_y(),
                }
                TreeView
                BuildLog
                ScrollPosition
            ]
        ]
    }
}

/// The build's log in the Output panel.
#[derive(Component, Default, Clone)]
struct BuildLog;

/// Show the build's log, newest at the bottom, scrolled to it.
fn show_build_log(
    builds: Res<Builds>,
    mut lists: Query<(&mut TreeView, &mut ScrollPosition), With<BuildLog>>,
    added: Query<(), Added<BuildLog>>,
) {
    if !builds.is_changed() && added.is_empty() {
        return;
    }
    let items: Vec<TreeItem> = builds
        .log
        .iter()
        .enumerate()
        .map(|(row, line)| TreeItem {
            key: row as u64,
            depth: 0,
            label: line.text.clone(),
            icon: match line.level {
                Some(Level::Error) => Some(Icon::godot("StatusError")),
                Some(Level::Warning) => Some(Icon::godot("StatusWarning")),
                Some(Level::Note) | None => None,
            },
            has_children: false,
            expanded: false,
            selected: false,
            dimmed: line.quiet,
            buttons: Vec::new(),
        })
        .collect();
    for (mut list, mut scroll) in &mut lists {
        list.items.clone_from(&items);
        // Layout clamps it to the last row.
        scroll.0.y = f32::MAX;
    }
}

/// When a build fails, bring Output to the front, as Godot does with
/// errors, so the messages are in view.
fn show_output_on_failure(
    builds: Res<Builds>,
    mut shown: Local<Option<BuildStatus>>,
    logs: Query<Entity, With<BuildLog>>,
    parents: Query<&ChildOf>,
    pages: Query<&DockPage>,
    mut docks: Query<&mut Dock>,
) {
    if *shown == Some(builds.status) {
        return;
    }
    *shown = Some(builds.status);
    if builds.status != BuildStatus::Failed {
        return;
    }
    for log in &logs {
        let mut page = None;
        for ancestor in parents.iter_ancestors(log) {
            match (page, pages.get(ancestor), docks.get_mut(ancestor)) {
                (None, Ok(DockPage(index)), _) => page = Some(*index),
                (Some(index), _, Ok(mut dock)) => {
                    dock.set_if_neq(Dock { active: index });
                    break;
                }
                _ => {}
            }
        }
    }
}

/// Clicking a compiler message opens its file at its line.
fn open_clicked_message(
    mut events: MessageReader<TreeEvent>,
    lists: Query<(), With<BuildLog>>,
    builds: Res<Builds>,
    mut commands: Commands,
) {
    for event in events.read() {
        if let TreeEvent::Select { tree, key, .. } = *event
            && lists.contains(tree)
            && let Some((file, line, column)) = builds
                .log
                .get(key as usize)
                .and_then(|row| row.location.clone())
        {
            let path = file.display().to_string();
            run_command(
                &mut commands,
                "code.open",
                json!({ "path": path, "line": line, "column": column }),
            );
        }
    }
}

/// The status text in the Output panel.
#[derive(Component, Default, Clone)]
struct StatusLine;

/// Show the open project and the undo history in the Output panel.
fn update_status_line(
    mut executed: MessageReader<CommandExecuted>,
    history: Res<History>,
    project: Res<CurrentProject>,
    palette: Res<ActivePalette>,
    mut lines: Query<(&mut Text, &mut TextColor), With<StatusLine>>,
    added: Query<(), Added<StatusLine>>,
) {
    if executed.read().count() == 0 && !project.is_changed() && added.is_empty() {
        return;
    }
    let project = project
        .name()
        .unwrap_or("none (run: cargo editor -- path/to/game)");
    let steps = history.undo_len();
    let mut status = format!(
        "Oddity Engine {}  ·  Project: {project}  ·  History: {steps} step{}",
        env!("CARGO_PKG_VERSION"),
        if steps == 1 { "" } else { "s" },
    );
    if let Some(label) = history.undo_label() {
        status.push_str(&format!("  ·  Ctrl+Z: {label}"));
    }
    if history.is_dirty() {
        status.push_str("  ·  unsaved changes");
    }
    for (mut text, mut color) in &mut lines {
        text.0.clone_from(&status);
        color.0 = palette.font;
    }
}
