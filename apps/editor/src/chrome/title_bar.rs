//! Oddity's own title bar, replacing the operating system's.
//!
//! - Drag the bar to move the window; double-click it to maximise or restore.
//! - Minimise, maximise/restore and close buttons on the right, Windows-style
//!   (the close button turns red on hover).
//! - Drag any window edge or corner to resize.
//!
//! On macOS the native traffic-light buttons stay (people expect them there):
//! the system title bar is made transparent and our bar is drawn under it,
//! with room left for the traffic lights.

use bevy::{
    ecs::system::NonSendMarker,
    math::CompassOctant,
    picking::{
        Pickable,
        cursor::{EntityCursor, OverrideCursor},
        events::{PointerClick, PointerOut, PointerOver, PointerPress},
        pointer::PointerButton,
    },
    prelude::*,
    window::{PrimaryWindow, SystemCursorIcon},
    winit::WINIT_WINDOWS,
};
use oddity_icons::{Icon, UiIcon};
use oddity_plugin_api::MenuBar;
use oddity_widgets::ActivePalette;

use super::top_bar::top_bar_content;

/// Height of the title bar in logical pixels (jackdaw uses 36).
const BAR_HEIGHT: f32 = 36.0;
/// The wizard cat at the left of the bar. Larger than the 16 px menu icons,
/// so the brand reads at a glance.
const APP_ICON: f32 = 24.0;
/// Width of each caption button (Windows 11 uses 46).
const BUTTON_WIDTH: f32 = 46.0;
/// How close to a window edge the pointer must be to resize.
const EDGE: f32 = 5.0;
/// Corners get a larger grab area, so diagonal resizing is easy to hit.
const CORNER: f32 = 12.0;
/// Windows' close-button red.
const CLOSE_HOVER: Color = Color::srgb(0.910, 0.067, 0.137);
const CLOSE_PRESSED: Color = Color::srgb(0.945, 0.439, 0.478);

/// macOS keeps its native traffic lights; everywhere else we draw our own buttons.
const NATIVE_BUTTONS: bool = cfg!(target_os = "macos");
/// Room for the macOS traffic lights on the left of the bar.
const TRAFFIC_LIGHT_SPACE: f32 = 78.0;

pub struct TitleBarPlugin;

impl Plugin for TitleBarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Maximized>()
            .add_systems(Update, track_maximized);
        if !NATIVE_BUTTONS {
            app.add_systems(Update, edge_resize);
        }
    }
}

/// Window settings for a custom title bar. Call on the primary window.
pub fn configure(window: &mut Window) {
    if NATIVE_BUTTONS {
        window.titlebar_transparent = true;
        window.fullsize_content_view = true;
        window.titlebar_show_title = false;
    } else {
        window.decorations = false;
    }
}

/// The title bar: app icon, the top bar's menus and controls, then the
/// window buttons. One row, like jackdaw.
pub fn title_bar(menus: MenuBar) -> impl Scene {
    let left_pad = if NATIVE_BUTTONS {
        TRAFFIC_LIGHT_SPACE
    } else {
        10.0
    };
    bsn! {
        Node {
            height: px(BAR_HEIGHT),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(6),
            padding: UiRect::left(px(left_pad)),
        }
        WindowDragArea
        on(drag_window)
        Children [
            UiIcon { icon: Icon::oddity_cat(), size: APP_ICON }
            Node { flex_shrink: 0.0 }
            WindowDragArea
            --
            @top_bar_content(menus)
            --
            Node { align_self: AlignSelf::Stretch, flex_shrink: 0.0 }
            Children [ { caption_buttons() } ]
        ]
    }
}

/// Marks parts of the title bar that move the window when dragged.
/// Buttons don't carry it, so pressing them never starts a drag.
#[derive(Component, Default, Clone)]
pub struct WindowDragArea;

/// What a caption button does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowAction {
    #[default]
    Minimize,
    /// Maximise, or restore when already maximised.
    Maximize,
    Close,
}

impl WindowAction {
    fn icon(self, maximized: bool) -> Icon {
        Icon::ui(match self {
            WindowAction::Minimize => "window-minimize",
            WindowAction::Maximize if maximized => "window-restore",
            WindowAction::Maximize => "window-maximize",
            WindowAction::Close => "window-close",
        })
    }
}

/// A minimise, maximise or close button.
#[derive(Component, Default, Clone)]
pub struct CaptionButton(pub WindowAction);

/// Whether the primary window is maximised, read from the OS each frame.
#[derive(Resource, Default, Debug, PartialEq, Eq)]
pub struct Maximized(pub bool);

