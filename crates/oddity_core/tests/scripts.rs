//! D2: scripts on nodes are data in the editor, and every change to them is
//! one undoable command.

use bevy_ecs::{entity::Entity, name::Name, world::World};
use oddity_core::{
    CommandError, EditorWorldExt, History,
    scripts::{ScriptTypes, Scripts},
    testing::standard_world,
};
use serde_json::{Value, json};

fn node(world: &mut World, name: &str) -> Entity {
    world
        .query::<(Entity, &Name)>()
        .iter(world)
        .find(|(_, n)| n.as_str() == name)
        .map(|(e, _)| e)
        .unwrap()
}

fn mover(world: &World, entity: Entity) -> Option<(bool, Value)> {
    world
        .get::<Scripts>(entity)?
        .get("game::Mover")
        .map(|script| (script.enabled, Value::Object(script.values.0.clone())))
}

#[test]
fn an_attached_script_starts_at_its_defaults_and_undo_takes_it_off() {
    let mut world = standard_world();
    let player = node(&mut world, "Player");
    world
        .run_command(
            "script.attach",
            json!({ "entity": player.to_bits(), "script": "Mover" }),
        )
        .unwrap();
    assert_eq!(
        mover(&world, player),
        Some((true, json!({ "speed": 3.0, "target": null })))
    );
    world.undo().unwrap();
    assert!(
        world.get::<Scripts>(player).is_none(),
        "the node is as it was"
    );
}

#[test]
fn a_script_unknown_to_the_last_build_or_attached_twice_is_refused() {
    let mut world = standard_world();
    let goblin = node(&mut world, "Goblin");
    let attach = |world: &mut World, script: &str| {
        world.run_command(
            "script.attach",
            json!({ "entity": goblin.to_bits(), "script": script }),
        )
    };
    assert!(matches!(
        attach(&mut world, "Flier"),
        Err(CommandError::Invalid(why)) if why.contains("build the game")
    ));
    assert!(matches!(
        attach(&mut world, "Mover"),
        Err(CommandError::AlreadyHasComponent { .. })
    ));
    world.insert_resource(ScriptTypes::default());
    assert!(attach(&mut world, "game::Mover").is_err());
}

#[test]
fn scrubbing_a_field_is_one_undo_step() {
    let mut world = standard_world();
    let goblin = node(&mut world, "Goblin");
    let steps = world.resource::<History>().undo_len();
    world.resource_mut::<History>().begin_gesture();
    for speed in [3.5, 4.0, 4.5, 5.0] {
        world
            .run_command(
                "script.set_field",
                json!({ "entity": goblin.to_bits(), "script": "Mover", "field": "speed", "value": speed }),
            )
            .unwrap();
    }
    world.resource_mut::<History>().end_gesture();
    assert_eq!(world.resource::<History>().undo_len(), steps + 1);
    assert_eq!(mover(&world, goblin).unwrap().1["speed"], 5.0);
    world.undo().unwrap();
    assert_eq!(mover(&world, goblin).unwrap().1["speed"], 3.0);
}

#[test]
fn a_nested_field_takes_a_dotted_path_and_undo_removes_what_it_added() {
    let mut world = standard_world();
    let goblin = node(&mut world, "Goblin");
    world
        .run_command(
            "script.set_field",
            json!({ "entity": goblin.to_bits(), "script": "game::Mover", "field": "stats.level", "value": 3 }),
        )
        .unwrap();
    assert_eq!(
        mover(&world, goblin).unwrap().1["stats"],
        json!({ "level": 3 })
    );
    world.undo().unwrap();
    assert_eq!(
        mover(&world, goblin).unwrap().1,
        json!({ "speed": 3.0, "target": null, "stats": {} }),
        "the field is gone; the struct it made stays empty"
    );
}

#[test]
fn switching_off_and_removing_come_back_with_undo() {
    let mut world = standard_world();
    let goblin = node(&mut world, "Goblin");
    let args = |extra: Value| {
        let mut args = json!({ "entity": goblin.to_bits(), "script": "Mover" });
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        args
    };
    world
        .run_command("script.set_enabled", args(json!({ "enabled": false })))
        .unwrap();
    assert!(!mover(&world, goblin).unwrap().0);
    world.run_command("script.remove", args(json!({}))).unwrap();
    assert!(world.get::<Scripts>(goblin).is_none());
    world.undo().unwrap();
    assert!(!mover(&world, goblin).unwrap().0, "back, still off");
    world.undo().unwrap();
    assert!(mover(&world, goblin).unwrap().0);
}

#[test]
fn a_file_holds_the_script_named_like_it_or_its_only_one() {
    let types: ScriptTypes = serde_json::from_value(json!({
        "scripts": [
            { "type_path": "game::player::Player", "name": "Player", "file": "/game/src/player.rs" },
            { "type_path": "game::player::Muzzle", "name": "Muzzle", "file": "/game/src/player.rs" },
            { "type_path": "game::camera::FollowCamera", "name": "FollowCamera", "file": "/game/src/follow_camera.rs" },
            { "type_path": "game::misc::A", "name": "A", "file": "/game/src/misc.rs" },
            { "type_path": "game::misc::B", "name": "B", "file": "/game/src/misc.rs" }
        ]
    }))
    .unwrap();
    let in_file = |path: &str| {
        types
            .in_file(std::path::Path::new(path))
            .map(|s| s.name.as_str())
    };
    assert_eq!(
        in_file("/game/src/player.rs"),
        Some("Player"),
        "named like the file"
    );
    assert_eq!(in_file("/game/src/follow_camera.rs"), Some("FollowCamera"));
    assert_eq!(
        in_file("/game/src/misc.rs"),
        None,
        "two, neither named like it"
    );
    assert_eq!(in_file("/game/src/none.rs"), None);
}
