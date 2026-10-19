//! D2: an Inspector field edits a script's field the way it edits a
//! component's, through its `FieldTarget`, as one undoable command.

use bevy::prelude::{Entity, Name};
use oddity_core::{EditorWorldExt, testing::standard_world};
use oddity_plugin_api::FieldTarget;
use serde_json::json;

#[test]
fn a_script_field_is_read_and_set_through_its_target() {
    let mut world = standard_world();
    let goblin = world
        .query::<(Entity, &Name)>()
        .iter(&world)
        .find(|(_, name)| name.as_str() == "Goblin")
        .map(|(entity, _)| entity)
        .unwrap();
    let speed = FieldTarget {
        entity: goblin,
        component: "script:game::Mover".into(),
        path: "speed".into(),
    };
    assert_eq!(speed.script(), Some("game::Mover"));
    assert_eq!(speed.get(&world).unwrap(), json!(3.0));
    speed.set(&mut world, json!(6.5)).unwrap();
    assert_eq!(speed.get(&world).unwrap(), json!(6.5));
    world.undo().unwrap();
    assert_eq!(speed.get(&world).unwrap(), json!(3.0));

    let missing = FieldTarget {
        path: "no_such_field".into(),
        ..speed.clone()
    };
    assert_eq!(missing.get(&world).unwrap(), json!(null));
}
