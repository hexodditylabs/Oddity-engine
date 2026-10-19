//! A2's exit test: the runner's order, `_onready` timing, immediate
//! changes, destroying, and switching off a panicking script.
//!
//! Scripts log what happens to them in a [`Log`] resource through
//! `ctx.world_mut()`; `Ctx`'s own methods arrive with A3.

use bevy::prelude::*;
use oddity_script::{Ctx, DisabledScripts, Script, ScriptPlugin, script};

#[derive(Resource, Default)]
struct Log(Vec<String>);

fn log(ctx: &mut Ctx, line: String) {
    ctx.world_mut().resource_mut::<Log>().0.push(line);
}

/// Logs every callback, tagged with its label.
#[script]
struct Tracer {
    pub label: String,
}

impl Script for Tracer {
    fn _onenable(&mut self, ctx: &mut Ctx) {
        log(ctx, format!("{} enable", self.label));
    }
    fn _onready(&mut self, ctx: &mut Ctx) {
        log(ctx, format!("{} ready", self.label));
    }
    fn _onprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        log(ctx, format!("{} process", self.label));
    }
    fn _onphysicsprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        log(ctx, format!("{} physics", self.label));
    }
    fn _onlateprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        log(ctx, format!("{} late", self.label));
    }
    fn _ondisable(&mut self, ctx: &mut Ctx) {
        log(ctx, format!("{} disable", self.label));
    }
    fn _ondestroy(&mut self, ctx: &mut Ctx) {
        log(ctx, format!("{} destroy", self.label));
    }
}

/// Runs before order-0 scripts on the same node.
#[script(order = -5)]
struct Early {
    pub label: String,
}

impl Script for Early {
    fn _onprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        log(ctx, format!("{} process", self.label));
    }
}

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, ScriptPlugin))
        .init_resource::<Log>();
    app
}

fn tracer(label: &str) -> Tracer {
    Tracer {
        label: label.into(),
    }
}

/// The lines containing `word`, in order.
fn lines(app: &App, word: &str) -> Vec<String> {
    app.world()
        .resource::<Log>()
        .0
        .iter()
        .filter(|line| line.contains(word))
        .cloned()
        .collect()
}

#[test]
fn scripts_run_in_hierarchy_order_then_by_order() {
    let mut app = app();
    let world = app.world_mut();
    let a = world.spawn(tracer("A")).id();
    world.spawn((tracer("B"), ChildOf(a)));
    world.spawn((
        tracer("B2"),
        Early {
            label: "B2 early".into(),
        },
        ChildOf(a),
    ));
    world.spawn(tracer("C"));
    app.update();
    assert_eq!(
        lines(&app, "process"),
        [
            "A process",
            "B process",
            "B2 early process",
            "B2 process",
            "C process"
        ]
    );
}

#[test]
fn ready_runs_once_before_the_first_process_and_late_runs_after() {
    let mut app = app();
    app.world_mut().spawn(tracer("T"));
    app.update();
    app.update();
    assert_eq!(
        app.world().resource::<Log>().0,
        [
            "T enable",
            "T ready",
            "T process",
            "T late",
            "T process",
            "T late"
        ]
    );
}

#[test]
fn the_physics_stage_runs_its_callback() {
    let mut app = app();
    app.world_mut().spawn(tracer("P"));
    app.world_mut().run_schedule(FixedUpdate);
    assert_eq!(lines(&app, "physics"), ["P physics"]);
}

#[derive(Resource, Default)]
struct Counter(u32);

#[script(order = -1)]
struct Bumper;

impl Script for Bumper {
    fn _onprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        ctx.world_mut().resource_mut::<Counter>().0 += 1;
    }
}

#[script]
struct Reader;

impl Script for Reader {
    fn _onprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        let seen = ctx.world_mut().resource::<Counter>().0;
        log(ctx, format!("saw {seen}"));
    }
}

