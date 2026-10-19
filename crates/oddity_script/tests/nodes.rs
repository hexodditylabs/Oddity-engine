//! A4's exit test: the spec's Enemy example reaches the player's `Health`
//! through `call`, and a script calling itself is queued until it finishes.
//! Also `position_of`, `global_transform_of`, `all` and groups.

use std::time::Duration;

use bevy::{prelude::*, time::TimeUpdateStrategy};
use oddity_script::{Ctx, NodeRef, Script, ScriptPlugin, script};

#[derive(Resource, Default)]
struct Log(Vec<String>);

fn log(ctx: &mut Ctx, line: &str) {
    ctx.world_mut()
        .resource_mut::<Log>()
        .0
        .push(line.to_owned());
}

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, ScriptPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(1)))
        .init_resource::<Log>();
    app
}

// The spec's Health and Enemy, as far as A4 goes: no events yet (A5).

#[script]
struct Health {
    #[default(100.0)]
    pub max: f32,
    current: f32,
}

impl Script for Health {
    fn _onready(&mut self, _ctx: &mut Ctx) {
        self.current = self.max;
    }
}

impl Health {
    fn damage(&mut self, amount: f32, _ctx: &mut Ctx) {
        self.current -= amount;
    }
}

#[script]
struct Player;

impl Script for Player {}

#[script]
struct Enemy {
    #[default(3.0)]
    pub speed: f32,
    #[default(20.0)]
    pub damage_per_second: f32,
    pub target: NodeRef,
}

impl Script for Enemy {
    fn _onready(&mut self, ctx: &mut Ctx) {
        if self.target.is_none() {
            self.target = ctx.all::<Player>().first().copied().unwrap_or_default();
        }
    }

    fn _onprocess(&mut self, ctx: &mut Ctx, delta: f32) {
        let Some(target) = ctx.position_of(self.target) else {
            return;
        };
        let to_target = target - ctx.position();
        if to_target.length() > 1.0 {
            ctx.translate(to_target.normalize() * self.speed * delta);
            ctx.look_at(target);
        } else {
            let damage = self.damage_per_second * delta;
            ctx.call::<Health>(self.target, move |health, ctx| health.damage(damage, ctx));
        }
    }
}

#[test]
fn the_enemy_example_damages_the_player_through_call() {
    let mut app = app();
    let world = app.world_mut();
    let player = world
        .spawn((Player, Health::default(), Transform::default()))
        .id();
    world.spawn((Enemy::default(), Transform::from_xyz(0.5, 0.0, 0.0)));
    for _ in 0..3 {
        app.update();
    }
    let elapsed = app.world().resource::<Time>().elapsed_secs();
    assert!(elapsed > 0.0);
    let health = app.world().get::<Health>(player).unwrap();
    assert_eq!(health.current, 100.0 - 20.0 * elapsed);
}

#[test]
fn a_far_enemy_walks_toward_the_player_first() {
    let mut app = app();
    let world = app.world_mut();
    world.spawn((Player, Health::default(), Transform::default()));
    let enemy = world
        .spawn((Enemy::default(), Transform::from_xyz(10.0, 0.0, 0.0)))
        .id();
    for _ in 0..3 {
        app.update();
    }
    let x = app.world().get::<Transform>(enemy).unwrap().translation.x;
    assert!(x < 10.0, "moved closer: x = {x}");
}

/// Calls itself, and another script on its node.
#[script]
struct Echo;

impl Script for Echo {
    fn _onprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        log(ctx, "start");
        let me = ctx.node();
        ctx.call::<Echo>(me, |_, ctx| log(ctx, "self-call"));
        ctx.call::<Listener>(me, |_, ctx| log(ctx, "other script, right away"));
        log(ctx, "end");
    }
}

#[script]
struct Listener;

impl Script for Listener {}

#[test]
fn a_self_call_is_queued_and_runs_after_the_caller() {
    let mut app = app();
    app.world_mut().spawn((Echo, Listener));
    app.update();
    assert_eq!(
        app.world().resource::<Log>().0,
        ["start", "other script, right away", "end", "self-call"]
    );
}

/// Records what it finds, for the tests below to check.
#[derive(Resource, Default)]
struct Found {
    position: Option<Vec3>,
    gone: Option<Vec3>,
    players: Vec<NodeRef>,
    enemies: Vec<NodeRef>,
}

#[script]
struct Finder {
    pub other: NodeRef,
    pub gone: NodeRef,
}

impl Script for Finder {
    fn _onprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        ctx.add_to_group("enemies");
        ctx.add_to_group("enemies");
        let found = Found {
            position: ctx.position_of(self.other),
            gone: ctx.global_transform_of(self.gone).map(|t| t.translation()),
            players: ctx.all::<Player>(),
            enemies: ctx.in_group("enemies"),
        };
        ctx.world_mut().insert_resource(found);
    }
}

#[test]
fn positions_of_other_nodes_all_and_groups() {
    let mut app = app();
    let world = app.world_mut();
    let root = world.spawn(Transform::default()).id();
    let first = world
        .spawn((Player, Transform::from_xyz(1.0, 2.0, 3.0), ChildOf(root)))
        .id();
    let second = world.spawn((Player, ChildOf(root))).id();
    let gone = world.spawn_empty().id();
    world.despawn(gone);
    let finder = world
        .spawn(Finder {
            other: NodeRef::from(first),
            gone: NodeRef::from(gone),
        })
        .id();
    // Transforms propagate at the end of a frame; look on the second.
    app.update();
    app.update();

    let found = app.world().resource::<Found>();
    assert_eq!(found.position, Some(Vec3::new(1.0, 2.0, 3.0)));
    assert_eq!(found.gone, None, "a node that's gone has no position");
    assert_eq!(found.players, [NodeRef::from(first), NodeRef::from(second)]);
    assert_eq!(found.enemies, [NodeRef::from(finder)], "added once");
}

#[script]
struct Leaver;

impl Script for Leaver {
    fn _onprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        ctx.add_to_group("enemies");
        ctx.remove_from_group("enemies");
        let still = ctx.in_group("enemies").len();
        log(ctx, &format!("{still} left"));
        // A call to nothing does nothing.
        ctx.call::<Health>(NodeRef::NONE, |_, _| unreachable!());
    }
}

#[test]
fn remove_from_group_and_calls_to_no_node() {
    let mut app = app();
    app.world_mut().spawn(Leaver);
    app.update();
    assert_eq!(app.world().resource::<Log>().0, ["0 left"]);
}
