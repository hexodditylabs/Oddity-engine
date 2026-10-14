//! Sets the window and taskbar icon to the Oddity wizard cat.

use bevy::{ecs::system::NonSendMarker, prelude::*, winit::WINIT_WINDOWS};
use oddity_icons::Icon;
use winit::window::Icon as WinitIcon;

/// Size of the window icon in pixels.
const ICON_PX: u32 = 64;

pub struct WindowIconPlugin;

impl Plugin for WindowIconPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            set_window_icon.run_if(not(resource_exists::<IconSet>)),
        );
    }
}

/// Marks that the icon has been applied.
#[derive(Resource)]
struct IconSet;

/// winit windows only exist after the first frames, and must be touched on
/// the main thread (hence `NonSendMarker`).
fn set_window_icon(_main_thread: NonSendMarker, mut commands: Commands) {
    let applied = WINIT_WINDOWS.with_borrow(|winit_windows| {
        if winit_windows.windows.is_empty() {
            return false;
        }
        match cat_icon() {
            Some(icon) => {
                for window in winit_windows.windows.values() {
                    window.set_window_icon(Some(icon.clone()));
                }
            }
            None => warn!("could not build the window icon"),
        }
        true
    });
    if applied {
        commands.insert_resource(IconSet);
    }
}

/// The 15x15 cat, scaled by a whole number so its pixels stay square,
/// centred on a transparent square.
fn cat_icon() -> Option<WinitIcon> {
    const CAT_GRID: u32 = 15;
    let drawn = (ICON_PX / CAT_GRID) * CAT_GRID;
    let cat = Icon::oddity_cat().rasterize(drawn).ok()?;

    let mut rgba = vec![0u8; (ICON_PX * ICON_PX * 4) as usize];
    let left = (ICON_PX - cat.width) / 2;
    let top = (ICON_PX - cat.height) / 2;
    for y in 0..cat.height {
        let src = (y * cat.width * 4) as usize;
        let dst = (((top + y) * ICON_PX + left) * 4) as usize;
        let row = (cat.width * 4) as usize;
        rgba[dst..dst + row].copy_from_slice(&cat.data[src..src + row]);
    }
    WinitIcon::from_rgba(rgba, ICON_PX, ICON_PX).ok()
}
