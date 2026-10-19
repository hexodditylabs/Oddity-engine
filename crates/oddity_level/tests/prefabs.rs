//! B2: an instance with an override loads with the overridden value and
//! every other value from the prefab; changing the prefab file changes
//! every instance that didn't override it. Scripts instantiate prefabs.

use std::{fs, path::Path};

use bevy::{prelude::*, transform::TransformPlugin};
use oddity_level::{
    Level, LevelError, LevelPlugin, PrefabInstance, PrefabLibrary, save_level, save_prefab,
    spawn_level,
};
use oddity_project::AssetIndex;
use oddity_script::{Ctx, NodeRef, Prefab, Script, ScriptPlugin, script};
use serde_json::{Value, json};

// `#[script]` names the runtime `oddity`, as games see it.
use oddity_script as oddity;

#[script(id = "9c2e5b10")]
struct Health {
    #[default(100.0)]
    pub max: f32,
    pub label: String,
    pub eye: NodeRef,
    pub target: NodeRef,
}

impl Script for Health {}

/// The goblin prefab, as the editor would save it: a root with Health whose
/// `eye` points at its own child.
fn goblin_prefab(max: f32, label: &str) -> Level {
    let mut app = app_without_prefabs();
    let world = app.world_mut();
    let goblin = world
        .spawn((Name::new("Goblin"), Transform::default()))
        .id();
    let eye = world
        .spawn((
            Name::new("Eye"),
            ChildOf(goblin),
            Transform::from_xyz(0.0, 1.0, 0.0),
        ))
        .id();
    world.entity_mut(goblin).insert(Health {
        max,
        label: label.into(),
        eye: eye.into(),
        target: NodeRef::NONE,
    });
    save_prefab(world, goblin).unwrap()
}

fn app_without_prefabs() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, ScriptPlugin));
    app
}

fn app(root: &Path) -> App {
    let mut app = app_without_prefabs();
    app.add_plugins(LevelPlugin::new(root));
    app
}

fn write(root: &Path, path: &str, level: &Level) {
    let full = root.join(path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, level.to_json()).unwrap();
}

/// Main › Player, Enemies › (Goblin: max 30, moved; Goblin2: as the prefab;
/// Boss: targets the Player).
fn arena() -> Level {
    let prefab = json!({ "asset": null, "path": "prefabs/goblin.prefab" });
    Level::from_json(
        &json!({
            "format": "oddity-level",
            "version": 1,
            "nodes": [
                { "id": 1, "name": "Main", "parent": null },
                { "id": 2, "name": "Player", "parent": 1 },
                { "id": 3, "name": "Enemies", "parent": 1 },
                { "id": 4, "name": "Goblin", "parent": 3, "prefab": prefab,
                  "overrides": [
                      { "node": ".", "component": "script:9c2e5b10", "field": "max", "value": 30.0 },
                      { "node": ".", "component": "bevy_transform::components::transform::Transform",
                        "field": "translation.x", "value": 5.0 }
                  ] },
                { "id": 5, "name": "Goblin2", "parent": 3, "prefab": prefab },
                { "id": 6, "name": "Boss", "parent": 3, "prefab": prefab,
                  "overrides": [
                      { "node": ".", "component": "script:9c2e5b10", "field": "target", "value": 2 }
                  ] }
            ]
        })
        .to_string(),
    )
    .unwrap()
}

fn find(world: &mut World, name: &str) -> Entity {
    world
        .query::<(Entity, &Name)>()
        .iter(world)
        .find(|(_, n)| n.as_str() == name)
        .map(|(e, _)| e)
        .unwrap()
}

fn child(world: &World, parent: Entity, name: &str) -> Entity {
    world
        .get::<Children>(parent)
        .unwrap()
        .iter()
        .find(|&c| world.get::<Name>(c).map(Name::as_str) == Some(name))
        .unwrap()
}

