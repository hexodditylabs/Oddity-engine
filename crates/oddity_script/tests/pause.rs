//! A9: while paused, a *Pausable* script stops and an *Always* one keeps
//! running; `delta` follows the time scale.

use std::time::Duration;

use bevy::{prelude::*, time::TimeUpdateStrategy};
use oddity_script::{Ctx, ProcessMode, Script, ScriptPlugin, script};

/// Each node's per-frame calls, and the `delta` of the last one.
#[derive(Resource, Default)]
struct Log(Vec<(String, f32)>);

impl Log {
    fn names(&self) -> Vec<&str> {
        self.0.iter().map(|(name, _)| name.as_str()).collect()
    }
}

#[script]
struct Ticker {
    pub label: String,
    ready: bool,
}

impl Script for Ticker {
    fn _onready(&mut self, _ctx: &mut Ctx) {
        self.ready = true;
    }

    fn _onprocess(&mut self, ctx: &mut Ctx, delta: f32) {
        let label = self.label.clone();
        ctx.world_mut().resource_mut::<Log>().0.push((label, delta));
    }
}

fn ticker(label: &str) -> Ticker {
    Ticker {
        label: label.into(),
        ..default()
    }
}

/// Every frame lasts 100 ms of real time.
fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, ScriptPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            100,
        )))
        .init_resource::<Log>();
    app
}

/// One frame's calls, by label.
fn frame(app: &mut App) -> Vec<String> {
    app.world_mut().resource_mut::<Log>().0.clear();
    app.update();
    app.world()
        .resource::<Log>()
        .names()
        .into_iter()
        .map(str::to_owned)
        .collect()
}

fn set_paused(app: &mut App, paused: bool) {
    let mut time = app.world_mut().resource_mut::<Time<Virtual>>();
    if paused { time.pause() } else { time.unpause() }
}

#[test]
fn while_paused_pausable_scripts_stop_and_the_others_follow_their_mode() {
    let mut app = app();
    let world = app.world_mut();
    world.spawn(ticker("pausable"));
    world.spawn((ticker("always"), ProcessMode::Always));
    world.spawn((ticker("menu"), ProcessMode::WhenPaused));
    let disabled = world
        .spawn((ticker("disabled"), ProcessMode::Disabled))
        .id();
    // Inherit, under an Always parent.
    world
        .spawn(ProcessMode::Always)
        .with_child(ticker("child of always"));

    assert_eq!(frame(&mut app), ["pausable", "always", "child of always"]);

    set_paused(&mut app, true);
    assert_eq!(frame(&mut app), ["always", "menu", "child of always"]);

    set_paused(&mut app, false);
    assert_eq!(frame(&mut app), ["pausable", "always", "child of always"]);

    assert!(
        app.world().get::<Ticker>(disabled).unwrap().ready,
        "a disabled node is still made ready, as in Godot"
    );
}

/// Pauses the game on its first frame.
#[script]
struct Pauser;

impl Script for Pauser {
    fn _onprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        if !ctx.is_paused() {
            ctx.pause(true);
            ctx.world_mut()
                .resource_mut::<Log>()
                .0
                .push(("pauser".into(), 0.0));
        }
    }
}

#[test]
fn pausing_stops_the_pausable_scripts_after_it_in_the_same_frame() {
    let mut app = app();
    let world = app.world_mut();
    world.spawn(ticker("before"));
    world.spawn((Pauser, ProcessMode::Always));
    world.spawn(ticker("after"));
    world.spawn((ticker("menu"), ProcessMode::WhenPaused));
    assert_eq!(frame(&mut app), ["before", "pauser", "menu"]);
    assert_eq!(frame(&mut app), ["menu"]);
}

fn last_delta(app: &App) -> f32 {
    app.world().resource::<Log>().0.last().unwrap().1
}

#[test]
fn delta_follows_the_time_scale_and_scripts_running_while_paused_get_real_time() {
    let mut app = app();
    app.world_mut()
        .spawn((ticker("always"), ProcessMode::Always));
    frame(&mut app); // the first frame has no time to measure
    frame(&mut app);
    assert!(
        (last_delta(&app) - 0.1).abs() < 1e-6,
        "{}",
        last_delta(&app)
    );

    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .set_relative_speed(0.5);
    frame(&mut app);
    assert!(
        (last_delta(&app) - 0.05).abs() < 1e-6,
        "{}",
        last_delta(&app)
    );

    set_paused(&mut app, true);
    frame(&mut app);
    assert!(
        (last_delta(&app) - 0.05).abs() < 1e-6,
        "game time stands still, but a pause menu still animates: {}",
        last_delta(&app)
    );
}

/// Sets the time scale from a script, and logs what it reads back.
#[script]
struct SlowMotion;

impl Script for SlowMotion {
    fn _onprocess(&mut self, ctx: &mut Ctx, delta: f32) {
        ctx.set_time_scale(0.25);
        let scale = ctx.time_scale();
        ctx.world_mut()
            .resource_mut::<Log>()
            .0
            .push((format!("scale {scale}"), delta));
    }
}

#[test]
fn a_script_sets_the_time_scale_for_the_next_frame() {
    let mut app = app();
    app.world_mut().spawn(SlowMotion);
    frame(&mut app);
    assert_eq!(frame(&mut app), ["scale 0.25"]);
    assert!(
        (last_delta(&app) - 0.025).abs() < 1e-6,
        "{}",
        last_delta(&app)
    );
}

/// Expires after a second.
#[script]
struct Bomb;

impl Script for Bomb {
    fn _onready(&mut self, ctx: &mut Ctx) {
        ctx.after(1.0, "boom");
    }
}

#[script]
impl Bomb {
    #[listener]
    fn boom(&mut self, ctx: &mut Ctx) {
        ctx.destroy();
    }
}

#[test]
fn a_pausable_nodes_timers_stand_still_while_paused() {
    let mut app = app();
    let bomb = app.world_mut().spawn(Bomb).id();
    frame(&mut app); // ready: the timer starts
    for _ in 0..5 {
        frame(&mut app); // 0.5 s
    }
    set_paused(&mut app, true);
    for _ in 0..20 {
        frame(&mut app);
    }
    assert!(
        app.world().get_entity(bomb).is_ok(),
        "paused: no time passes"
    );
    set_paused(&mut app, false);
    for _ in 0..6 {
        frame(&mut app);
    }
    assert!(app.world().get_entity(bomb).is_err());
}
