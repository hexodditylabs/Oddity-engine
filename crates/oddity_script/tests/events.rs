//! A5's exit test: Health's `on_died` reaches a listener on another node
//! right away, and one on its own node, whose script is running, queued.

use bevy::prelude::*;
use oddity_script::{Ctx, Event, Listener, NodeRef, ReflectScript, Script, ScriptPlugin, script};

#[derive(Resource, Default)]
struct Log(Vec<String>);

fn log(ctx: &mut Ctx, line: String) {
    ctx.world_mut().resource_mut::<Log>().0.push(line);
}

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, ScriptPlugin))
        .init_resource::<Log>();
    app
}

/// The spec's Health.
#[script]
struct Health {
    #[default(100.0)]
    pub max: f32,
    /// Fired once when health reaches zero.
    pub on_died: Event,
    /// Fired on every hit, with the damage taken.
    pub on_damaged: Event<f32>,
    current: f32,
}

impl Script for Health {
    fn _onready(&mut self, _ctx: &mut Ctx) {
        self.current = self.max;
    }
}

impl Health {
    fn damage(&mut self, amount: f32, ctx: &mut Ctx) {
        if self.current <= 0.0 {
            return;
        }
        self.current -= amount;
        self.on_damaged.invoke(ctx, amount);
        if self.current <= 0.0 {
            self.on_died.invoke(ctx);
        }
    }
}

/// Listens from another node, wired from code as the Inspector would.
#[script]
struct GameOver {
    pub player: NodeRef,
    pub stay_away: bool,
}

impl Script for GameOver {
    fn _onready(&mut self, ctx: &mut Ctx) {
        ctx.connect(self.player, "on_died", "show");
        ctx.connect(self.player, "on_damaged", "flash");
        if self.stay_away {
            ctx.disconnect(self.player, "on_died", "show");
        }
    }
}

#[script]
impl GameOver {
    #[listener]
    fn show(&mut self, ctx: &mut Ctx) {
        log(ctx, "game over".into());
    }

    #[listener]
    fn flash(&mut self, ctx: &mut Ctx, damage: &f32) {
        log(ctx, format!("flash {damage}"));
    }
}

/// Lives on the player's node, listens to its own Health, and hits it.
#[script]
struct Brawler;

impl Script for Brawler {
    fn _onready(&mut self, ctx: &mut Ctx) {
        let me = ctx.node();
        ctx.connect(me, "on_died", "die");
    }

    fn _onprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        let me = ctx.node();
        ctx.call::<Health>(me, |health, ctx| health.damage(100.0, ctx));
        log(ctx, "brawler done".into());
    }
}

#[script]
impl Brawler {
    #[listener]
    fn die(&mut self, ctx: &mut Ctx) {
        log(ctx, "brawler died".into());
    }
}

/// The game-over screen first in the Hierarchy, so it connects before the
/// player is hit.
fn level(app: &mut App, stay_away: bool) -> (Entity, Entity) {
    let world = app.world_mut();
    let screen = world.spawn_empty().id();
    let player = world.spawn((Health::default(), Brawler)).id();
    world.entity_mut(screen).insert(GameOver {
        player: NodeRef::from(player),
        stay_away,
    });
    (screen, player)
}

#[test]
fn on_died_reaches_another_node_now_and_its_own_node_queued() {
    let mut app = app();
    level(&mut app, false);
    app.update();
    assert_eq!(
        app.world().resource::<Log>().0,
        [
            "flash 100",    // on_damaged, with its value, on another node
            "game over",    // on_died, on another node: right away
            "brawler done", // the caller finishes...
            "brawler died", // ...then its own listener, which was queued
        ]
    );
}

#[test]
fn connections_are_saved_as_data_and_disconnect_undoes_them() {
    let mut app = app();
    let (screen, player) = level(&mut app, true);
    app.update();
    assert_eq!(
        app.world().resource::<Log>().0,
        ["flash 100", "brawler done", "brawler died"],
        "show was disconnected"
    );
    let health = app.world().get::<Health>(player).unwrap();
    assert_eq!(
        health.on_damaged.listeners(),
        [Listener {
            node: NodeRef::from(screen),
            name: "flash".into(),
        }]
    );
    assert_eq!(
        health.on_died.listeners().len(),
        1,
        "only the brawler's own"
    );
}

#[test]
fn listeners_are_listed_by_name() {
    let registry = AppTypeRegistry::new_with_derived_types();
    let registry = registry.read();
    let names = registry
        .get_type_data::<ReflectScript>(std::any::TypeId::of::<GameOver>())
        .unwrap()
        .listener_names();
    assert_eq!(names, ["show", "flash"]);
    let none = registry
        .get_type_data::<ReflectScript>(std::any::TypeId::of::<Health>())
        .unwrap()
        .listener_names();
    assert!(none.is_empty());
}
