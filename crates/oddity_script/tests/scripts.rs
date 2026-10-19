//! A1's exit test: scripts register themselves, and `ReflectScript` finds
//! them with their ID, order, saved fields, defaults and Inspector
//! attributes.

use std::any::TypeId;

use bevy::{
    prelude::*,
    reflect::{TypeRegistry, std_traits::ReflectDefault},
};
use oddity_script::{
    NodeRef, ReflectScript, Script,
    attributes::{Export, Header, Min, Multiline, Range, Tooltip},
    script,
};

#[script(id = "4f9d2c71")]
struct Health {
    #[default(100.0)]
    #[range(0.0, 500.0)]
    pub max: f32,
    /// How much is left.
    #[show]
    current: f32,
    #[hide]
    pub cached: f32,
    hits: u32,
}

impl Script for Health {}

#[script(order = -10)]
struct Player {
    #[header("Movement")]
    #[default(6.0)]
    #[min(0)]
    pub speed: f32,
    #[tooltip("Where bullets come from")]
    pub muzzle: NodeRef,
    #[multiline]
    #[default(String::from("Hero"))]
    pub title: String,
    #[default(Vec3::new(0.0, 10.0, 8.0))]
    pub offset: Vec3,
}

impl Script for Player {}

/// A script with no fields works as a tag.
#[script]
struct EnemyCountLabel;

impl Script for EnemyCountLabel {}

fn registry() -> TypeRegistry {
    let mut registry = TypeRegistry::new();
    registry.register_derived_types();
    registry
}

fn script_data<T: 'static>(registry: &TypeRegistry) -> &ReflectScript {
    registry
        .get(TypeId::of::<T>())
        .and_then(|registration| registration.data::<ReflectScript>())
        .expect("registered as a script")
}

#[test]
fn scripts_register_themselves_with_their_id_and_order() {
    let registry = registry();
    let mut scripts: Vec<&str> = registry
        .iter()
        .filter(|registration| registration.data::<ReflectScript>().is_some())
        .map(|registration| registration.type_info().type_path_table().short_path())
        .collect();
    scripts.sort_unstable();
    assert_eq!(scripts, ["EnemyCountLabel", "Health", "Player"]);

    let health = script_data::<Health>(&registry);
    assert_eq!((health.id(), health.order()), (Some("4f9d2c71"), 0));
    let player = script_data::<Player>(&registry);
    assert_eq!((player.id(), player.order()), (None, -10));
}

#[test]
fn pub_and_show_fields_are_saved_hide_and_private_ones_are_not() {
    let registry = registry();
    let info = registry
        .get(TypeId::of::<Health>())
        .unwrap()
        .type_info()
        .as_struct()
        .unwrap();
    let saved = |name: &str| info.field(name).unwrap().has_attribute::<Export>();
    assert!(saved("max"));
    assert!(saved("current"), "#[show] saves a private field");
    assert!(!saved("cached"), "#[hide] leaves a pub field out");
    assert!(!saved("hits"), "private fields are runtime-only");
}

#[test]
fn defaults_come_from_the_field_defaults() {
    let registry = registry();
    let make = |id: TypeId| {
        registry
            .get(id)
            .and_then(|r| r.data::<ReflectDefault>())
            .unwrap()
            .default()
    };
    let health = make(TypeId::of::<Health>());
    let health = health.downcast_ref::<Health>().unwrap();
    assert_eq!((health.max, health.current, health.hits), (100.0, 0.0, 0));

    let player = make(TypeId::of::<Player>());
    let player = player.downcast_ref::<Player>().unwrap();
    assert_eq!(player.speed, 6.0);
    assert_eq!(player.title, "Hero");
    assert_eq!(player.offset, Vec3::new(0.0, 10.0, 8.0));
    assert!(player.muzzle.is_none());
}

#[test]
fn inspector_attributes_are_recorded_on_their_fields() {
    let registry = registry();
    let fields = |id: TypeId| registry.get(id).unwrap().type_info().as_struct().unwrap();
    let health = fields(TypeId::of::<Health>());
    assert_eq!(
        health.field("max").unwrap().get_attribute::<Range>(),
        Some(&Range {
            min: 0.0,
            max: 500.0
        })
    );
    assert_eq!(
        health.field("current").unwrap().get_attribute::<Tooltip>(),
        Some(&Tooltip("How much is left.")),
        "a doc comment is the tooltip"
    );

    let player = fields(TypeId::of::<Player>());
    let speed = player.field("speed").unwrap();
    assert_eq!(speed.get_attribute::<Header>(), Some(&Header("Movement")));
    assert_eq!(speed.get_attribute::<Min>(), Some(&Min(0.0)));
    assert_eq!(
        player.field("muzzle").unwrap().get_attribute::<Tooltip>(),
        Some(&Tooltip("Where bullets come from"))
    );
    assert!(player.field("title").unwrap().has_attribute::<Multiline>());
}

#[test]
fn reflect_script_reaches_a_script_of_its_own_type_only() {
    let registry = registry();
    let player_script = script_data::<Player>(&registry);
    let mut player = Player::default();
    assert!(player_script.get_mut(&mut player).is_some());
    assert!(player_script.get(&Health::default()).is_none());
}
