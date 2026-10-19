//! P3's exit test: a node type registered by a plugin can be listed and
//! created, in one undo step, and `node.create` passes the round-trip check.

use bevy::prelude::*;
use oddity_core::{
    EditorWorldExt, History, NodeKind, headless_app,
    testing::{round_trip, round_trip_all, standard_world_with},
};
use oddity_icons::Icon;
use oddity_plugin_api::{EditorAppExt, NodeType, NodeTypes};
use serde_json::{Value, json};

/// The component a beacon node starts with.
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
#[reflect(Component, Default)]
struct Beacon {
    range: f32,
}

impl Default for Beacon {
    fn default() -> Self {
        Self { range: 12.0 }
    }
}

struct BeaconPlugin;

impl Plugin for BeaconPlugin {
    fn build(&self, app: &mut App) {
        app.register_node(
            NodeType::new::<Beacon>("Beacon", Icon::godot("Marker3D")).category("Test"),
        );
    }
}

fn world() -> World {
    standard_world_with(|app| {
        app.add_plugins(BeaconPlugin);
    })
}

fn create(world: &mut World, args: Value) -> Entity {
    let output = world.run_command("node.create", args).unwrap();
    Entity::from_bits(output["entity"].as_u64().unwrap())
}

#[test]
fn a_registered_type_is_listed_with_its_category_and_icon() {
    let mut world = world();
    let listed = world.run_command("node_types.list", json!({})).unwrap();
    let types = listed["node_types"].as_array().unwrap();
    assert_eq!(
        types[0],
        json!({"name": "Node", "category": "Basic", "icon": "Node"})
    );
    assert!(types.contains(&json!({"name": "Beacon", "category": "Test", "icon": "Marker3D"})));
}

#[test]
fn creating_a_node_from_the_registry_is_one_undo_step() {
    let mut world = world();
    let beacon = create(&mut world, json!({"type": "Beacon"}));
    assert_eq!(world.get::<Beacon>(beacon), Some(&Beacon { range: 12.0 }));
    assert_eq!(world.get::<NodeKind>(beacon).unwrap().0, "Beacon");
    assert_eq!(world.get::<Name>(beacon).unwrap().as_str(), "Beacon");
    assert_eq!(world.resource::<History>().undo_len(), 1);

    world.undo().unwrap();
    let tree = world.run_command("level.tree", json!({})).unwrap();
    assert!(!tree.to_string().contains("Beacon"));
    world.redo().unwrap();
    assert_eq!(world.get::<Beacon>(beacon), Some(&Beacon { range: 12.0 }));

    // A second one gets a name of its own, as in Godot.
    let second = create(&mut world, json!({"type": "Beacon"}));
    assert_eq!(world.get::<Name>(second).unwrap().as_str(), "Beacon2");
}

#[test]
fn an_unknown_type_is_refused_without_leaving_anything() {
    let mut world = world();
    let count = |world: &World| world.iter_entities().count();
    let before = count(&world);
    assert!(
        world
            .run_command("node.create", json!({"type": "Dragon"}))
            .is_err()
    );
    assert_eq!(count(&world), before);
}

#[test]
fn node_create_passes_the_round_trip_check() {
    round_trip(&world, "node.create").unwrap();
    let failures = round_trip_all(&world);
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn node_types_are_registered_headless_too() {
    let mut app = headless_app();
    app.add_plugins(BeaconPlugin);
    let types = app.world().resource::<NodeTypes>();
    assert!(types.get("Beacon").is_some());
    assert!(
        types.get("Node").is_some(),
        "the plain node is always there"
    );
}

#[test]
fn icons_come_from_the_type_or_fall_back_by_name() {
    let mut app = headless_app();
    app.add_plugins(BeaconPlugin);
    let types = app.world().resource::<NodeTypes>();
    let kind = |name: &str| NodeKind(name.into());
    assert_eq!(
        types.icon_for(Some(&kind("Beacon"))),
        Icon::godot("Marker3D")
    );
    // Not registered, but Godot has an icon by that name.
    assert_eq!(
        types.icon_for(Some(&kind("CharacterBody3D"))),
        Icon::godot("CharacterBody3D")
    );
    assert_eq!(
        types.icon_for(Some(&kind("NoSuchThing"))),
        Icon::godot("Node")
    );
    assert_eq!(types.icon_for(None), Icon::godot("Node"));
}
