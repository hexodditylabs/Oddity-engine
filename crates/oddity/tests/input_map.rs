//! A8: the shooter's `oddity.toml` input map drives its Player the way the
//! spec's example reads it: `axis2("move")` and `action_pressed("fire")`.

use std::path::Path;

use bevy::input::{
    ButtonState, InputPlugin,
    keyboard::{Key, KeyboardInput, NativeKey},
};
use oddity::prelude::*;
use oddity::{InputMap, ScriptPlugin, input_map};
use oddity_project::{InputSection, Project};

fn shooter() -> Project {
    Project::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/shooter")).unwrap()
}

#[derive(Resource, Default)]
struct Seen {
    movement: Vec2,
    fire: bool,
}

#[script]
struct Player;

impl Script for Player {
    fn _onprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        let seen = Seen {
            movement: ctx.input().axis2("move"),
            fire: ctx.input().action_pressed("fire"),
        };
        ctx.world_mut().insert_resource(seen);
    }
}

fn press(app: &mut App, key_code: KeyCode) {
    app.world_mut().write_message(KeyboardInput {
        key_code,
        logical_key: Key::Unidentified(NativeKey::Unidentified),
        state: ButtonState::Pressed,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
}

#[test]
fn the_shooters_input_map_moves_and_fires_the_player() {
    let map = input_map(&shooter().file.input).unwrap();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, InputPlugin, ScriptPlugin))
        .insert_resource(map)
        .init_resource::<Seen>();
    app.world_mut().spawn(Player);

    press(&mut app, KeyCode::KeyA);
    press(&mut app, KeyCode::Space);
    app.update();
    let seen = app.world().resource::<Seen>();
    assert_eq!(seen.movement, Vec2::NEG_X);
    assert!(seen.fire);
}

#[test]
fn a_misspelt_binding_names_its_action() {
    let mut input = InputSection::default();
    input
        .actions
        .insert("jump".into(), vec!["Space".into(), "Spcae".into()]);
    let error = input_map(&input).unwrap_err();
    assert_eq!(error.entry, "jump");
    assert_eq!(error.error.binding, "Spcae");
    assert!(error.to_string().starts_with("input map entry \"jump\""));
    assert_eq!(input_map(&InputSection::default()), Ok(InputMap::default()));
}
