//! K4's exit test: every command undoes, redoes and replays exactly.
//!
//! Today every command comes from `oddity_core`. An extension adds its own
//! version of this test, passing a world that also has its headless plugin.

use std::borrow::Cow;

use bevy_ecs::{name::Name, world::World};
use oddity_core::{
    ArgKind, ArgSpec, Args, CommandError, CommandRegistry, CommandSpec, EditorCommand, Invocation,
    registry::entity_arg,
    testing::{RoundTripError, round_trip, round_trip_all, standard_world},
};

const ENTITY: ArgSpec = ArgSpec {
    name: "entity",
    kind: ArgKind::Entity,
    required: true,
    description: "The node to rename",
};
use serde_json::json;

#[test]
fn every_command_round_trips() {
    let failures = round_trip_all(&standard_world);
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Renames a node and "forgets" to put the old name back on undo.
struct ForgetfulRename(bevy_ecs::entity::Entity);

impl EditorCommand for ForgetfulRename {
    fn label(&self) -> Cow<'_, str> {
        "Forgetful rename".into()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        world.entity_mut(self.0).insert(Name::new("Renamed"));
        Ok(())
    }

    fn undo(&mut self, _world: &mut World) -> Result<(), CommandError> {
        Ok(())
    }

    fn invocation(&self) -> Invocation {
        Invocation::new("test.broken_undo", json!({ "entity": self.0.to_bits() }))
    }
}

fn with_broken_command(example: Option<&'static str>) -> World {
    let mut world = standard_world();
    let mut spec = CommandSpec::edit("test.broken_undo", "Rename badly.", &[ENTITY]);
    spec.example = example;
    world
        .resource_mut::<CommandRegistry>()
        .register(spec, |_, args: &Args| {
            Ok(Box::new(ForgetfulRename(entity_arg(args, "entity")?)))
        });
    world
}

#[test]
fn a_broken_undo_fails_with_the_commands_id() {
    let error = round_trip(
        &|| with_broken_command(Some(r#"{"entity": "Player"}"#)),
        "test.broken_undo",
    )
    .unwrap_err();
    assert!(matches!(
        error,
        RoundTripError::Mismatch { step: "undo", .. }
    ));
    assert!(
        error.to_string().starts_with("`test.broken_undo`"),
        "{error}"
    );
    // And the check over every command reports it too.
    let failures = round_trip_all(&|| with_broken_command(Some(r#"{"entity": "Player"}"#)));
    assert_eq!(failures.len(), 1, "{failures:#?}");
}

#[test]
fn an_edit_without_an_example_fails() {
    let error = round_trip(&|| with_broken_command(None), "test.broken_undo").unwrap_err();
    assert!(matches!(error, RoundTripError::NoExample(_)), "{error}");
}

#[test]
fn an_example_naming_a_missing_node_fails() {
    let error = round_trip(
        &|| with_broken_command(Some(r#"{"entity": "Nobody"}"#)),
        "test.broken_undo",
    )
    .unwrap_err();
    assert!(
        matches!(error, RoundTripError::BadExample { .. }),
        "{error}"
    );
}

#[test]
fn snapshots_hold_names_values_and_order_but_no_ids() {
    let snapshot = oddity_core::testing::snapshot(&mut standard_world());
    let text = snapshot.to_string();
    assert!(!text.contains("unreadable"), "{snapshot:#}");
    let player = &snapshot["children"][0];
    assert_eq!(player["name"], "Player");
    let health = player["components"]
        .as_object()
        .unwrap()
        .iter()
        .find(|(path, _)| path.ends_with("Health"))
        .map(|(_, value)| value.clone());
    assert_eq!(health, Some(json!({"current": 10.0, "max": 10.0})));
    assert_eq!(snapshot["children"][1]["children"][0]["name"], "Goblin");
    // Two copies of the level compare equal, though their ids may differ.
    let mut other = standard_world();
    other.spawn(Name::new("Unrelated, outside the level"));
    assert_eq!(oddity_core::testing::snapshot(&mut other), snapshot);
}
