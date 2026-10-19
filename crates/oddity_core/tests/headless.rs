//! Phase 1 exit criterion: commands run headless, with undo.

use bevy_ecs::{
    entity::Entity, entity_disabling::Disabled, hierarchy::ChildOf, name::Name, prelude::*,
};
use bevy_reflect::{Reflect, prelude::ReflectDefault};
use oddity_core::{
    CommandError, CommandExecuted, EditorCommand, EditorWorldExt, History, LevelRoot, Selection,
    Source,
    commands::{Batch, DespawnEntity, RenameEntity, ReparentEntity, SetField, SpawnEntity},
    headless_app, level_root, reflect_json,
};
use serde_json::json;

#[derive(Component, Reflect, Default, Debug, PartialEq, Clone)]
#[reflect(Component, Default)]
struct Health {
    current: f32,
    max: f32,
}

#[derive(Component, Reflect, Default, Debug, PartialEq, Clone)]
#[reflect(Component, Default)]
struct Speed(f32);

/// A headless world with a level called `Main` open.
fn world() -> World {
    let mut app = headless_app();
    app.register_type::<Health>().register_type::<Speed>();
    let mut world = std::mem::take(app.world_mut());
    world.spawn((Name::new("Main"), LevelRoot));
    world
}

/// Spawn a node at the top of the open level, without a command.
fn node(world: &mut World, bundle: impl Bundle) -> Entity {
    let root = level_root(world).expect("the test world has a level");
    world.spawn((bundle, ChildOf(root))).id()
}

fn entity_from(output: serde_json::Value) -> Entity {
    Entity::from_bits(output["entity"].as_u64().unwrap())
}

fn name(world: &World, entity: Entity) -> Option<String> {
    world.get::<Name>(entity).map(|n| n.as_str().to_owned())
}

fn visible(world: &World, entity: Entity) -> bool {
    world.get_entity(entity).is_ok() && world.get::<Disabled>(entity).is_none()
}

#[test]
fn spawn_rename_undo_redo() {
    let mut world = world();
    let spawn = SpawnEntity::new(&mut world, "Player");
    let player = spawn.entity();
    world.execute(spawn).unwrap();
    world.execute(RenameEntity::new(player, "Hero")).unwrap();
    assert_eq!(name(&world, player).as_deref(), Some("Hero"));

    world.undo().unwrap();
    assert_eq!(name(&world, player).as_deref(), Some("Player"));
    world.undo().unwrap();
    assert!(!visible(&world, player), "undoing a spawn hides the entity");

    world.redo().unwrap();
    world.redo().unwrap();
    assert!(visible(&world, player), "redo brings back the same entity");
    assert_eq!(name(&world, player).as_deref(), Some("Hero"));
    assert!(world.redo().is_err());
}

#[test]
fn delete_and_undo_restores_children() {
    let mut world = world();
    let parent = entity_from(
        world
            .run_command("entity.spawn", json!({"name": "Level"}))
            .unwrap(),
    );
    let child = entity_from(
        world
            .run_command(
                "entity.spawn",
                json!({"name": "Enemy", "parent": parent.to_bits()}),
            )
            .unwrap(),
    );
    world.get_resource_mut::<Selection>().unwrap().set([child]);

    world.execute(DespawnEntity::new(&world, parent)).unwrap();
    assert!(!visible(&world, parent) && !visible(&world, child));
    assert!(
        world.resource::<Selection>().is_empty(),
        "deleted nodes leave the selection"
    );

    world.undo().unwrap();
    assert!(visible(&world, parent) && visible(&world, child));
    assert_eq!(
        world.get::<ChildOf>(child).map(|c| c.parent()),
        Some(parent)
    );
}

#[test]
fn deleted_entities_are_despawned_when_history_drops_them() {
    let mut world = world();
    world.insert_resource(History::with_limit(1));
    let spawn = SpawnEntity::new(&mut world, "Temp");
    let temp = spawn.entity();
    world.execute(spawn).unwrap();
    world.execute(DespawnEntity::new(&world, temp)).unwrap();
    assert!(
        world.get_entity(temp).is_ok(),
        "kept while undo can still restore it"
    );

    // A third step pushes the delete out of the one-step history.
    let other = SpawnEntity::new(&mut world, "Other");
    world.execute(other).unwrap();
    assert!(
        world.get_entity(temp).is_err(),
        "despawned for real once unreachable"
    );
}

