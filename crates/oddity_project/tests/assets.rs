//! B0: creating, renaming and moving files through the asset index keeps
//! every ID, and an asset whose `.import` file was lost gets its ID back by
//! path.

use std::{fs, path::Path};

use oddity_project::{AssetError, AssetId, AssetIndex, ImportFile};

fn write(root: &Path, path: &str, text: &str) {
    let full = root.join(path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, text).unwrap();
}

/// What the files on disk say: a fresh scan.
fn on_disk(root: &Path) -> Vec<(String, AssetId)> {
    AssetIndex::scan(root)
        .unwrap()
        .iter()
        .map(|(path, id)| (path.to_owned(), id))
        .collect()
}

#[test]
fn a_scan_finds_the_assets_and_identify_all_gives_each_an_id() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for path in [
        "oddity.toml",
        "README.md",
        "src/lib.rs",
        "target/debug/game",
        ".oddity/play/main.level",
        "levels/.hidden",
        "levels/main.level",
        "prefabs/enemies/goblin.prefab",
        "textures/wall.png",
    ] {
        write(root, path, "x");
    }
    let mut index = AssetIndex::scan(root).unwrap();
    assert_eq!(
        index.unidentified(),
        [
            "levels/main.level",
            "prefabs/enemies/goblin.prefab",
            "textures/wall.png"
        ],
        "only files in asset folders, none hidden"
    );
    assert_eq!(index.iter().count(), 0, "a scan writes nothing");

    index.identify_all().unwrap();
    assert!(index.unidentified().is_empty());
    assert!(root.join("textures/wall.png.import").is_file());
    let ids: Vec<_> = index.iter().map(|(p, id)| (p.to_owned(), id)).collect();
    assert_eq!(ids.len(), 3);
    assert_eq!(on_disk(root), ids, "the IDs are on disk");
}

#[test]
fn creating_renaming_and_moving_keep_every_id() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut index = AssetIndex::scan(root).unwrap();
    let bullet = index.create("prefabs/bullet.prefab", b"{}").unwrap();
    let goblin = index
        .create("prefabs/enemies/goblin.prefab", b"{}")
        .unwrap();
    let orc = index.create("prefabs/enemies/orc.prefab", b"{}").unwrap();
    assert_eq!(index.path_of(bullet), Some("prefabs/bullet.prefab"));

    // Rename in place.
    index
        .move_path("prefabs/bullet.prefab", "prefabs/rocket.prefab")
        .unwrap();
    // Move to another folder, created on the way.
    index
        .move_path("prefabs/enemies/orc.prefab", "monsters/orc.prefab")
        .unwrap();
    // Move a whole folder.
    index
        .move_path("prefabs/enemies", "monsters/small")
        .unwrap();

    assert_eq!(index.path_of(bullet), Some("prefabs/rocket.prefab"));
    assert_eq!(index.path_of(orc), Some("monsters/orc.prefab"));
    assert_eq!(index.path_of(goblin), Some("monsters/small/goblin.prefab"));
    assert_eq!(index.id_of("monsters/small/goblin.prefab"), Some(goblin));
    assert_eq!(index.id_of("prefabs/enemies/goblin.prefab"), None);
    assert!(!root.join("prefabs/bullet.prefab.import").exists());

    let mut expected = vec![
        ("monsters/orc.prefab".to_owned(), orc),
        ("monsters/small/goblin.prefab".to_owned(), goblin),
        ("prefabs/rocket.prefab".to_owned(), bullet),
    ];
    expected.sort();
    assert_eq!(on_disk(root), expected, "the disk agrees");
}

#[test]
fn a_lost_import_file_is_recovered_by_path() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut index = AssetIndex::scan(root).unwrap();
    let wall = index.create("textures/wall.png", b"png").unwrap();
    fs::remove_file(root.join("textures/wall.png.import")).unwrap();

    // The editor reopens the project: the texture looks new.
    let mut index = AssetIndex::scan(root).unwrap();
    index.identify_all().unwrap();
    assert_ne!(index.id_of("textures/wall.png"), Some(wall));

    // A level still refers to it as { asset: wall, path: "textures/wall.png" }.
    let found = index.resolve(wall, "textures/wall.png").unwrap();
    assert_eq!(found.as_deref(), Some("textures/wall.png"));
    assert_eq!(index.id_of("textures/wall.png"), Some(wall));
    assert_eq!(
        on_disk(root),
        [("textures/wall.png".to_owned(), wall)],
        "the old ID is written back"
    );
}

