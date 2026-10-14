//! The bottom panel's pages. Output shows a live status line (project, undo
//! history, unsaved changes) until the log arrives with the play loop.

use bevy::{prelude::*, text::FontSourceTemplate};
use oddity_core::History;
use oddity_widgets::{ActivePalette, fonts::paths};

use super::InsetPanel;
use crate::project::CurrentProject;

pub struct BottomPanelPlugin;

impl Plugin for BottomPanelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, update_status_line);
    }
}

/// The Output page: the log in a darker inset, as Godot draws it.
pub fn output_page() -> impl Scene {
    bsn! {
        Node { flex_grow: 1.0, padding: UiRect { left: px(6), right: px(6), top: px(6) } }
        Children [
            Node { flex_grow: 1.0, padding: px(8), border_radius: BorderRadius::all(px(4)) }
            InsetPanel
            Children [
                Text("")
                StatusLine
                TextFont {
                    font: FontSourceTemplate::Handle(paths::MONO),
                    font_size: FontSize::Px(13.0),
                }
            ]
        ]
    }
}

/// The status text in the Output panel.
#[derive(Component, Default, Clone)]
struct StatusLine;

/// Show the open project and the undo history in the Output panel.
fn update_status_line(
    history: Res<History>,
    project: Res<CurrentProject>,
    palette: Res<ActivePalette>,
    mut lines: Query<(&mut Text, &mut TextColor), With<StatusLine>>,
    added: Query<(), Added<StatusLine>>,
) {
    if !history.is_changed() && !project.is_changed() && added.is_empty() {
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
