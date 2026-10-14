//! Phase 1 exit criterion: commands run headless, with undo.

use bevy_ecs::{
    entity::Entity, entity_disabling::Disabled, hierarchy::ChildOf, name::Name, prelude::*,
};
use bevy_reflect::{Reflect, prelude::ReflectDefault};
use oddity_core::{
    EditorCommand, EditorWorldExt, History, Selection,
    commands::{Batch, DespawnEntity, RenameEntity, ReparentEntity, SetField, SpawnEntity},
    headless_app, reflect_json,
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

fn world() -> World {
    let mut app = headless_app();
    app.register_type::<Health>().register_type::<Speed>();
    std::mem::take(app.world_mut())
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
    let enemy = world
        .spawn((
            Name::new("Enemy"),
            Health {
                current: 10.0,
                max: 10.0,
            },
        ))
        .id();

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
    let enemy = world.spawn((Name::new("Enemy"), Health::default())).id();
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
    let e = world.spawn(Name::new("Runner")).id();
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
    let root = world.spawn(Name::new("Root")).id();
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
    let a = world.spawn(Name::new("A")).id();
    let b = world.spawn(Name::new("B")).id();
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
    let e = world.spawn(Name::new("E")).id();
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
    let e = world.spawn(Name::new("E")).id();
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
    let e = app.world_mut().spawn(Name::new("Old")).id();
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
        .undo_labels()
        .map(|l| l.into_owned())
        .collect();
    assert_eq!(labels, vec!["Add Player", "Delete Player"]);
    let _: &dyn EditorCommand = &RenameEntity::new(player, "x");
}