#[test]
fn an_override_wins_and_everything_else_comes_from_the_prefab() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "prefabs/goblin.prefab",
        &goblin_prefab(100.0, "grunt"),
    );
    let mut app = app(dir.path());
    let world = app.world_mut();
    spawn_level(world, &arena()).unwrap();

    let goblin = find(world, "Goblin");
    let health = world.get::<Health>(goblin).unwrap();
    assert_eq!(health.max, 30.0, "overridden");
    assert_eq!(health.label, "grunt", "from the prefab");
    assert_eq!(world.get::<Transform>(goblin).unwrap().translation.x, 5.0);
    let eye = child(world, goblin, "Eye");
    assert_eq!(
        world.get::<Health>(goblin).unwrap().eye,
        NodeRef::from(eye),
        "a reference inside the prefab points at this instance's own node"
    );
    assert_eq!(world.get::<Transform>(eye).unwrap().translation.y, 1.0);

    let boss = find(world, "Boss");
    let player = find(world, "Player");
    assert_eq!(
        world.get::<Health>(boss).unwrap().target,
        NodeRef::from(player),
        "a reference in an override is a level id"
    );
}

#[test]
fn changing_the_prefab_changes_every_instance_that_didnt_override_it() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "prefabs/goblin.prefab",
        &goblin_prefab(100.0, "grunt"),
    );
    write(dir.path(), "levels/arena.level", &arena());
    write(
        dir.path(),
        "prefabs/goblin.prefab",
        &goblin_prefab(50.0, "brute"),
    );

    let mut app = app(dir.path());
    let world = app.world_mut();
    let level = Level::read(dir.path().join("levels/arena.level")).unwrap();
    spawn_level(world, &level).unwrap();
    let (goblin, goblin2) = (find(world, "Goblin"), find(world, "Goblin2"));
    assert_eq!(
        world.get::<Health>(goblin).unwrap().max,
        30.0,
        "still overridden"
    );
    assert_eq!(world.get::<Health>(goblin).unwrap().label, "brute");
    assert_eq!(world.get::<Health>(goblin2).unwrap().max, 50.0);
    assert_eq!(world.get::<Health>(goblin2).unwrap().label, "brute");
}

#[test]
fn a_level_saves_its_instances_as_prefab_and_overrides_only() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "prefabs/goblin.prefab",
        &goblin_prefab(100.0, "grunt"),
    );
    let mut app = app(dir.path());
    let world = app.world_mut();
    let arena = arena();
    let root = spawn_level(world, &arena).unwrap();
    // A node added to an instance in the level is the level's; one added
    // before the Player doesn't change the Player's id, which the Boss's
    // override refers to.
    let goblin = find(world, "Goblin");
    world.spawn((Name::new("Hat"), ChildOf(goblin)));
    let sign = world.spawn(Name::new("Sign")).id();
    world.entity_mut(root).insert_children(0, &[sign]);

    let saved = save_level(world, root).unwrap();
    let nodes: Vec<(u64, &str, Option<u64>)> = saved
        .nodes
        .iter()
        .map(|n| (n.id, n.name.as_str(), n.parent))
        .collect();
    assert_eq!(
        nodes,
        [
            (1, "Main", None),
            (7, "Sign", Some(1)),
            (2, "Player", Some(1)),
            (3, "Enemies", Some(1)),
            (4, "Goblin", Some(3)),
            (8, "Hat", Some(4)),
            (5, "Goblin2", Some(3)),
            (6, "Boss", Some(3)),
        ],
        "no Eye nodes (they come from the prefab), and every old id kept"
    );
    assert_eq!(saved.nodes[4].overrides, arena.nodes[3].overrides);
    assert!(saved.nodes[4].components.is_empty());
    assert_eq!(saved.nodes[7].overrides, arena.nodes[5].overrides);

    let mut again = app_without_prefabs();
    again.insert_resource(PrefabLibrary::new(dir.path()));
    let root = spawn_level(again.world_mut(), &saved).unwrap();
    assert_eq!(save_level(again.world(), root).unwrap(), saved);
}

#[test]
fn a_missing_prefab_keeps_its_instance_for_the_next_save() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    let world = app.world_mut();
    let root = spawn_level(world, &arena()).unwrap();
    let goblin = find(world, "Goblin");
    assert!(world.get::<Health>(goblin).is_none());
    assert_eq!(
        world.get::<PrefabInstance>(goblin).unwrap().prefab.path,
        "prefabs/goblin.prefab"
    );
    assert_eq!(save_level(world, root).unwrap(), arena());
}

