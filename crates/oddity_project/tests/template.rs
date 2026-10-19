//! C2: a new game project has the game crate, a main level, the runner, the
//! `.gitignore` and the shared target folder.

use std::{fs, path::Path};

use oddity_project::{OdditySource, Project, ProjectError, crate_name, shared_target_dir};

fn toml_at(path: &Path) -> toml::Table {
    fs::read_to_string(path).unwrap().parse().unwrap()
}

fn engine() -> OdditySource {
    OdditySource::Path("/engine/crates/oddity".into())
}

#[test]
fn a_new_game_has_everything_it_needs_to_build_and_run() {
    let dir = tempfile::tempdir().unwrap();
    let project = Project::create_game(dir.path().join("my game"), "My Game!", &engine()).unwrap();
    let root = &project.root;

    let game = toml_at(&root.join("Cargo.toml"));
    assert_eq!(game["package"]["name"].as_str(), Some("my_game"));
    assert!(game.contains_key("workspace"), "its own workspace");
    assert_eq!(
        game["dependencies"]["oddity"]["path"].as_str(),
        Some("/engine/crates/oddity")
    );
    assert_eq!(
        game["profile"]["dev"]["package"]["*"]["opt-level"].as_integer(),
        Some(3)
    );
    let lib = fs::read_to_string(root.join("src/lib.rs")).unwrap();
    assert!(lib.contains("pub struct GamePlugin"));

    let runner = toml_at(&root.join(".oddity/runner/Cargo.toml"));
    assert_eq!(
        runner["dependencies"]["my_game"]["path"].as_str(),
        Some("../..")
    );
    let main = fs::read_to_string(root.join(".oddity/runner/src/main.rs")).unwrap();
    assert!(main.contains("oddity::run::<my_game::GamePlugin>()"));

    let config = toml_at(&root.join(".cargo/config.toml"));
    assert_eq!(
        config["build"]["target-dir"].as_str().map(Path::new),
        Some(shared_target_dir().as_path())
    );
    assert!(
        config["alias"]["play"]
            .as_str()
            .unwrap()
            .contains(".oddity/runner")
    );

    let ignored = fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(ignored.contains("/.oddity/") && ignored.contains("/.cargo/config.toml"));

    let loaded = Project::load(root).unwrap();
    assert_eq!(
        loaded.file.editor.main_level.as_deref(),
        Some("levels/main.level")
    );
    assert!(root.join("levels/main.level").is_file());
    assert!(loaded.file.input.axes2.contains_key("move"));
}

#[test]
fn the_runner_is_written_again_after_a_clone() {
    let dir = tempfile::tempdir().unwrap();
    let project = Project::create_game(dir.path(), "Clone", &engine()).unwrap();
    fs::remove_dir_all(project.root.join(".oddity")).unwrap();
    fs::remove_dir_all(project.root.join(".cargo")).unwrap();

    let release = OdditySource::release();
    project.write_runner(&release).unwrap();
    let runner = toml_at(&project.root.join(".oddity/runner/Cargo.toml"));
    let oddity = &runner["dependencies"]["oddity"];
    assert_eq!(
        oddity["git"].as_str(),
        Some("https://github.com/hexodditylabs/Oddity-engine")
    );
    assert_eq!(
        oddity["tag"].as_str(),
        Some(format!("v{}", oddity_project::EDITOR_VERSION).as_str())
    );
    assert!(project.root.join(".cargo/config.toml").is_file());
}

#[test]
fn a_game_isnt_made_over_another_project() {
    let dir = tempfile::tempdir().unwrap();
    Project::create_game(dir.path(), "First", &engine()).unwrap();
    assert!(matches!(
        Project::create_game(dir.path(), "Second", &engine()),
        Err(ProjectError::AlreadyExists(_))
    ));
}

#[test]
fn project_names_become_crate_names() {
    for (project, expected) in [
        ("My Game", "my_game"),
        ("  Space--Shooter 2 ", "space_shooter_2"),
        ("3D Racer", "game_3d_racer"),
        ("Café", "caf"),
        ("!!!", "game"),
        ("Test", "test_game"),
        ("oddity", "oddity_game"),
    ] {
        assert_eq!(crate_name(project), expected, "{project:?}");
    }
}

#[test]
fn the_shared_target_folder_is_per_editor_version() {
    assert!(
        shared_target_dir()
            .ends_with(Path::new("oddity/target").join(oddity_project::EDITOR_VERSION))
    );
}

#[test]
fn a_cargo_config_written_by_hand_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let project = Project::create_game(dir.path(), "Mine", &engine()).unwrap();
    let config = project.root.join(".cargo/config.toml");
    fs::write(&config, "[build]\ntarget-dir = \"../target\"\n").unwrap();
    project.write_runner(&engine()).unwrap();
    assert_eq!(
        fs::read_to_string(&config).unwrap(),
        "[build]\ntarget-dir = \"../target\"\n"
    );
}

#[test]
fn a_new_runner_starts_from_the_games_locked_versions() {
    let dir = tempfile::tempdir().unwrap();
    let project = Project::create_game(dir.path(), "Locked", &engine()).unwrap();
    fs::write(project.root.join("Cargo.lock"), "# the game's versions\n").unwrap();
    fs::remove_dir_all(project.root.join(".oddity")).unwrap();
    project.write_runner(&engine()).unwrap();
    let runner_lock = project.root.join(".oddity/runner/Cargo.lock");
    assert_eq!(
        fs::read_to_string(&runner_lock).unwrap(),
        "# the game's versions\n"
    );

    fs::write(&runner_lock, "# the runner's own, later\n").unwrap();
    project.write_runner(&engine()).unwrap();
    assert_eq!(
        fs::read_to_string(&runner_lock).unwrap(),
        "# the runner's own, later\n",
        "an existing one is the runner's"
    );
}
