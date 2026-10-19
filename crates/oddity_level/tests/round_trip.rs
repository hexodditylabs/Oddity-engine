//! B1: a level saved from a World and read back gives the same level, with
//! `NodeRef`s pointing at the right nodes and nested structs intact; what
//! can't be read is kept as written.

use bevy::{ecs::entity_disabling::Disabled, prelude::*, transform::TransformPlugin};
use oddity_level::{Level, LevelError, NodeKind, UnknownData, save_level, spawn_level};
use oddity_script::{Event, NodeRef, Script, ScriptPlugin, script};
use serde_json::json;

// `#[script]` names the runtime `oddity`, as games see it.
use oddity_script as oddity;

#[derive(Reflect, Default, Debug, Clone, PartialEq)]
struct Stats {
    level: u32,
    tags: Vec<String>,
    aim: Aim,
}

#[derive(Reflect, Default, Debug, Clone, PartialEq)]
enum Aim {
    #[default]
    Free,
    Locked {
        strength: f32,
    },
}

#[script(id = "4f9d2c71")]
struct Player {
    #[default(6.0)]
    pub speed: f32,
    pub muzzle: NodeRef,
    pub target: NodeRef,
    pub stats: Stats,
    pub on_died: Event,
    #[hide]
    pub secret: u32,
    cooldown: f32,
}

impl Script for Player {}

/// No `id`: stored by its Rust path.
#[script]
struct Spinner {
    pub degrees: f32,
}

impl Script for Spinner {}

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, ScriptPlugin));
    app
}

/// Main › (Player › Muzzle, Enemies › Goblin, Deleted); returns Main.
fn build(world: &mut World) -> Entity {
    let main = world
        .spawn((
            Name::new("Main"),
            NodeKind("Node3D".into()),
            Transform::from_xyz(1.0, 2.0, 3.0),
            Visibility::default(),
        ))
        .id();
    let player = world.spawn((Name::new("Player"), ChildOf(main))).id();
    let muzzle = world
        .spawn((
            Name::new("Muzzle"),
            ChildOf(player),
            Transform::from_xyz(0.0, 0.0, -1.0),
        ))
        .id();
    let enemies = world.spawn((Name::new("Enemies"), ChildOf(main))).id();
    let goblin = world
        .spawn((
            Name::new("Goblin"),
            ChildOf(enemies),
            Spinner { degrees: 90.0 },
        ))
        .id();
    world.spawn((Name::new("Deleted"), ChildOf(main), Disabled));
    world.entity_mut(player).insert(Player {
        speed: 7.5,
        muzzle: muzzle.into(),
        target: goblin.into(),
        stats: Stats {
            level: 3,
            tags: vec!["hero".into()],
            aim: Aim::Locked { strength: 0.5 },
        },
        on_died: Event::default(),
        secret: 42,
        cooldown: 9.0,
    });
    main
}

fn find(world: &mut World, name: &str) -> Entity {
    world
        .query::<(Entity, &Name)>()
        .iter(world)
        .find(|(_, n)| n.as_str() == name)
        .map(|(e, _)| e)
        .unwrap()
}

#[test]
fn saving_then_loading_gives_the_same_level() {
    let mut first = app();
    let main = build(first.world_mut());
    let saved = save_level(first.world(), main).unwrap();

    let mut second = app();
    let text = saved.to_json();
    let root = spawn_level(second.world_mut(), &Level::from_json(&text).unwrap()).unwrap();
    let again = save_level(second.world(), root).unwrap();
    assert_eq!(again, saved);
    assert_eq!(again.to_json(), text);
}

