//! Editor-wide keyboard shortcuts. Ctrl means Cmd on macOS.
//!
//! | Keys | Action |
//! | --- | --- |
//! | Ctrl+Z | Undo |
//! | Ctrl+Shift+Z, Ctrl+Y | Redo |
//! | Ctrl+A | Add a node under the selection (as in Godot) |
//! | Delete | Delete the selected nodes |
//! | Q, W, E, R | Select, move, rotate, scale tool |
//! | T | Local space on/off |
//! | Y | Snapping on/off |
//! | Ctrl+Shift+F11 | Distraction-free mode |
//!
//! None of them fire while a text field has focus, so typing, and the text
//! field's own undo, keep working.

use bevy::{input_focus::InputFocus, prelude::*, text::EditableText};
use oddity_core::EditorWorldExt;

use crate::{
    docks::viewport::{Tool, ViewportAction},
    docks::{ViewportSettings, add_node, delete_selection},
    layout::DistractionFree,
};

pub struct ShortcutsPlugin;

impl Plugin for ShortcutsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, shortcuts);
    }
}

/// An action bound to a key.
#[derive(Clone, Copy, Debug)]
enum Shortcut {
    Undo,
    Redo,
    AddNode,
    Delete,
    Viewport(ViewportAction),
    DistractionFree,
}

fn shortcuts(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Option<Res<InputFocus>>,
    text_fields: Query<(), With<EditableText>>,
    mut commands: Commands,
) {
    let typing = focus
        .and_then(|f| f.get())
        .is_some_and(|entity| text_fields.contains(entity));
    if typing {
        return;
    }
    let Some(shortcut) = pressed_shortcut(&keys) else {
        return;
    };
    commands.queue(move |world: &mut World| {
        let result = match shortcut {
            Shortcut::Undo => world.undo(),
            Shortcut::Redo => world.redo(),
            Shortcut::AddNode => {
                add_node(world);
                Ok(())
            }
            Shortcut::Delete => {
                delete_selection(world);
                Ok(())
            }
            Shortcut::Viewport(action) => {
                action.apply(&mut world.resource_mut::<ViewportSettings>());
                Ok(())
            }
            Shortcut::DistractionFree => {
                let mut focused = world.resource_mut::<DistractionFree>();
                focused.0 = !focused.0;
                Ok(())
            }
        };
        // "Nothing to undo" and the like are normal; not worth a warning.
        if let Err(error) = result {
            debug!("{error}");
        }
    });
}

fn pressed_shortcut(keys: &ButtonInput<KeyCode>) -> Option<Shortcut> {
    let ctrl = keys.any_pressed(if cfg!(target_os = "macos") {
        [KeyCode::SuperLeft, KeyCode::SuperRight]
    } else {
        [KeyCode::ControlLeft, KeyCode::ControlRight]
    });
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);

    if ctrl && keys.just_pressed(KeyCode::KeyZ) {
        Some(if shift {
            Shortcut::Redo
        } else {
            Shortcut::Undo
        })
    } else if ctrl && keys.just_pressed(KeyCode::KeyY) {
        Some(Shortcut::Redo)
    } else if ctrl && keys.just_pressed(KeyCode::KeyA) {
        Some(Shortcut::AddNode)
    } else if ctrl && shift && keys.just_pressed(KeyCode::F11) {
        Some(Shortcut::DistractionFree)
    } else if ctrl {
        None
    } else if keys.just_pressed(KeyCode::Delete) {
        Some(Shortcut::Delete)
    } else {
        // Godot's single-letter viewport keys.
        let action = |key| keys.just_pressed(key);
        [
            (KeyCode::KeyQ, ViewportAction::Tool(Tool::Select)),
            (KeyCode::KeyW, ViewportAction::Tool(Tool::Move)),
            (KeyCode::KeyE, ViewportAction::Tool(Tool::Rotate)),
            (KeyCode::KeyR, ViewportAction::Tool(Tool::Scale)),
            (KeyCode::KeyT, ViewportAction::ToggleLocalSpace),
            (KeyCode::KeyY, ViewportAction::ToggleSnap),
        ]
        .into_iter()
        .find(|(key, _)| action(*key))
        .map(|(_, a)| Shortcut::Viewport(a))
    }
}