#[test]
fn recovery_also_works_before_identify_all() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let wall = AssetIndex::scan(root)
        .unwrap()
        .create("textures/wall.png", b"png")
        .unwrap();
    fs::remove_file(root.join("textures/wall.png.import")).unwrap();
    let mut index = AssetIndex::scan(root).unwrap();
    assert_eq!(
        index.resolve(wall, "textures/wall.png").unwrap().as_deref(),
        Some("textures/wall.png")
    );
    assert!(index.unidentified().is_empty());
}

#[test]
fn a_path_hint_never_steals_another_assets_id() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut index = AssetIndex::scan(root).unwrap();
    let floor = index.create("textures/floor.png", b"png").unwrap();
    let gone = AssetId::new();
    assert_eq!(index.resolve(gone, "textures/floor.png").unwrap(), None);
    assert_eq!(index.id_of("textures/floor.png"), Some(floor));
    assert_eq!(index.resolve(gone, "textures/nothing.png").unwrap(), None);
}

#[test]
fn a_copy_made_outside_the_editor_gets_its_own_id_and_keeps_the_settings() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut index = AssetIndex::scan(root).unwrap();
    let wall = index.create("textures/wall.png", b"png").unwrap();
    let mut settings = ImportFile::new();
    settings.id = wall;
    settings.import.insert("filter".into(), "nearest".into());
    settings
        .other
        .insert("from_a_newer_editor".into(), true.into());
    let text = serde_json::to_string(&settings).unwrap();
    fs::write(root.join("textures/wall.png.import"), &text).unwrap();
    fs::copy(
        root.join("textures/wall.png"),
        root.join("textures/wall2.png"),
    )
    .unwrap();
    fs::write(root.join("textures/wall2.png.import"), &text).unwrap();

    let mut index = AssetIndex::scan(root).unwrap();
    assert_eq!(index.id_of("textures/wall.png"), Some(wall));
    assert_eq!(index.unidentified(), ["textures/wall2.png"]);
    index.identify_all().unwrap();
    let copy = index.id_of("textures/wall2.png").unwrap();
    assert_ne!(copy, wall);

    let written: ImportFile =
        serde_json::from_str(&fs::read_to_string(root.join("textures/wall2.png.import")).unwrap())
            .unwrap();
    assert_eq!(written.id, copy);
    assert_eq!(written.import["filter"], "nearest");
    assert_eq!(written.other["from_a_newer_editor"], true);
}

#[test]
fn a_broken_import_file_is_reported_and_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "textures/wall.png", "png");
    write(root, "textures/wall.png.import", "{ not json");
    let mut index = AssetIndex::scan(root).unwrap();
    assert!(matches!(index.problems(), [AssetError::Parse { .. }]));
    index.identify_all().unwrap();
    assert_eq!(
        fs::read_to_string(root.join("textures/wall.png.import")).unwrap(),
        "{ not json"
    );
}

#[test]
fn paths_must_name_an_asset_inside_the_project() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = AssetIndex::scan(dir.path()).unwrap();
    for path in [
        "../outside/x.png",
        "/levels/x.level",
        "C:/levels/x.level",
        "levels\\x.level",
        "x.png",
        "src/lib.rs",
        "levels/.hidden",
        "levels//x.level",
        "levels/x.level.import",
    ] {
        assert!(
            matches!(index.create(path, b""), Err(AssetError::InvalidPath { .. })),
            "{path:?}"
        );
    }
    index.create("levels/a.level", b"").unwrap();
    index.create("levels/b.level", b"").unwrap();
    assert!(matches!(
        index.move_path("levels/a.level", "levels/b.level"),
        Err(AssetError::AlreadyExists(_))
    ));
    assert!(matches!(
        index.move_path("levels/none.level", "levels/c.level"),
        Err(AssetError::NotFound(_))
    ));
}

#[test]
fn ids_read_back_as_written() {
    let id = AssetId::new();
    assert_eq!(id.to_string().parse::<AssetId>().unwrap(), id);
    assert_eq!(id.to_string().len(), 36, "a hyphenated UUID");
    assert_ne!(AssetId::new(), id);
}