#[test]
fn undone_spawn_is_despawned_when_redo_is_cleared() {
    let mut world = world();
    let spawn = SpawnEntity::new(&mut world, "Ghost");
    let ghost = spawn.entity();
    world.execute(spawn).unwrap();
    world.undo().unwrap();
    let fresh = SpawnEntity::new(&mut world, "New");
    world.execute(fresh).unwrap();
    assert!(world.get_entity(ghost).is_err());
}

#[test]
fn set_field_by_json_and_undo() {
    let mut world = world();
    let enemy = node(
        &mut world,
        (
            Name::new("Enemy"),
            Health {
                current: 10.0,
                max: 10.0,
            },
        ),
    );

    world
        .run_command(
            "component.set",
            json!({"entity": enemy.to_bits(), "component": "Health", "path": "current", "value": 3.5}),
        )
        .unwrap();
    assert_eq!(world.get::<Health>(enemy).unwrap().current, 3.5);
    assert_eq!(
        reflect_json::field_json(&world, enemy, "Health", "current").unwrap(),
        json!(3.5)
    );

    world.undo().unwrap();
    assert_eq!(world.get::<Health>(enemy).unwrap().current, 10.0);

    // The whole component at once, by its full type path.
    let full_path = std::any::type_name::<Health>();
    let set_all = SetField::from_json(
        &world,
        enemy,
        full_path,
        "",
        json!({"current": 1.0, "max": 5.0}),
    )
    .unwrap();
    world.execute(set_all).unwrap();
    assert_eq!(
        world.get::<Health>(enemy),
        Some(&Health {
            current: 1.0,
            max: 5.0
        })
    );
}

#[test]
fn bad_input_is_rejected_without_changes() {
    let mut world = world();
    let enemy = node(&mut world, (Name::new("Enemy"), Health::default()));
    let history_len = |w: &World| w.resource::<History>().undo_len();

    let wrong_type = world.run_command(
        "component.set",
        json!({"entity": enemy.to_bits(), "component": "Health", "path": "current", "value": "lots"}),
    );
    assert!(wrong_type.is_err());
    let wrong_path = world.run_command(
        "component.set",
        json!({"entity": enemy.to_bits(), "component": "Health", "path": "armor", "value": 1}),
    );
    assert!(wrong_path.is_err());
    assert!(world.run_command("does.not.exist", json!({})).is_err());
    assert!(
        world
            .run_command(
                "entity.rename",
                json!({"entity": enemy.to_bits(), "name": "  "})
            )
            .is_err()
    );
    assert_eq!(history_len(&world), 0, "failed commands leave no history");
}

#[test]
fn add_and_remove_components() {
    let mut world = world();
    let e = node(&mut world, Name::new("Runner"));
    world
        .run_command(
            "component.add",
            json!({"entity": e.to_bits(), "component": "Speed"}),
        )
        .unwrap();
    assert_eq!(world.get::<Speed>(e), Some(&Speed(0.0)));
    world.get_mut::<Speed>(e).unwrap().0 = 7.0;

    world
        .run_command(
            "component.remove",
            json!({"entity": e.to_bits(), "component": "Speed"}),
        )
        .unwrap();
    assert!(world.get::<Speed>(e).is_none());
    world.undo().unwrap();
    assert_eq!(
        world.get::<Speed>(e),
        Some(&Speed(7.0)),
        "undo restores the exact value"
    );

    let components = reflect_json::components_of(&world, e).unwrap();
    assert!(components.iter().any(|c| c.ends_with("Speed")));
}

#[test]
fn reparent_keeps_sibling_order_on_undo() {
    let mut world = world();
    let root = node(&mut world, Name::new("Root"));
    let a = world.spawn((Name::new("A"), ChildOf(root))).id();
    let b = world.spawn((Name::new("B"), ChildOf(root))).id();
    let c = world.spawn((Name::new("C"), ChildOf(root))).id();

    world.execute(ReparentEntity::new(a, Some(c))).unwrap();
    assert_eq!(world.get::<ChildOf>(a).unwrap().parent(), c);
    world.undo().unwrap();
    let children: Vec<Entity> = world.get::<Children>(root).unwrap().iter().collect();
    assert_eq!(children, vec![a, b, c]);

    assert!(
        world.execute(ReparentEntity::new(root, Some(a))).is_err(),
        "cannot parent a node to its own child"
    );
}

