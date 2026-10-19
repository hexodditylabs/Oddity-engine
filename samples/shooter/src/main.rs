//! The shooter's runner: `cargo run` in this folder plays the main level.

fn main() -> oddity::bevy::app::AppExit {
    oddity::run::<shooter::GamePlugin>()
}
