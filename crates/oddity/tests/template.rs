//! C2: a new game's main level loads: a camera (off until the game starts),
//! a light and a floor, and nothing it can't read.

use bevy::transform::TransformPlugin;
use oddity::prelude::*;
use oddity_level::{Level, MeshShape, UnknownData, spawn_level};
use oddity_project::{OdditySource, Project};

#[test]
fn a_new_games_main_level_loads() {
    let dir = tempfile::tempdir().unwrap();
    let source = OdditySource::Path("crates/oddity".into());
    let project = Project::create_game(dir.path(), "Fresh", &source).unwrap();
    let main = project.file.editor.main_level.clone().unwrap();
    let level = Level::read(project.root.join(main)).unwrap();

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin));
    let world = app.world_mut();
    spawn_level(world, &level).unwrap();
    assert_eq!(world.query::<&UnknownData>().iter(world).count(), 0);
    let cameras: Vec<bool> = world
        .query::<&Camera>()
        .iter(world)
        .map(|camera| camera.is_active)
        .collect();
    assert_eq!(cameras, [false]);
    assert_eq!(world.query::<&DirectionalLight>().iter(world).count(), 1);
    assert_eq!(world.query::<&MeshShape>().iter(world).count(), 1);
}