#[test]
fn batch_is_one_step_and_all_or_nothing() {
    let mut world = world();
    let a = node(&mut world, Name::new("A"));
    let b = node(&mut world, Name::new("B"));
    world
        .execute(
            Batch::new("Rename two")
                .with(RenameEntity::new(a, "A2"))
                .with(RenameEntity::new(b, "B2")),
        )
        .unwrap();
    assert_eq!(world.resource::<History>().undo_len(), 1);
    world.undo().unwrap();
    assert_eq!(name(&world, a).as_deref(), Some("A"));
    assert_eq!(name(&world, b).as_deref(), Some("B"));

    let gone = world.spawn_empty().id();
    world.despawn(gone);
    let failing = Batch::new("Half bad")
        .with(RenameEntity::new(a, "Changed"))
        .with(RenameEntity::new(gone, "Nope"));
    assert!(world.execute(failing).is_err());
    assert_eq!(
        name(&world, a).as_deref(),
        Some("A"),
        "the good half was rolled back"
    );
}

#[test]
fn dirty_flag_follows_save_point() {
    let mut world = world();
    let e = node(&mut world, Name::new("E"));
    assert!(!world.resource::<History>().is_dirty());

    world.execute(RenameEntity::new(e, "One")).unwrap();
    assert!(world.resource::<History>().is_dirty());
    world.resource_mut::<History>().mark_saved();
    assert!(!world.resource::<History>().is_dirty());

    world.execute(RenameEntity::new(e, "Two")).unwrap();
    assert!(world.resource::<History>().is_dirty());
    world.undo().unwrap();
    assert!(
        !world.resource::<History>().is_dirty(),
        "back at the saved state"
    );

    // Branching away from the saved state can never get back to it.
    world.undo().unwrap();
    world.execute(RenameEntity::new(e, "Three")).unwrap();
    world.undo().unwrap();
    assert!(world.resource::<History>().is_dirty());
}

#[test]
fn undo_and_redo_by_name_and_registry_listing() {
    let mut world = world();
    let e = node(&mut world, Name::new("E"));
    world.execute(RenameEntity::new(e, "F")).unwrap();
    world.run_command("history.undo", json!({})).unwrap();
    assert_eq!(name(&world, e).as_deref(), Some("E"));
    world.run_command("history.redo", json!(null)).unwrap();
    assert_eq!(name(&world, e).as_deref(), Some("F"));

    let registry = world.resource::<oddity_core::CommandRegistry>();
    let ids: Vec<_> = registry.specs().map(|s| s.id).collect();
    for id in [
        "entity.spawn",
        "entity.despawn",
        "component.set",
        "selection.set",
        "history.undo",
    ] {
        assert!(ids.contains(&id), "{id} missing");
    }
}

#[test]
fn commands_queue_from_systems() {
    use oddity_core::EditorCommandsExt;
    let mut app = headless_app();
    let root = app.world_mut().spawn((Name::new("Main"), LevelRoot)).id();
    let e = app
        .world_mut()
        .spawn((Name::new("Old"), ChildOf(root)))
        .id();
    app.add_systems(bevy_app::Update, move |mut commands: Commands| {
        commands.execute(RenameEntity::new(e, "New"));
    });
    app.update();
    assert_eq!(name(app.world(), e).as_deref(), Some("New"));
    assert_eq!(app.world().resource::<History>().undo_len(), 1);
}

#[test]
fn history_labels_read_well() {
    let mut world = world();
    let spawn = SpawnEntity::new(&mut world, "Player");
    let player = spawn.entity();
    world.execute(spawn).unwrap();
    world.execute(DespawnEntity::new(&world, player)).unwrap();
    let labels: Vec<String> = world
        .resource::<History>()
        .undo_steps()
        .map(|step| step.label.into_owned())
        .collect();
    assert_eq!(labels, vec!["Add Player", "Delete Player"]);
    let _: &dyn EditorCommand = &RenameEntity::new(player, "x");
}