fn caption_buttons() -> Vec<impl Scene> {
    if NATIVE_BUTTONS {
        return Vec::new();
    }
    [
        WindowAction::Minimize,
        WindowAction::Maximize,
        WindowAction::Close,
    ]
    .into_iter()
    .map(caption_button)
    .collect()
}

fn caption_button(action: WindowAction) -> impl Scene {
    let icon = action.icon(false);
    bsn! {
        Node {
            width: px(BUTTON_WIDTH),
            align_self: AlignSelf::Stretch,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        }
        CaptionButton(action)
        BackgroundColor(Color::NONE)
        on(caption_hover)
        on(caption_unhover)
        on(caption_press)
        on(caption_click)
        Children [
            UiIcon { icon: {icon.clone()}, size: 16.0 }
            Pickable::IGNORE
        ]
    }
}

fn caption_hover(
    over: On<PointerOver>,
    buttons: Query<&CaptionButton>,
    mut backgrounds: Query<&mut BackgroundColor>,
    palette: Res<ActivePalette>,
) {
    let entity = over.event_target();
    let (Ok(button), Ok(mut bg)) = (buttons.get(entity), backgrounds.get_mut(entity)) else {
        return;
    };
    bg.0 = match button.0 {
        WindowAction::Close => CLOSE_HOVER,
        _ => palette.flat_hover,
    };
}

fn caption_unhover(
    out: On<PointerOut>,
    mut backgrounds: Query<&mut BackgroundColor, With<CaptionButton>>,
) {
    if let Ok(mut bg) = backgrounds.get_mut(out.event_target()) {
        bg.0 = Color::NONE;
    }
}

fn caption_press(
    press: On<PointerPress>,
    buttons: Query<&CaptionButton>,
    mut backgrounds: Query<&mut BackgroundColor>,
    palette: Res<ActivePalette>,
) {
    let entity = press.event_target();
    let (Ok(button), Ok(mut bg)) = (buttons.get(entity), backgrounds.get_mut(entity)) else {
        return;
    };
    bg.0 = match button.0 {
        WindowAction::Close => CLOSE_PRESSED,
        _ => palette.flat_pressed,
    };
}

fn caption_click(
    click: On<PointerClick>,
    buttons: Query<&CaptionButton>,
    mut backgrounds: Query<&mut BackgroundColor>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    maximized: Res<Maximized>,
    mut exit: MessageWriter<AppExit>,
) {
    let entity = click.event_target();
    if click.button != PointerButton::Primary {
        return;
    }
    let Ok(button) = buttons.get(entity) else {
        return;
    };
    // The pointer leaves the button as the window changes; don't stay highlighted.
    if let Ok(mut bg) = backgrounds.get_mut(entity) {
        bg.0 = Color::NONE;
    }
    match button.0 {
        WindowAction::Minimize => window.set_minimized(true),
        WindowAction::Maximize => window.set_maximized(!maximized.0),
        // Phase 3: ask about unsaved levels and prefabs before closing.
        WindowAction::Close => {
            exit.write(AppExit::Success);
        }
    }
}

/// Press on the bar: drag to move, double-click to maximise or restore.
fn drag_window(
    press: On<PointerPress>,
    drag_areas: Query<(), With<WindowDragArea>>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    maximized: Res<Maximized>,
) {
    if press.button != PointerButton::Primary || !drag_areas.contains(press.original_event_target())
    {
        return;
    }
    // The same press on a resize edge starts a resize (`edge_resize`);
    // starting a move as well would win, so the edge would never resize.
    let on_edge = window
        .cursor_position()
        .and_then(|cursor| edge_at(cursor, Vec2::new(window.width(), window.height())));
    if !NATIVE_BUTTONS && !maximized.0 && window.resizable && on_edge.is_some() {
        return;
    }
    if press.count.is_multiple_of(2) {
        window.set_maximized(!maximized.0);
    } else {
        window.start_drag_move();
    }
}

/// Bevy doesn't report whether a window is maximised, so ask winit
/// (main thread only), and swap the maximise/restore icon when it changes.
fn track_maximized(
    _main_thread: NonSendMarker,
    window: Single<Entity, With<PrimaryWindow>>,
    mut maximized: ResMut<Maximized>,
    buttons: Query<(&CaptionButton, &Children)>,
    mut icons: Query<&mut UiIcon>,
) {
    let now = WINIT_WINDOWS
        .with_borrow(|windows| windows.get_window(*window).map(|w| w.is_maximized()))
        .unwrap_or(false);
    if !maximized.set_if_neq(Maximized(now)) {
        return;
    }
    for (button, children) in &buttons {
        if button.0 != WindowAction::Maximize {
            continue;
        }
        for child in children {
            if let Ok(mut icon) = icons.get_mut(*child) {
                icon.icon = WindowAction::Maximize.icon(now);
            }
        }
    }
}