#[test]
fn the_file_looks_like_the_spec() {
    let mut app = app();
    let main = build(app.world_mut());
    let level = save_level(app.world(), main).unwrap();
    let names: Vec<(u64, &str, Option<u64>)> = level
        .nodes
        .iter()
        .map(|n| (n.id, n.name.as_str(), n.parent))
        .collect();
    assert_eq!(
        names,
        [
            (1, "Main", None),
            (2, "Player", Some(1)),
            (3, "Muzzle", Some(2)),
            (4, "Enemies", Some(1)),
            (5, "Goblin", Some(4)),
        ],
        "parents first, siblings in order, deleted nodes left out"
    );

    let main_node = &level.nodes[0];
    assert_eq!(main_node.kind.as_deref(), Some("Node3D"));
    let keys: Vec<&str> = main_node.components.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "bevy_camera::visibility::Visibility",
            "bevy_transform::components::transform::Transform"
        ],
        "nothing Bevy works out by itself"
    );
    assert_eq!(
        main_node.components["bevy_transform::components::transform::Transform"]["translation"],
        json!([1.0, 2.0, 3.0])
    );

    let player = &level.nodes[1].components["script:4f9d2c71"];
    assert_eq!(
        player,
        &json!({
            "speed": 7.5,
            "muzzle": 3,
            "target": 5,
            "stats": { "level": 3, "tags": ["hero"], "aim": { "Locked": { "strength": 0.5 } } },
            "on_died": { "listeners": [] }
        }),
        "only saved fields; NodeRefs as ids"
    );
    let goblin = &level.nodes[4].components;
    assert!(goblin.contains_key(<Spinner as TypePath>::type_path()));
}

#[test]
fn references_point_at_the_loaded_nodes() {
    let mut first = app();
    let main = build(first.world_mut());
    let level = save_level(first.world(), main).unwrap();

    let mut second = app();
    let world = second.world_mut();
    spawn_level(world, &level).unwrap();
    let (goblin, muzzle, player) = (
        find(world, "Goblin"),
        find(world, "Muzzle"),
        find(world, "Player"),
    );
    let loaded = world.get::<Player>(player).unwrap();
    assert_eq!(loaded.target, NodeRef::from(goblin));
    assert_eq!(loaded.muzzle, NodeRef::from(muzzle));
    assert_eq!(loaded.stats.aim, Aim::Locked { strength: 0.5 });
    assert_eq!(loaded.speed, 7.5);
    assert_eq!(loaded.secret, 0, "#[hide] isn't saved");
    assert_eq!(
        loaded.cooldown, 0.0,
        "private fields start from their default"
    );
    assert_eq!(
        world.get::<ChildOf>(muzzle).map(ChildOf::parent),
        Some(player)
    );
}

#[test]
fn an_events_listeners_point_at_the_loaded_nodes() {
    let text = json!({
        "format": "oddity-level",
        "version": 1,
        "nodes": [
            { "id": 1, "name": "Main", "parent": null },
            { "id": 7, "name": "Player", "parent": 1,
              "components": { "script:4f9d2c71": {
                  "on_died": { "listeners": [{ "node": 9, "name": "cheer" }] }
              } } },
            { "id": 9, "name": "Crowd", "parent": 1 }
        ]
    })
    .to_string();
    let level = Level::from_json(&text).unwrap();
    let mut app = app();
    let world = app.world_mut();
    let root = spawn_level(world, &level).unwrap();
    let (player, crowd) = (find(world, "Player"), find(world, "Crowd"));
    let listener = &world.get::<Player>(player).unwrap().on_died.listeners()[0];
    assert_eq!(listener.node, NodeRef::from(crowd));
    assert_eq!(listener.name, "cheer");
    assert_eq!(
        world.get::<Player>(player).unwrap().speed,
        6.0,
        "the default"
    );

    let saved = save_level(world, root).unwrap();
    assert_eq!(
        saved.nodes[1].components["script:4f9d2c71"]["on_died"],
        json!({ "listeners": [{ "node": 9, "name": "cheer" }] }),
        "nodes keep the ids they were loaded with"
    );
}

#[test]
fn a_reference_outside_the_level_is_saved_as_none() {
    let mut app = app();
    let world = app.world_mut();
    let outside = world.spawn(Name::new("Editor camera")).id();
    let main = world
        .spawn((
            Name::new("Main"),
            Player {
                target: outside.into(),
                ..default()
            },
        ))
        .id();
    let level = save_level(world, main).unwrap();
    assert_eq!(
        level.nodes[0].components["script:4f9d2c71"]["target"],
        json!(null)
    );
}