#[test]
fn commands_list_reports_every_spec_with_its_kind() {
    let mut world = world();
    let listed = world.run_command("commands.list", json!({})).unwrap();
    let listed = listed["commands"].as_array().unwrap();
    let registry = world.resource::<oddity_core::CommandRegistry>();
    assert_eq!(listed.len(), registry.specs().count());

    let kind_of = |id: &str| {
        listed
            .iter()
            .find(|spec| spec["id"] == id)
            .unwrap_or_else(|| panic!("{id} missing"))["kind"]
            .clone()
    };
    assert_eq!(kind_of("entity.spawn"), "edit");
    assert_eq!(kind_of("component.set"), "edit");
    assert_eq!(kind_of("selection.set"), "action");
    assert_eq!(kind_of("history.undo"), "action");
    for query in [
        "level.tree",
        "component.get",
        "component.list",
        "selection.get",
        "history.list",
        "commands.list",
    ] {
        assert_eq!(kind_of(query), "query");
    }
}

#[test]
fn queries_leave_the_history_untouched() {
    let mut world = world();
    let root = level_root(&mut world).unwrap();
    let enemy = world
        .spawn((
            Name::new("Goblin"),
            Health {
                current: 4.0,
                max: 10.0,
            },
            ChildOf(root),
        ))
        .id();
    world.execute(RenameEntity::new(enemy, "Goblin2")).unwrap();
    world.execute(RenameEntity::new(enemy, "Goblin3")).unwrap();
    world.undo().unwrap();
    world.get_resource_mut::<Selection>().unwrap().set([enemy]);
    let before = world.resource::<History>().to_json();

    let entity = enemy.to_bits();
    let ids: Vec<&str> = world
        .resource::<oddity_core::CommandRegistry>()
        .specs()
        .filter(|spec| spec.kind == oddity_core::CommandKind::Query)
        .map(|spec| spec.id)
        .collect();
    for id in ids {
        let args = json!({"entity": entity, "component": "Health", "path": "current"});
        world
            .run_command(id, args)
            .unwrap_or_else(|e| panic!("{id}: {e}"));
    }
    assert_eq!(world.resource::<History>().to_json(), before);

    let tree = world.run_command("level.tree", json!({})).unwrap();
    assert_eq!(tree["root"]["name"], "Main");
    assert_eq!(tree["root"]["children"][0]["name"], "Goblin2");
    let value = world
        .run_command(
            "component.get",
            json!({"entity": entity, "component": "Health", "path": "current"}),
        )
        .unwrap();
    assert_eq!(value["value"], json!(4.0));
    let selection = world.run_command("selection.get", json!({})).unwrap();
    assert_eq!(selection["primary"], json!(entity));
    let history = world.run_command("history.list", json!({})).unwrap();
    assert_eq!(history["undo"][0]["label"], "Rename to Goblin2");
    assert_eq!(history["redo"][0]["label"], "Rename to Goblin3");
}

#[test]
fn deleted_nodes_are_left_out_of_queries() {
    let mut world = world();
    let root = level_root(&mut world).unwrap();
    let crate_ = world.spawn((Name::new("Crate"), ChildOf(root))).id();
    world.execute(DespawnEntity::new(&world, crate_)).unwrap();

    let tree = world.run_command("level.tree", json!({})).unwrap();
    assert_eq!(tree["root"]["children"], json!([]));
    assert!(
        world
            .run_command("component.list", json!({"entity": crate_.to_bits()}))
            .is_err()
    );
}