/// Resize from the window's edges and corners, which the OS no longer
/// provides once decorations are off.
fn edge_resize(
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    maximized: Res<Maximized>,
    mut override_cursor: ResMut<OverrideCursor>,
    mut showing_resize_cursor: Local<bool>,
) {
    let edge = if maximized.0 || !window.resizable {
        None
    } else {
        window
            .cursor_position()
            .and_then(|cursor| edge_at(cursor, Vec2::new(window.width(), window.height())))
    };

    match edge {
        Some(direction) => {
            override_cursor.0 = Some(EntityCursor::System(resize_cursor(direction)));
            *showing_resize_cursor = true;
            if mouse.just_pressed(MouseButton::Left) {
                window.start_drag_resize(direction);
            }
        }
        None if *showing_resize_cursor => {
            override_cursor.0 = None;
            *showing_resize_cursor = false;
        }
        None => {}
    }
}

/// Which edge or corner a point is on, if any.
fn edge_at(point: Vec2, size: Vec2) -> Option<CompassOctant> {
    let near = |distance: f32, reach: f32| distance >= 0.0 && distance < reach;
    let (left, right) = (point.x, size.x - point.x);
    let (top, bottom) = (point.y, size.y - point.y);

    // Corners first, with their larger grab area.
    let corner =
        |a: f32, b: f32| near(a, CORNER) && near(b, CORNER) && (near(a, EDGE) || near(b, EDGE));
    if corner(top, left) {
        return Some(CompassOctant::NorthWest);
    }
    if corner(top, right) {
        return Some(CompassOctant::NorthEast);
    }
    if corner(bottom, left) {
        return Some(CompassOctant::SouthWest);
    }
    if corner(bottom, right) {
        return Some(CompassOctant::SouthEast);
    }
    if near(top, EDGE) {
        Some(CompassOctant::North)
    } else if near(bottom, EDGE) {
        Some(CompassOctant::South)
    } else if near(left, EDGE) {
        Some(CompassOctant::West)
    } else if near(right, EDGE) {
        Some(CompassOctant::East)
    } else {
        None
    }
}

fn resize_cursor(direction: CompassOctant) -> SystemCursorIcon {
    match direction {
        CompassOctant::North | CompassOctant::South => SystemCursorIcon::NsResize,
        CompassOctant::East | CompassOctant::West => SystemCursorIcon::EwResize,
        CompassOctant::NorthEast | CompassOctant::SouthWest => SystemCursorIcon::NeswResize,
        CompassOctant::NorthWest | CompassOctant::SouthEast => SystemCursorIcon::NwseResize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: Vec2 = Vec2::new(1280.0, 760.0);

    #[test]
    fn edges() {
        assert_eq!(
            edge_at(Vec2::new(640.0, 2.0), SIZE),
            Some(CompassOctant::North)
        );
        assert_eq!(
            edge_at(Vec2::new(640.0, 758.0), SIZE),
            Some(CompassOctant::South)
        );
        assert_eq!(
            edge_at(Vec2::new(1.0, 380.0), SIZE),
            Some(CompassOctant::West)
        );
        assert_eq!(
            edge_at(Vec2::new(1278.0, 380.0), SIZE),
            Some(CompassOctant::East)
        );
    }

    #[test]
    fn corners_have_a_bigger_grab_area() {
        assert_eq!(
            edge_at(Vec2::new(2.0, 10.0), SIZE),
            Some(CompassOctant::NorthWest)
        );
        assert_eq!(
            edge_at(Vec2::new(1270.0, 2.0), SIZE),
            Some(CompassOctant::NorthEast)
        );
        assert_eq!(
            edge_at(Vec2::new(10.0, 758.0), SIZE),
            Some(CompassOctant::SouthWest)
        );
        assert_eq!(
            edge_at(Vec2::new(1278.0, 750.0), SIZE),
            Some(CompassOctant::SouthEast)
        );
    }

    #[test]
    fn inside_is_not_an_edge() {
        assert_eq!(edge_at(Vec2::new(640.0, 380.0), SIZE), None);
        assert_eq!(edge_at(Vec2::new(10.0, 10.0), SIZE), None);
        assert_eq!(edge_at(Vec2::new(-5.0, 380.0), SIZE), None);
    }
}
