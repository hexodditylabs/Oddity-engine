//! `ODDITY_SCREENSHOT=out.png oddity-editor` saves a screenshot after the UI
//! settles, then quits. Used to check the editor's look in CI and reviews.

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
            app.insert_resource(ScreenshotPath(path))
                .add_systems(Update, take_screenshot);
        }
    }
}

#[derive(Resource)]
struct ScreenshotPath(String);

fn take_screenshot(mut commands: Commands, path: Res<ScreenshotPath>, mut frame: Local<u32>) {
    *frame += 1;
    if *frame != SETTLE_FRAMES {
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