#[test]
fn every_step_replays_from_its_invocation() {
    let mut world = world();
    let a = node(
        &mut world,
        (
            Name::new("A"),
            Health {
                current: 1.0,
                max: 1.0,
            },
        ),
    );
    let b = node(&mut world, Name::new("B"));
    world.execute(RenameEntity::new(a, "Renamed")).unwrap();
    world.execute(ReparentEntity::new(a, Some(b))).unwrap();
    // Built from a reflected value rather than JSON: the invocation still
    // carries the value as JSON, read back after it applied.
    world
        .execute(SetField::new(a, "Health", "max", Box::new(9.0_f32)))
        .unwrap();

    let history = world.run_command("history.list", json!({})).unwrap();
    let steps: Vec<serde_json::Value> = history["undo"]
        .as_array()
        .unwrap()
        .iter()
        .map(|step| step["invocation"].clone())
        .collect();
    assert_eq!(steps[0]["id"], "entity.rename");
    assert_eq!(steps[1]["args"]["parent"], json!(b.to_bits()));
    assert_eq!(steps[2]["args"]["value"], json!(9.0));

    // Undo everything, then replay the invocations by name.
    for _ in 0..3 {
        world.undo().unwrap();
    }
    assert_eq!(name(&world, a).as_deref(), Some("A"));
    for step in &steps {
        world
            .run_command(step["id"].as_str().unwrap(), step["args"].clone())
            .unwrap();
    }
    assert_eq!(name(&world, a).as_deref(), Some("Renamed"));
    assert_eq!(world.get::<ChildOf>(a).map(|c| c.parent()), Some(b));
    assert_eq!(world.get::<Health>(a).unwrap().max, 9.0);
}

#[test]
fn undoing_past_dropped_steps_still_counts_as_unsaved() {
    let mut world = world();
    world.insert_resource(History::with_limit(2));
    let e = node(&mut world, Name::new("A"));
    // Saved as opened; the first rename falls out of the two-step history.
    for new_name in ["B", "C", "D"] {
        world.execute(RenameEntity::new(e, new_name)).unwrap();
    }
    world.undo().unwrap();
    world.undo().unwrap();
    assert_eq!(name(&world, e).as_deref(), Some("B"));
    assert!(
        world.resource::<History>().is_dirty(),
        "the dropped rename is still unsaved"
    );

    // Saved after the step that is dropped next: undoing everything left
    // gets back to exactly what was saved.
    world.redo().unwrap();
    world.redo().unwrap();
    world.undo().unwrap();
    world.resource_mut::<History>().mark_saved();
    world.redo().unwrap();
    world.execute(RenameEntity::new(e, "E")).unwrap();
    world.undo().unwrap();
    world.undo().unwrap();
    assert_eq!(name(&world, e).as_deref(), Some("C"));
    assert!(!world.resource::<History>().is_dirty());
}

#[test]
fn a_failed_spawn_leaves_no_entity_behind() {
    let mut world = world();
    let count = |world: &World| world.iter_entities().count();
    let before = count(&world);
    let missing = Entity::from_raw_u32(999_999).unwrap();
    let result = world.run_command(
        "entity.spawn",
        json!({"name": "Orphan", "parent": missing.to_bits()}),
    );
    assert!(result.is_err());
    assert_eq!(count(&world), before);
}

#[test]
fn spawning_without_a_parent_puts_the_node_at_the_top_of_the_level() {
    let mut world = world();
    world
        .run_command("entity.spawn", json!({"name": "Crate"}))
        .unwrap();
    let tree = world.run_command("level.tree", json!({})).unwrap();
    assert_eq!(tree["root"]["children"][0]["name"], "Crate");

    let mut empty = headless_app();
    assert!(matches!(
        empty
            .world_mut()
            .run_command("entity.spawn", json!({"name": "Crate"})),
        Err(CommandError::NoLevel)
    ));
}

#[test]
fn commands_leave_editor_ui_and_the_level_root_alone() {
    let mut world = world();
    let root = level_root(&mut world).unwrap();
    let ui = world.spawn(Name::new("Some UI panel")).id();
    let crate_ = node(&mut world, Name::new("Crate"));
    let mut run = |id: &str, args: serde_json::Value| world.run_command(id, args);

    for (id, args) in [
        ("entity.despawn", json!({"entity": ui.to_bits()})),
        (
            "entity.rename",
            json!({"entity": ui.to_bits(), "name": "X"}),
        ),
        (
            "component.remove",
            json!({"entity": ui.to_bits(), "component": "Name"}),
        ),
        ("selection.set", json!({"entities": [ui.to_bits()]})),
        (
            "entity.reparent",
            json!({"entity": crate_.to_bits(), "parent": ui.to_bits()}),
        ),
        ("entity.spawn", json!({"name": "X", "parent": ui.to_bits()})),
        ("entity.despawn", json!({"entity": root.to_bits()})),
        (
            "entity.reparent",
            json!({"entity": root.to_bits(), "parent": crate_.to_bits()}),
        ),
        (
            "component.remove",
            json!({"entity": root.to_bits(), "component": "LevelRoot"}),
        ),
        (
            "component.add",
            json!({"entity": crate_.to_bits(), "component": "LevelRoot"}),
        ),
    ] {
        assert!(run(id, args.clone()).is_err(), "{id} {args} should fail");
    }
    assert_eq!(
        world.get::<Name>(ui).map(Name::as_str),
        Some("Some UI panel")
    );
    assert!(world.get::<LevelRoot>(root).is_some());
    assert_eq!(world.resource::<History>().undo_len(), 0);
}

