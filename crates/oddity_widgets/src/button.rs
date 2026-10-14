//! Flat icon buttons for toolbars and bars.
//!
//! - [`icon_button`]: compact, for the title bar's dense rows.
//! - [`tool_button`]: Godot's 28 px toolbar button. Add [`Toggled`] to make
//!   it a toggle that shows its state the Godot way: a grey fill and an
//!   accent-coloured icon while on.
//!
//! Both send Feathers' `Activate` event on click, exactly like Feathers
//! buttons, so callers handle every kind the same way:
//! `@tool_button(Icon::godot("Add")) on(|_: On<Activate>| ...)`.

use bevy::{
    picking::{Pickable, events::PointerClick, pointer::PointerButton},
    prelude::*,
    ui_widgets::Activate,
};
use oddity_icons::{Icon, UiIcon};

use crate::{
    ActivePalette,
    hover::{HoverFill, hover_fill_out, hover_fill_over, hover_fill_press, hover_fill_release},
};

/// Width of an icon button; its height is [`ICON_BUTTON_HEIGHT`].
pub const ICON_BUTTON_WIDTH: f32 = 22.0;
/// Height of an icon button; fits in a 22 px control row with a 2 px inset.
pub const ICON_BUTTON_HEIGHT: f32 = 18.0;

/// A square, borderless button showing one 14 px icon. Highlights on hover.
pub fn icon_button(icon: Icon) -> impl Scene {
    bsn! {
        Node {
            width: px(ICON_BUTTON_WIDTH),
            height: px(ICON_BUTTON_HEIGHT),
            flex_shrink: 0.0,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(px(4)),
        }
        HoverFill
        on(hover_fill_over) on(hover_fill_out) on(hover_fill_press) on(hover_fill_release)
        on(activate_on_click)
        Children [
            UiIcon { icon: {icon.clone()}, size: 14.0 }
            Pickable::IGNORE
        ]
    }
}

/// Side of a [`tool_button`], as in Godot's toolbars.
pub const TOOL_BUTTON_SIZE: f32 = 28.0;

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(Update, paint_toggles);
}

/// Godot's toolbar button: 28 px square, one 16 px icon, highlights on hover.
pub fn tool_button(icon: Icon) -> impl Scene {
    bsn! {
        Node {
            width: px(TOOL_BUTTON_SIZE),
            height: px(TOOL_BUTTON_SIZE),
            flex_shrink: 0.0,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(px(4)),
        }
        HoverFill
        on(hover_fill_over) on(hover_fill_out) on(hover_fill_press) on(hover_fill_release)
        on(activate_on_click)
        Children [
            UiIcon { icon: {icon.clone()}, size: 16.0 }
            Pickable::IGNORE
        ]
    }
}

/// Makes a button a toggle. The owner sets the value; the button only shows it.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Toggled(pub bool);

fn paint_toggles(
    palette: Res<ActivePalette>,
    mut buttons: Query<
        (&Toggled, &mut HoverFill, &mut BackgroundColor, &Children),
        Changed<Toggled>,
    >,
    mut icons: Query<&mut ImageNode>,
) {
    for (toggled, mut fill, mut background, children) in &mut buttons {
        let (base, tint) = if toggled.0 {
            (palette.button_normal, palette.accent)
        } else {
            (Color::NONE, Color::WHITE)
        };
        fill.base = base;
        background.0 = base;
        for child in children {
            if let Ok(mut image) = icons.get_mut(*child) {
                image.color = tint;
            }
        }
    }
}

/// Turn a primary-button click into `Activate`, the event Feathers buttons send.
pub fn activate_on_click(click: On<PointerClick>, mut commands: Commands) {
    if click.button == PointerButton::Primary {
        commands.trigger(Activate {
            entity: click.event_target(),
        });
    }
}
