//! Hover and press feedback for flat, background-only controls.
//!
//! Feathers buttons bring their own states; this is for the lighter controls
//! Godot and jackdaw use everywhere (icon buttons, tabs, tree rows), which
//! show nothing until the pointer is over them.
//!
//! Add [`HoverFill`] and these observers to an entity:
//!
//! ```ignore
//! bsn! { Node {..} HoverFill on(hover_fill_over) on(hover_fill_out) on(hover_fill_press) on(hover_fill_release) }
//! ```
//!
//! From code: `commands.spawn(..).observe(hover_fill_over).observe(hover_fill_out)..`.

use bevy::{
    picking::events::{PointerOut, PointerOver, PointerPress, PointerRelease},
    prelude::*,
};

use crate::ActivePalette;

/// Paints a subtle background while hovered or pressed, then returns to `base`.
///
/// `base` is what the control looks like at rest. Code that marks a control as
/// active (a selected row, the running Play button) sets `base`, so hovering
/// and leaving never erases that state.
#[derive(Component, Clone, Debug)]
#[require(BackgroundColor)]
pub struct HoverFill {
    /// The resting background.
    pub base: Color,
}

impl Default for HoverFill {
    fn default() -> Self {
        Self { base: Color::NONE }
    }
}

/// Pointer entered: show the hover colour.
pub fn hover_fill_over(
    over: On<PointerOver>,
    palette: Res<ActivePalette>,
    mut fills: Query<(&mut BackgroundColor, &HoverFill)>,
) {
    if let Ok((mut bg, fill)) = fills.get_mut(over.event_target()) {
        bg.0 = hover_colour(&palette, fill.base);
    }
}

/// A control at rest shows nothing, so hover gives it the flat hover grey. An
/// active one (a selected row, a toggled tool) keeps its colour, slightly
/// lightened, so hovering never makes it look unselected.
fn hover_colour(palette: &ActivePalette, base: Color) -> Color {
    if base.alpha() == 0.0 {
        palette.flat_hover
    } else {
        base.mix(&Color::WHITE, 0.06)
    }
}

/// Pointer left: back to the resting colour.
pub fn hover_fill_out(out: On<PointerOut>, mut fills: Query<(&mut BackgroundColor, &HoverFill)>) {
    if let Ok((mut bg, fill)) = fills.get_mut(out.event_target()) {
        bg.0 = fill.base;
    }
}

/// Button down: show the pressed colour.
pub fn hover_fill_press(
    press: On<PointerPress>,
    palette: Res<ActivePalette>,
    mut fills: Query<&mut BackgroundColor, With<HoverFill>>,
) {
    if let Ok(mut bg) = fills.get_mut(press.event_target()) {
        bg.0 = palette.flat_pressed;
    }
}

/// Button up while still over the control: back to hover.
pub fn hover_fill_release(
    release: On<PointerRelease>,
    palette: Res<ActivePalette>,
    mut fills: Query<(&mut BackgroundColor, &HoverFill)>,
) {
    if let Ok((mut bg, fill)) = fills.get_mut(release.event_target()) {
        bg.0 = hover_colour(&palette, fill.base);
    }
}