#[test]
fn deleted_nodes_cannot_be_edited() {
    let mut world = world();
    let crate_ = node(&mut world, Name::new("Crate"));
    world.execute(DespawnEntity::new(&world, crate_)).unwrap();
    assert!(
        world
            .run_command(
                "entity.rename",
                json!({"entity": crate_.to_bits(), "name": "Ghost"})
            )
            .is_err()
    );
    assert_eq!(name(&world, crate_).as_deref(), Some("Crate"));
}

#[test]
fn history_list_reports_who_made_each_step() {
    let mut world = world();
    let agent = Source::Mcp {
        client: "Claude".into(),
    };
    let crate_ = entity_from(
        world
            .run_command("entity.spawn", json!({"name": "Crate"}))
            .unwrap(),
    );
    world
        .run_command_as(
            "entity.rename",
            json!({"entity": crate_.to_bits(), "name": "Barrel"}),
            agent.clone(),
        )
        .unwrap();

    let history = world.run_command("history.list", json!({})).unwrap();
    assert_eq!(history["undo"][0]["source"], json!({"kind": "user"}));
    assert_eq!(
        history["undo"][1]["source"],
        json!({"kind": "mcp", "client": "Claude"})
    );

    // Undo keeps the step's source for redo.
    world.undo().unwrap();
    let history = world.resource::<History>();
    assert_eq!(history.redo_steps().next().unwrap().source, &agent);
}

#[test]
fn every_command_is_announced_with_its_source_and_result() {
    let mut world = world();
    let crate_ = node(&mut world, Name::new("Crate"));
    world
        .resource_mut::<bevy_ecs::message::Messages<CommandExecuted>>()
        .clear();

    world
        .execute_as(RenameEntity::new(crate_, "Barrel"), Source::Cli)
        .unwrap();
    let ghost = Entity::from_raw_u32(999_999).unwrap();
    assert!(world.execute(RenameEntity::new(ghost, "Nope")).is_err());
    world.undo().unwrap();
    world.run_command("level.tree", json!({})).unwrap();

    let root = level_root(&mut world);
    let sent: Vec<CommandExecuted> = world
        .resource_mut::<bevy_ecs::message::Messages<CommandExecuted>>()
        .drain()
        .collect();
    let ids: Vec<&str> = sent.iter().map(|m| m.id).collect();
    assert_eq!(
        ids,
        ["entity.rename", "entity.rename", "history.undo"],
        "queries are not announced"
    );
    assert_eq!(sent[0].source, Source::Cli);
    assert_eq!(sent[0].args["name"], "Barrel");
    assert_eq!(sent[0].document, root);
    assert!(sent[0].result.is_ok());
    assert!(matches!(sent[1].result, Err(CommandError::NoEntity(_))));
    assert_eq!(sent[2].source, Source::User);
}

/// Health's `current` on `entity`, set by name as the Inspector will.
fn set_current(world: &mut World, entity: Entity, value: f32) {
    world
        .run_command(
            "component.set",
            json!({"entity": entity.to_bits(), "component": "Health", "path": "current", "value": value}),
        )
        .unwrap();
}

fn current(world: &World, entity: Entity) -> f32 {
    world.get::<Health>(entity).unwrap().current
}

