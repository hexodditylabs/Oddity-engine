//! A8: scripts read raw keys and the mouse through `ctx.input()`, and the
//! project's actions and axes through the input map.

use bevy::{
    input::{
        ButtonState, InputPlugin,
        gamepad::{Gamepad, GamepadAxis},
        keyboard::{Key, KeyboardInput, NativeKey},
        mouse::{MouseButtonInput, MouseMotion},
    },
    prelude::*,
};
use oddity_script::{
    Axis2Binding, Binding, Ctx, DisabledScripts, InputMap, Script, ScriptPlugin, Stick, script,
};

/// What the script saw this frame.
#[derive(Resource, Default, Debug)]
struct Seen {
    space: (bool, bool, bool),
    fire: (bool, bool, bool),
    movement: Vec2,
    steer: f32,
    motion: Vec2,
}

#[script]
struct Reader;

impl Script for Reader {
    fn _onprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        let input = ctx.input();
        let seen = Seen {
            space: (
                input.pressed(KeyCode::Space),
                input.just_pressed(KeyCode::Space),
                input.just_released(KeyCode::Space),
            ),
            fire: (
                input.action_pressed("fire"),
                input.action_just_pressed("fire"),
                input.action_just_released("fire"),
            ),
            movement: input.axis2("move"),
            steer: input.axis("left", "right"),
            motion: input.mouse_motion(),
        };
        ctx.world_mut().insert_resource(seen);
    }
}

fn binding(text: &str) -> Binding {
    text.parse().unwrap()
}

fn app() -> App {
    let mut map = InputMap::default();
    map.add_action("fire", vec![binding("Space"), binding("MouseLeft")])
        .add_action("left", vec![binding("KeyA")])
        .add_action("right", vec![binding("KeyD"), binding("GamepadDPadRight")])
        .add_axis2(
            "move",
            Axis2Binding {
                left: Some(binding("KeyA")),
                right: Some(binding("KeyD")),
                up: Some(binding("KeyW")),
                down: Some(binding("KeyS")),
                stick: Some(Stick::Left),
            },
        );
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, InputPlugin, ScriptPlugin))
        .insert_resource(map)
        .init_resource::<Seen>();
    app.world_mut().spawn(Reader);
    app
}

fn key(app: &mut App, key_code: KeyCode, state: ButtonState) {
    app.world_mut().write_message(KeyboardInput {
        key_code,
        logical_key: Key::Unidentified(NativeKey::Unidentified),
        state,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
}

fn mouse(app: &mut App, button: MouseButton, state: ButtonState) {
    app.world_mut().write_message(MouseButtonInput {
        button,
        state,
        window: Entity::PLACEHOLDER,
    });
}

fn seen(app: &App) -> &Seen {
    app.world().resource::<Seen>()
}

#[test]
fn a_key_is_just_pressed_for_one_frame_then_held_then_just_released() {
    let mut app = app();
    key(&mut app, KeyCode::Space, ButtonState::Pressed);
    app.update();
    assert_eq!(seen(&app).space, (true, true, false));
    app.update();
    assert_eq!(seen(&app).space, (true, false, false));
    key(&mut app, KeyCode::Space, ButtonState::Released);
    app.update();
    assert_eq!(seen(&app).space, (false, false, true));
}

#[test]
fn an_action_is_pressed_by_any_binding_and_just_pressed_once() {
    let mut app = app();
    mouse(&mut app, MouseButton::Left, ButtonState::Pressed);
    app.update();
    assert_eq!(seen(&app).fire, (true, true, false), "the mouse fires");

    key(&mut app, KeyCode::Space, ButtonState::Pressed);
    app.update();
    assert_eq!(
        seen(&app).fire,
        (true, false, false),
        "a second binding on a held action doesn't press it again"
    );

    mouse(&mut app, MouseButton::Left, ButtonState::Released);
    app.update();
    assert_eq!(
        seen(&app).fire,
        (true, false, false),
        "Space still holds it"
    );

    key(&mut app, KeyCode::Space, ButtonState::Released);
    app.update();
    assert_eq!(seen(&app).fire, (false, false, true));
}

#[test]
fn axis2_adds_keys_and_stick_and_moving_diagonally_isnt_faster() {
    let mut app = app();
    key(&mut app, KeyCode::KeyW, ButtonState::Pressed);
    app.update();
    assert_eq!(seen(&app).movement, Vec2::Y, "up is +y");

    key(&mut app, KeyCode::KeyD, ButtonState::Pressed);
    app.update();
    let diagonal = seen(&app).movement;
    assert!((diagonal.length() - 1.0).abs() < 1e-5, "{diagonal}");
    assert!(diagonal.x > 0.0 && diagonal.y > 0.0);

    key(&mut app, KeyCode::KeyW, ButtonState::Released);
    key(&mut app, KeyCode::KeyD, ButtonState::Released);
    let mut pad = Gamepad::default();
    pad.analog_mut().set(GamepadAxis::LeftStickX, -0.5);
    app.world_mut().spawn(pad);
    app.update();
    assert_eq!(
        seen(&app).movement,
        Vec2::new(-0.5, 0.0),
        "the stick, alone"
    );
}

#[test]
fn axis_is_positive_minus_negative_and_counts_gamepad_buttons() {
    let mut app = app();
    key(&mut app, KeyCode::KeyA, ButtonState::Pressed);
    app.update();
    assert_eq!(seen(&app).steer, -1.0);

    let mut pad = Gamepad::default();
    pad.digital_mut().press(GamepadButton::DPadRight);
    app.world_mut().spawn(pad);
    app.update();
    assert_eq!(seen(&app).steer, 0.0, "left and right cancel out");
}

#[test]
fn mouse_motion_is_this_frames_movement() {
    let mut app = app();
    app.world_mut().write_message(MouseMotion {
        delta: Vec2::new(3.0, -2.0),
    });
    app.update();
    assert_eq!(seen(&app).motion, Vec2::new(3.0, -2.0));
    app.update();
    assert_eq!(seen(&app).motion, Vec2::ZERO);
}

#[script]
struct Typo;

impl Script for Typo {
    fn _onprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        ctx.input().action_pressed("fyre");
    }
}

#[test]
fn a_misspelt_action_switches_the_script_off() {
    let mut app = app();
    let typo = app.world_mut().spawn(Typo).id();
    app.update();
    let disabled = app.world().get::<DisabledScripts>(typo).unwrap();
    assert_eq!(disabled.0, [<Typo as TypePath>::type_path()]);
}

#[test]
fn bindings_are_written_as_in_oddity_toml() {
    for text in ["Space", "KeyW", "ArrowUp", "MouseLeft", "GamepadSouth"] {
        assert_eq!(binding(text).to_string(), text);
    }
    assert_eq!(binding("KeyW"), Binding::Key(KeyCode::KeyW));
    assert_eq!(binding("MouseRight"), Binding::Mouse(MouseButton::Right));
    assert_eq!(
        binding("GamepadLeftTrigger2"),
        Binding::Gamepad(GamepadButton::LeftTrigger2)
    );
    for wrong in ["Spcae", "Mouse", "MouseOther", "GamepadNorthWest", ""] {
        assert!(wrong.parse::<Binding>().is_err(), "{wrong:?}");
    }
    assert_eq!("LeftStick".parse(), Ok(Stick::Left));
    assert!("LeftStik".parse::<Stick>().is_err());
}