#[test]
fn what_cant_be_read_is_kept_as_written() {
    let text = json!({
        "format": "oddity-level",
        "version": 1,
        "nodes": [
            { "id": 1, "name": "Main", "parent": null,
              "components": {
                  "script:deadbeef": { "hp": 3 },
                  "my_game::Gone": { "x": [1, 2] },
                  "script:4f9d2c71": { "speed": 2.0, "removed_field": true }
              },
              "added_by_a_newer_oddity": { "x": 1 } }
        ]
    })
    .to_string();
    let level = Level::from_json(&text).unwrap();
    let mut app = app();
    let world = app.world_mut();
    let root = spawn_level(world, &level).unwrap();
    let unknown = world.get::<UnknownData>(root).unwrap();
    assert_eq!(unknown.components.len(), 2);
    assert_eq!(world.get::<Player>(root).unwrap().speed, 2.0);

    let saved = save_level(world, root).unwrap();
    let node = &saved.nodes[0];
    assert_eq!(node.components["script:deadbeef"], json!({ "hp": 3 }));
    assert_eq!(node.components["my_game::Gone"], json!({ "x": [1, 2] }));
    assert_eq!(node.other["added_by_a_newer_oddity"], json!({ "x": 1 }));
    assert!(
        node.components["script:4f9d2c71"]
            .get("removed_field")
            .is_none(),
        "a field the script no longer has is dropped by the game, which has the real type"
    );
}

fn invalid(level: serde_json::Value) -> String {
    let mut app = app();
    let level = Level::from_json(&level.to_string()).unwrap();
    let before = app.world().entities().len();
    let error = spawn_level(app.world_mut(), &level).unwrap_err();
    assert!(matches!(error, LevelError::Invalid(_)), "{error}");
    assert_eq!(app.world().entities().len(), before, "nothing is spawned");
    error.to_string()
}

#[test]
fn a_broken_file_spawns_nothing_and_says_why() {
    let node = |id: u64, parent: Option<u64>| json!({ "id": id, "name": "n", "parent": parent });
    let level = |nodes: Vec<serde_json::Value>| json!({ "format": "oddity-level", "version": 1, "nodes": nodes });
    assert!(invalid(json!({ "format": "bsn", "version": 1, "nodes": [] })).contains("format"));
    assert!(
        invalid(json!({ "format": "oddity-level", "version": 2, "nodes": [] })).contains("newer")
    );
    assert!(invalid(level(vec![node(1, None), node(2, None)])).contains("2 root nodes"));
    assert!(invalid(level(vec![node(2, Some(1)), node(1, None)])).contains("before its parent"));
    assert!(invalid(level(vec![node(1, None), node(1, Some(1))])).contains("two nodes"));
    assert!(matches!(
        Level::from_json("{ not json"),
        Err(LevelError::Parse(_))
    ));
}

#[derive(bevy::ecs::schedule::ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
struct Graph3d;

#[derive(Resource, Default)]
struct CamerasWithoutGraph(u32);

/// What Bevy's render plugins set up: `Camera3d` brings a 3D render graph,
/// and a `Camera` added without one is warned about.
fn app_with_cameras() -> App {
    use bevy::render::camera::CameraRenderGraph;

    let mut app = app();
    app.init_resource::<CamerasWithoutGraph>()
        .register_required_components_with::<Camera3d, CameraRenderGraph>(|| {
            CameraRenderGraph::new(Graph3d)
        });
    app.world_mut()
        .register_component_hooks::<Camera>()
        .on_add(|mut world, context| {
            if !world.entity(context.entity).contains::<CameraRenderGraph>() {
                world.resource_mut::<CamerasWithoutGraph>().0 += 1;
            }
        });
    app
}

#[test]
fn a_camera_comes_back_as_its_kind_brings_it() {
    use bevy::render::camera::CameraRenderGraph;

    let mut first = app_with_cameras();
    let world = first.world_mut();
    let main = world.spawn(Name::new("Main")).id();
    world.spawn((
        Name::new("Camera"),
        ChildOf(main),
        Camera3d::default(),
        Camera {
            is_active: false,
            ..default()
        },
    ));
    let level = save_level(world, main).unwrap();
    assert!(
        !level.nodes[1]
            .components
            .keys()
            .any(|key| key.ends_with("CameraRenderGraph")),
        "brought in by Camera3d, so not saved"
    );

    let mut second = app_with_cameras();
    let world = second.world_mut();
    spawn_level(world, &level).unwrap();
    let camera = find(world, "Camera");
    assert_eq!(
        world.get::<CameraRenderGraph>(camera).unwrap().0,
        CameraRenderGraph::new(Graph3d).0
    );
    assert_eq!(
        world.resource::<CamerasWithoutGraph>().0,
        0,
        "Camera3d goes in before Camera, so the camera never lacks its graph"
    );
    assert!(
        !world.get::<Camera>(camera).unwrap().is_active,
        "and the saved Camera wins"
    );
}