#[test]
fn scrubbing_a_value_over_50_frames_is_one_undo_step() {
    let mut app = headless_app();
    app.register_type::<Health>();
    let world = app.world_mut();
    let root = world.spawn((Name::new("Main"), LevelRoot)).id();
    let goblin = world
        .spawn((Name::new("Goblin"), Health::default(), ChildOf(root)))
        .id();

    app.world_mut().resource_mut::<History>().begin_gesture();
    for frame in 1..=50 {
        set_current(app.world_mut(), goblin, frame as f32);
        app.update();
    }
    app.world_mut().resource_mut::<History>().end_gesture();

    let world = app.world_mut();
    assert_eq!(world.resource::<History>().undo_len(), 1);
    assert_eq!(current(world, goblin), 50.0);
    world.undo().unwrap();
    assert_eq!(
        current(world, goblin),
        0.0,
        "one undo goes all the way back"
    );
    world.redo().unwrap();
    assert_eq!(current(world, goblin), 50.0);

    // The step replays as the value it ended on.
    let history = world.run_command("history.list", json!({})).unwrap();
    assert_eq!(
        history["undo"][0]["invocation"]["args"]["value"],
        json!(50.0)
    );
}

#[test]
fn edits_merge_only_within_one_gesture_on_one_field() {
    let mut world = world();
    let goblin = node(&mut world, (Name::new("Goblin"), Health::default()));
    let steps = |world: &World| world.resource::<History>().undo_len();

    // Without a gesture every edit is its own step, so an agent setting a
    // field twice can undo once.
    set_current(&mut world, goblin, 1.0);
    set_current(&mut world, goblin, 2.0);
    assert_eq!(steps(&world), 2);

    // A gesture never folds into a step made before it.
    world.resource_mut::<History>().begin_gesture();
    set_current(&mut world, goblin, 3.0);
    set_current(&mut world, goblin, 4.0);
    assert_eq!(steps(&world), 3);

    // Another field, or another source, starts a new step.
    world
        .run_command(
            "component.set",
            json!({"entity": goblin.to_bits(), "component": "Health", "path": "max", "value": 9.0}),
        )
        .unwrap();
    assert_eq!(steps(&world), 4);
    world
        .run_command_as(
            "component.set",
            json!({"entity": goblin.to_bits(), "component": "Health", "path": "max", "value": 8.0}),
            Source::Cli,
        )
        .unwrap();
    assert_eq!(steps(&world), 5);
    world.resource_mut::<History>().end_gesture();
}

#[test]
fn saving_mid_gesture_then_merging_counts_as_unsaved() {
    let mut world = world();
    let goblin = node(&mut world, (Name::new("Goblin"), Health::default()));
    world.resource_mut::<History>().begin_gesture();
    set_current(&mut world, goblin, 1.0);
    world.resource_mut::<History>().mark_saved();
    set_current(&mut world, goblin, 2.0);
    world.resource_mut::<History>().end_gesture();
    assert!(world.resource::<History>().is_dirty());
}

