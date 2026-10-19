//! Drag handles that resize the panel next to them, like the gaps between
//! Godot's docks.
//!
//! A handle sits between two siblings in a flex row (or column) and resizes
//! one of them: [`ResizeTarget::Before`] for a left/top dock,
//! [`ResizeTarget::After`] for a right/bottom dock. The other side is
//! expected to be `flex_grow` and simply takes up the rest.
//!
//! ```ignore
//! Children [
//!     @hierarchy_dock()  Node { width: px(260) }
//!     -- @split_handle(SplitAxis::Horizontal, ResizeTarget::Before, 160.0, 600.0)
//!     -- @viewport()     Node { flex_grow: 1.0 }
//! ]
//! ```
//!
//! Sizes are set in logical pixels on the target's `Node`, so they survive
//! window resizes the way Godot's dock widths do.

use bevy::{
    picking::{
        cursor::EntityCursor,
        events::{PointerDrag, PointerDragEnd, PointerDragStart, PointerOut, PointerOver},
        pointer::PointerButton,
    },
    prelude::*,
    window::SystemCursorIcon,
};

use crate::ActivePalette;

/// Thickness of the grab area. It is also the gap between docks, 3 px as in Godot.
pub const HANDLE_THICKNESS: f32 = 3.0;

/// Which way the handle moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SplitAxis {
    /// A vertical bar dragged left and right (resizes widths).
    #[default]
    Horizontal,
    /// A horizontal bar dragged up and down (resizes heights).
    Vertical,
}

/// Which sibling the handle resizes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResizeTarget {
    /// The sibling before the handle (a left or top dock).
    #[default]
    Before,
    /// The sibling after the handle (a right or bottom dock).
    After,
}

/// A resize handle. See the module docs.
#[derive(Component, Clone, Debug)]
pub struct SplitHandle {
    /// Drag direction.
    pub axis: SplitAxis,
    /// Which neighbour gets resized.
    pub target: ResizeTarget,
    /// Smallest allowed size, in logical pixels.
    pub min: f32,
    /// Largest allowed size, in logical pixels.
    pub max: f32,
    /// The neighbour's size when the drag started. Using the total drag
    /// distance from there (instead of adding up per-frame deltas) keeps the
    /// edge exactly under the pointer.
    start_size: f32,
}

impl Default for SplitHandle {
    fn default() -> Self {
        Self::new(SplitAxis::Horizontal, ResizeTarget::Before, 120.0, 800.0)
    }
}

impl SplitHandle {
    /// A handle resizing `target` along `axis`, clamped to `min..=max`.
    pub fn new(axis: SplitAxis, target: ResizeTarget, min: f32, max: f32) -> Self {
        Self {
            axis,
            target,
            min,
            max: max.max(min),
            start_size: 0.0,
        }
    }

    /// `size` limited to `min..=max`. Never panics: if `min` is above `max`
    /// (the fields are public, so a scene can set them that way), `min` wins.
    pub fn clamp(&self, size: f32) -> f32 {
        size.min(self.max).max(self.min)
    }
}

/// A handle between two docks. Highlights in the accent colour while hovered
/// or dragged.
pub fn split_handle(axis: SplitAxis, target: ResizeTarget, min: f32, max: f32) -> impl Scene {
    let (width, height, cursor) = match axis {
        SplitAxis::Horizontal => (
            Val::Px(HANDLE_THICKNESS),
            Val::Auto,
            SystemCursorIcon::EwResize,
        ),
        SplitAxis::Vertical => (
            Val::Auto,
            Val::Px(HANDLE_THICKNESS),
            SystemCursorIcon::NsResize,
        ),
    };
    bsn! {
        Node {
            width: {width},
            height: {height},
            flex_shrink: 0.0,
            align_self: AlignSelf::Stretch,
        }
        SplitHandle { axis: {axis}, target: {target}, min: {min}, max: {max} }
        EntityCursor::System({cursor})
        BackgroundColor(Color::NONE)
        on(begin_drag)
        on(drag)
        on(end_drag)
        on(highlight)
        on(unhighlight)
    }
}

