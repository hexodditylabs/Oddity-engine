//! `ODDITY_SCREENSHOT=out.png oddity-editor` saves a screenshot after the UI
//! settles, then quits. Used to check the editor's look in CI and reviews.
//! `ODDITY_SCREENSHOT_FRAMES=1800` waits longer (here 30 seconds at 60 fps),
//! to catch something slow, such as a build.

use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
};

/// Frames to wait so fonts, icons and layout have settled.
const SETTLE_FRAMES: u32 = 90;

pub struct ScreenshotPlugin;

impl Plugin for ScreenshotPlugin {
    fn build(&self, app: &mut App) {
        if let Ok(path) = std::env::var("ODDITY_SCREENSHOT") {
            let frames = std::env::var("ODDITY_SCREENSHOT_FRAMES")
                .ok()
                .and_then(|frames| frames.parse().ok())
                .unwrap_or(SETTLE_FRAMES);
            app.insert_resource(ScreenshotPath(path, frames))
                .add_systems(Update, take_screenshot);
        }
    }
}

/// Where to save, and after how many frames.
#[derive(Resource)]
struct ScreenshotPath(String, u32);

fn take_screenshot(mut commands: Commands, path: Res<ScreenshotPath>, mut frame: Local<u32>) {
    *frame += 1;
    if *frame != path.1 {
        return;
    }
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path.0.clone()))
        .observe(
            |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                exit.write(AppExit::Success);
            },
        );
}