#[test]
fn a_three_command_batch_sent_as_json_undoes_in_one_step() {
    let mut world = world();
    let output = world
        .run_command(
            "batch",
            json!({
                "label": "Build a wall",
                "commands": [
                    {"id": "entity.spawn", "args": {"name": "Brick1"}},
                    {"id": "entity.spawn", "args": {"name": "Brick2"}},
                    {"id": "entity.spawn", "args": {"name": "Brick3"}},
                ],
            }),
        )
        .unwrap();
    let bricks: Vec<Entity> = output
        .as_array()
        .unwrap()
        .iter()
        .map(|out| entity_from(out.clone()))
        .collect();
    assert_eq!(bricks.len(), 3);
    let tree = |world: &mut World| world.run_command("level.tree", json!({})).unwrap();
    assert_eq!(
        tree(&mut world)["root"]["children"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    let history = world.resource::<History>();
    assert_eq!(history.undo_len(), 1);
    assert_eq!(history.undo_label().as_deref(), Some("Build a wall"));

    world.undo().unwrap();
    assert_eq!(tree(&mut world)["root"]["children"], json!([]));
    world.redo().unwrap();
    assert!(bricks.iter().all(|brick| visible(&world, *brick)));
}

#[test]
fn a_batch_whose_last_command_fails_leaves_the_level_unchanged() {
    let mut world = world();
    let crate_ = node(&mut world, Name::new("Crate"));
    let entities = |world: &World| world.iter_entities().count();
    let before = entities(&world);
    let missing = Entity::from_raw_u32(999_999).unwrap();

    let result = world.run_command(
        "batch",
        json!({"commands": [
            {"id": "entity.rename", "args": {"entity": crate_.to_bits(), "name": "Barrel"}},
            {"id": "entity.spawn", "args": {"name": "Lid", "parent": crate_.to_bits()}},
            {"id": "entity.rename", "args": {"entity": missing.to_bits(), "name": "Nope"}},
        ]}),
    );
    assert!(result.is_err());
    assert_eq!(name(&world, crate_).as_deref(), Some("Crate"));
    assert_eq!(entities(&world), before, "the spawned lid is gone too");
    assert_eq!(world.resource::<History>().undo_len(), 0);
}

#[test]
fn a_batch_step_can_use_what_an_earlier_step_made() {
    let mut world = world();
    let runner = node(&mut world, Name::new("Runner"));
    let e = runner.to_bits();
    world
        .run_command(
            "batch",
            json!({"commands": [
                {"id": "component.add", "args": {"entity": e, "component": "Speed"}},
                {"id": "component.set", "args": {"entity": e, "component": "Speed", "path": "", "value": 4.0}},
            ]}),
        )
        .unwrap();
    assert_eq!(world.get::<Speed>(runner), Some(&Speed(4.0)));
    world.undo().unwrap();
    assert!(world.get::<Speed>(runner).is_none());
}

#[test]
fn a_batch_only_holds_undoable_commands() {
    let mut world = world();
    for id in [
        "history.undo",
        "selection.set",
        "level.tree",
        "no.such.command",
    ] {
        let result = world.run_command(
            "batch",
            json!({"commands": [{"id": id, "args": {"entities": []}}]}),
        );
        assert!(result.is_err(), "{id} was accepted");
    }
    assert!(world.run_command("batch", json!({"commands": []})).is_err());
}

#[test]
fn a_group_turns_an_agents_calls_into_one_step() {
    let mut world = world();
    let agent = Source::Mcp {
        client: "Claude".into(),
    };
    let mut run =
        |id: &str, args: serde_json::Value| world.run_command_as(id, args, agent.clone()).unwrap();
    run(
        "history.group_begin",
        json!({"label": "Agent: build a wall of crates"}),
    );
    let first = entity_from(run("entity.spawn", json!({"name": "Crate"})));
    run("entity.spawn", json!({"name": "Crate2"}));
    run(
        "entity.rename",
        json!({"entity": first.to_bits(), "name": "Crate1"}),
    );
    run("history.group_end", json!({}));

    let history = world.run_command("history.list", json!({})).unwrap();
    let steps = history["undo"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["label"], "Agent: build a wall of crates");
    assert_eq!(
        steps[0]["source"],
        json!({"kind": "mcp", "client": "Claude"})
    );

    world.undo().unwrap();
    let tree = world.run_command("level.tree", json!({})).unwrap();
    assert_eq!(tree["root"]["children"], json!([]));
    world.redo().unwrap();
    assert_eq!(name(&world, first).as_deref(), Some("Crate1"));
}

#[test]
fn groups_must_be_opened_once_and_closed_once() {
    let mut world = world();
    assert!(world.run_command("history.group_end", json!({})).is_err());
    world
        .run_command("history.group_begin", json!({"label": "One"}))
        .unwrap();
    assert!(
        world
            .run_command("history.group_begin", json!({"label": "Two"}))
            .is_err()
    );
    // An empty group adds no step.
    world.run_command("history.group_end", json!({})).unwrap();
    assert_eq!(world.resource::<History>().undo_len(), 0);
}

#[test]
fn fields_are_listed_with_their_rust_types() {
    let mut world = world();
    let goblin = node(
        &mut world,
        (
            Name::new("Goblin"),
            Health {
                current: 4.0,
                max: 10.0,
            },
            Speed(2.0),
        ),
    );
    let fields = reflect_json::fields_of(&world, goblin, "Health").unwrap();
    let summary: Vec<(&str, &str)> = fields
        .iter()
        .map(|f| (f.name.as_str(), f.type_path.as_str()))
        .collect();
    assert_eq!(summary, [("current", "f32"), ("max", "f32")]);
    assert_eq!(fields[0].value, json!(4.0));

    // Not a struct with named fields: one unnamed field, the whole value.
    let speed = reflect_json::fields_of(&world, goblin, "Speed").unwrap();
    assert_eq!(speed.len(), 1);
    assert_eq!(speed[0].name, "");
    assert!(speed[0].type_path.ends_with("Speed"));
}
