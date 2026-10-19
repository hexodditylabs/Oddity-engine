//! The shooter from `roadmap/scripting.md`: move with WASD, fire with Space
//! or the left mouse button, and the goblins come for you.
//!
//! Until physics (task F) a bullet hits by distance, and the HUD and sound
//! wait for task G; the score and the frame rate show in the window title.

use oddity::bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use oddity::prelude::*;

mod bullet;
mod camera_follow;
mod enemy;
mod health;
mod player;

pub use bullet::Bullet;
pub use camera_follow::CameraFollow;
pub use enemy::{Enemy, EnemyKilled};
pub use health::Health;
pub use player::Player;

/// Game-wide score: a plain Bevy resource (Unity's singleton, Godot's autoload).
#[derive(Resource, Default)]
pub struct Score(pub u32);

/// Everything the game adds besides scripts, which register themselves.
#[derive(Default)]
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Score>()
            .add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_systems(Update, (count_score, show_title).chain());
    }
}

fn count_score(mut kills: MessageReader<EnemyKilled>, mut score: ResMut<Score>) {
    for kill in kills.read() {
        score.0 += kill.score;
    }
}

/// The score, and the frame rate averaged over the last second, twice a
/// second (more often would be too fast to read).
fn show_title(
    score: Res<Score>,
    diagnostics: Res<DiagnosticsStore>,
    time: Res<Time<Real>>,
    mut last: Local<f32>,
    mut window: Single<&mut Window>,
) {
    let now = time.elapsed_secs();
    if now - *last < 0.5 && !score.is_changed() {
        return;
    }
    *last = now;
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|fps| fps.smoothed())
        .unwrap_or(0.0);
    window.title = format!("Shooter · Score: {} · {fps:.0} fps", score.0);
}