#[test]
fn prefabs_are_found_by_asset_id_wherever_they_moved() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = AssetIndex::scan(dir.path()).unwrap();
    let id = index
        .create(
            "prefabs/goblin.prefab",
            goblin_prefab(77.0, "x").to_json().as_bytes(),
        )
        .unwrap();
    index
        .move_path("prefabs/goblin.prefab", "prefabs/enemies/goblin.prefab")
        .unwrap();

    let mut app = app(dir.path());
    let world = app.world_mut();
    let mut arena = arena();
    arena.nodes[4].prefab = Some(oddity_level::PrefabRef {
        asset: Some(id.uuid()),
        path: "prefabs/goblin.prefab".into(),
    });
    spawn_level(world, &arena).unwrap();
    let goblin2 = find(world, "Goblin2");
    assert_eq!(world.get::<Health>(goblin2).unwrap().max, 77.0);
}

#[test]
fn prefabs_inside_prefabs_come_later() {
    let dir = tempfile::tempdir().unwrap();
    let mut nested = goblin_prefab(1.0, "");
    nested.nodes[1].prefab = arena().nodes[3].prefab.clone();
    write(dir.path(), "prefabs/goblin.prefab", &nested);
    let mut app = app(dir.path());
    let error = app
        .world_mut()
        .resource_mut::<PrefabLibrary>()
        .get(&Prefab::at("prefabs/goblin.prefab"))
        .map(|file| file.nodes.len());
    assert_eq!(error.unwrap(), 2, "the library reads it");
    let world = app.world_mut();
    spawn_level(world, &arena()).unwrap();
    let goblin = find(world, "Goblin");
    assert!(
        world.get::<Health>(goblin).is_none(),
        "the instance is a placeholder: its prefab is refused"
    );

    let mut app = app_without_prefabs();
    let world = app.world_mut();
    let root = world.spawn(Name::new("Root")).id();
    world.spawn((
        Name::new("Inner"),
        ChildOf(root),
        PrefabInstance {
            prefab: Prefab::at("prefabs/goblin.prefab"),
            overrides: Vec::new(),
        },
    ));
    assert!(matches!(
        save_prefab(world, root),
        Err(LevelError::Invalid(_))
    ));
}

/// Fires one bullet, on its first frame, a little in front.
#[script]
struct Gun {
    pub bullet: Prefab,
    fired: bool,
}

impl Script for Gun {
    fn _onprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        if !self.fired {
            self.fired = true;
            let shot = ctx.instantiate(&self.bullet, Transform::from_xyz(0.0, 0.0, -2.0));
            ctx.world_mut().insert_resource(Shot(shot));
        }
    }
}

#[derive(Resource)]
struct Shot(NodeRef);

#[script(id = "b011e700")]
struct Bullet {
    pub speed: f32,
    ready: bool,
}

impl Script for Bullet {
    fn _onready(&mut self, _ctx: &mut Ctx) {
        self.ready = true;
    }
}

#[test]
fn a_script_instantiates_a_prefab_in_its_level() {
    let dir = tempfile::tempdir().unwrap();
    let bullet = {
        let mut app = app_without_prefabs();
        let world = app.world_mut();
        let root = world
            .spawn((
                Name::new("Bullet"),
                Transform::default(),
                Bullet {
                    speed: 20.0,
                    ready: false,
                },
            ))
            .id();
        save_prefab(world, root).unwrap()
    };
    write(dir.path(), "prefabs/bullet.prefab", &bullet);
    let level: Value = json!({
        "format": "oddity-level",
        "version": 1,
        "nodes": [
            { "id": 1, "name": "Main", "parent": null },
            { "id": 2, "name": "Player", "parent": 1, "components": {
                <Gun as TypePath>::type_path(): {
                    "bullet": { "asset": null, "path": "prefabs/bullet.prefab" }
                }
            } }
        ]
    });
    let mut app = app(dir.path());
    let main = spawn_level(
        app.world_mut(),
        &Level::from_json(&level.to_string()).unwrap(),
    )
    .unwrap();
    app.update();
    app.update();

    let world = app.world();
    let shot = world.resource::<Shot>().0.entity().unwrap();
    assert_eq!(world.get::<ChildOf>(shot).map(ChildOf::parent), Some(main));
    assert_eq!(world.get::<Transform>(shot).unwrap().translation.z, -2.0);
    let bullet = world.get::<Bullet>(shot).unwrap();
    assert_eq!(bullet.speed, 20.0, "from the prefab file");
    assert!(bullet.ready, "its script started");
    assert_eq!(
        world.get::<PrefabInstance>(shot).unwrap().prefab,
        Prefab::at("prefabs/bullet.prefab")
    );
}