#[test]
fn a_change_is_visible_to_the_next_script_immediately() {
    let mut app = app();
    app.init_resource::<Counter>();
    app.world_mut().spawn((Bumper, Reader));
    app.update();
    app.update();
    assert_eq!(lines(&app, "saw"), ["saw 1", "saw 2"]);
}

/// Destroys its node on its second frame.
#[script]
struct SelfDestruct {
    frames: u32,
}

impl Script for SelfDestruct {
    fn _onprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        self.frames += 1;
        if self.frames == 2 {
            ctx.destroy();
        }
    }
}

#[test]
fn a_destroyed_nodes_scripts_get_their_last_callbacks_then_stop() {
    let mut app = app();
    let world = app.world_mut();
    let doomed = world.spawn(SelfDestruct::default()).id();
    world.spawn((tracer("child"), ChildOf(doomed)));
    for _ in 0..3 {
        app.update();
    }
    assert!(app.world().get_entity(doomed).is_err(), "despawned");
    assert_eq!(
        lines(&app, "child"),
        [
            "child enable",
            "child ready",
            "child process",
            "child late",
            "child disable",
            "child destroy",
        ],
        "destroyed with its parent on frame 2, before its own process"
    );
}

#[test]
fn a_script_that_never_started_gets_no_destroy_callbacks() {
    let mut app = app();
    let node = app.world_mut().spawn(tracer("unstarted")).id();
    app.world_mut()
        .run_system_cached_with(
            |In(node): In<Entity>, mut commands: Commands| {
                oddity_script::destroy(&mut commands, node);
            },
            node,
        )
        .unwrap();
    assert!(app.world().get_entity(node).is_err());
    assert!(lines(&app, "unstarted").is_empty());
}

#[test]
fn systems_destroy_with_callbacks_too() {
    let mut app = app();
    let node = app.world_mut().spawn(tracer("N")).id();
    app.update();
    app.world_mut()
        .run_system_cached_with(
            |In(node): In<Entity>, mut commands: Commands| {
                oddity_script::destroy(&mut commands, node);
            },
            node,
        )
        .unwrap();
    assert!(app.world().get_entity(node).is_err());
    assert_eq!(lines(&app, "N d"), ["N disable", "N destroy"]);
}

/// Panics on every frame it runs.
#[script]
struct Panicker {
    pub attempts: u32,
}

impl Script for Panicker {
    fn _onprocess(&mut self, _ctx: &mut Ctx, _delta: f32) {
        self.attempts += 1;
        panic!("out of ammo");
    }
}

#[test]
fn a_panicking_script_is_switched_off_and_the_rest_keep_running() {
    let mut app = app();
    let world = app.world_mut();
    let broken = world.spawn(Panicker { attempts: 0 }).id();
    world.spawn(tracer("survivor"));
    app.update();
    app.update();

    assert_eq!(lines(&app, "survivor process").len(), 2);
    let world = app.world();
    assert_eq!(
        world.get::<Panicker>(broken).unwrap().attempts,
        1,
        "put back after the panic, and not called again"
    );
    let disabled = world.get::<DisabledScripts>(broken).unwrap();
    assert!(disabled.contains(<Panicker as TypePath>::type_path()));
}

/// Switches the node's Tracer off on its second frame and on again on its third.
#[script(order = -10)]
struct Switcher {
    frames: u32,
}

impl Script for Switcher {
    fn _onprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        self.frames += 1;
        match self.frames {
            2 => ctx.enable::<Tracer>(false),
            3 => ctx.enable::<Tracer>(true),
            _ => {}
        }
    }
}

#[test]
fn enable_switches_a_script_off_and_on_with_its_callbacks() {
    let mut app = app();
    app.world_mut().spawn((Switcher::default(), tracer("T")));
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(
        app.world().resource::<Log>().0,
        [
            "T enable",
            "T ready",
            "T process",
            "T late",    // frame 1
            "T disable", // frame 2: off
            "T enable",
            "T process",
            "T late", // frame 3: on again
        ]
    );
}
