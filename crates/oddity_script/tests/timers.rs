//! A7: `after` calls a listener once, `every` keeps calling it, and
//! `cancel_timers` stops them. The spec's Bullet expires after 2 seconds.

use std::time::Duration;

use bevy::{prelude::*, time::TimeUpdateStrategy};
use oddity_script::{Ctx, Script, ScriptPlugin, ScriptTimers, script};

#[derive(Resource, Default)]
struct Log(Vec<String>);

/// Every frame lasts exactly one second, except the first, which has no
/// time to measure.
fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, ScriptPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(1)))
        .init_resource::<Log>();
    // Bevy's virtual clock caps a frame at 250 ms by default.
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .set_max_delta(Duration::from_secs(5));
    app
}

/// The spec's Bullet, without the moving.
#[script]
struct Bullet;

impl Script for Bullet {
    fn _onready(&mut self, ctx: &mut Ctx) {
        ctx.after(2.0, "expire");
    }
}

#[script]
impl Bullet {
    #[listener]
    fn expire(&mut self, ctx: &mut Ctx) {
        ctx.destroy();
    }
}

#[test]
fn after_calls_the_listener_once_when_the_time_is_up() {
    let mut app = app();
    let bullet = app.world_mut().spawn(Bullet).id();
    app.update(); // 0 s: ready, the timer starts
    app.update(); // 1 s
    assert!(app.world().get_entity(bullet).is_ok(), "one second left");
    app.update(); // 2 s: expire
    assert!(app.world().get_entity(bullet).is_err());
}

/// Blinks every second, and stops itself after three.
#[script]
struct Blinker {
    blinks: u32,
}

impl Script for Blinker {
    fn _onready(&mut self, ctx: &mut Ctx) {
        ctx.every(1.0, "blink");
    }
}

#[script]
impl Blinker {
    #[listener]
    fn blink(&mut self, ctx: &mut Ctx) {
        self.blinks += 1;
        let line = format!("blink {}", self.blinks);
        ctx.world_mut().resource_mut::<Log>().0.push(line);
        if self.blinks == 3 {
            ctx.cancel_timers();
        }
    }
}

#[test]
fn every_repeats_until_cancelled() {
    let mut app = app();
    let blinker = app.world_mut().spawn(Blinker::default()).id();
    for _ in 0..6 {
        app.update();
    }
    assert_eq!(
        app.world().resource::<Log>().0,
        ["blink 1", "blink 2", "blink 3"]
    );
    assert!(
        app.world()
            .get::<ScriptTimers>(blinker)
            .unwrap()
            .0
            .is_empty(),
        "cancelled"
    );
}