/// The sibling a handle resizes.
fn neighbour(
    handle: Entity,
    target: ResizeTarget,
    parents: &Query<&ChildOf>,
    children: &Query<&Children>,
) -> Option<Entity> {
    let parent = parents.get(handle).ok()?.parent();
    let siblings = children.get(parent).ok()?;
    let index = siblings.iter().position(|e| e == handle)?;
    match target {
        ResizeTarget::Before => index.checked_sub(1).and_then(|i| siblings.get(i)).copied(),
        ResizeTarget::After => siblings.get(index + 1).copied(),
    }
}

fn begin_drag(
    start: On<PointerDragStart>,
    mut handles: Query<&mut SplitHandle>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    sizes: Query<&ComputedNode>,
) {
    if start.button != PointerButton::Primary {
        return;
    }
    let Ok(mut handle) = handles.get_mut(start.event_target()) else {
        return;
    };
    let Some(target) = neighbour(start.event_target(), handle.target, &parents, &children) else {
        return;
    };
    if let Ok(computed) = sizes.get(target) {
        // ComputedNode is in physical pixels; Node sizes are logical.
        let size = computed.size() * computed.inverse_scale_factor();
        handle.start_size = match handle.axis {
            SplitAxis::Horizontal => size.x,
            SplitAxis::Vertical => size.y,
        };
    }
}

fn drag(
    drag: On<PointerDrag>,
    handles: Query<&SplitHandle>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    mut nodes: Query<&mut Node>,
) {
    if drag.button != PointerButton::Primary {
        return;
    }
    let Ok(handle) = handles.get(drag.event_target()) else {
        return;
    };
    let Some(target) = neighbour(drag.event_target(), handle.target, &parents, &children) else {
        return;
    };
    let distance = match handle.axis {
        SplitAxis::Horizontal => drag.distance.x,
        SplitAxis::Vertical => drag.distance.y,
    };
    // Dragging right/down grows a panel before the handle and shrinks one after it.
    let size = match handle.target {
        ResizeTarget::Before => handle.start_size + distance,
        ResizeTarget::After => handle.start_size - distance,
    };
    let size = handle.clamp(size);

    if let Ok(mut node) = nodes.get_mut(target) {
        match handle.axis {
            SplitAxis::Horizontal => node.width = Val::Px(size),
            SplitAxis::Vertical => node.height = Val::Px(size),
        }
    }
}

fn end_drag(
    end: On<PointerDragEnd>,
    mut backgrounds: Query<&mut BackgroundColor, With<SplitHandle>>,
) {
    if let Ok(mut bg) = backgrounds.get_mut(end.event_target()) {
        bg.0 = Color::NONE;
    }
}

fn highlight(
    over: On<PointerOver>,
    palette: Res<ActivePalette>,
    mut backgrounds: Query<&mut BackgroundColor, With<SplitHandle>>,
) {
    if let Ok(mut bg) = backgrounds.get_mut(over.event_target()) {
        bg.0 = palette.highlight;
    }
}

fn unhighlight(
    out: On<PointerOut>,
    mut backgrounds: Query<&mut BackgroundColor, With<SplitHandle>>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    // Keep the highlight while dragging, even when the pointer runs ahead of the handle.
    if mouse.pressed(MouseButton::Left) {
        return;
    }
    if let Ok(mut bg) = backgrounds.get_mut(out.event_target()) {
        bg.0 = Color::NONE;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_keeps_sizes_in_range() {
        let handle = SplitHandle::new(SplitAxis::Horizontal, ResizeTarget::Before, 100.0, 300.0);
        assert_eq!(handle.clamp(50.0), 100.0);
        assert_eq!(handle.clamp(200.0), 200.0);
        assert_eq!(handle.clamp(400.0), 300.0);
    }

    #[test]
    fn clamp_survives_min_above_max() {
        let handle = SplitHandle {
            min: 300.0,
            max: 100.0,
            ..default()
        };
        assert_eq!(handle.clamp(50.0), 300.0);
        assert_eq!(handle.clamp(500.0), 300.0);
    }
}
