//! A6: a signal emitted by one script reaches every `#[on(..)]` handler,
//! after the emitter finishes and in Hierarchy order, and systems read it
//! as a message. Signals register themselves.

use bevy::prelude::*;
use oddity_script::{Ctx, DisabledScripts, Script, ScriptPlugin, ScriptSystems, script, signal};

#[derive(Resource, Default)]
struct Log(Vec<String>);

fn log(ctx: &mut Ctx, line: String) {
    ctx.world_mut().resource_mut::<Log>().0.push(line);
}

#[signal]
struct EnemyKilled {
    score: u32,
}

/// Emits one kill, on its first frame.
#[script]
struct Killer {
    done: bool,
}

impl Script for Killer {
    fn _onprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        if !self.done {
            self.done = true;
            ctx.emit(EnemyKilled { score: 10 });
            log(ctx, "killer done".into());
        }
    }
}

#[script]
struct Scoreboard {
    pub label: String,
    total: u32,
}

impl Script for Scoreboard {}

#[script]
impl Scoreboard {
    #[on(EnemyKilled)]
    fn add_score(&mut self, ctx: &mut Ctx, killed: &EnemyKilled) {
        self.total += killed.score;
        log(ctx, format!("{} got {}", self.label, killed.score));
    }
}

#[derive(Resource, Default)]
struct Kills(u32);

fn count_kills(mut kills: MessageReader<EnemyKilled>, mut count: ResMut<Kills>) {
    count.0 += kills.read().count() as u32;
}

fn scoreboard(label: &str) -> Scoreboard {
    Scoreboard {
        label: label.into(),
        ..default()
    }
}

/// An early scoreboard, the killer, then a late scoreboard. No
/// `add_message`: the signal registers itself.
fn app() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, ScriptPlugin))
        .init_resource::<Log>()
        .init_resource::<Kills>()
        .add_systems(Update, count_kills.after(ScriptSystems::Process));
    let world = app.world_mut();
    world.spawn(scoreboard("early"));
    world.spawn(Killer::default());
    let late = world.spawn(scoreboard("late")).id();
    (app, late)
}

#[test]
fn handlers_run_after_the_emitter_in_hierarchy_order_and_systems_read_it() {
    let (mut app, late) = app();
    app.update();
    app.update();
    assert_eq!(
        app.world().resource::<Log>().0,
        ["killer done", "early got 10", "late got 10"],
        "even the early scoreboard, which ran before the killer, waits for it"
    );
    assert_eq!(app.world().get::<Scoreboard>(late).unwrap().total, 10);
    assert_eq!(
        app.world().resource::<Kills>().0,
        1,
        "one message, read once"
    );
}

#[test]
fn a_switched_off_handler_misses_the_signal() {
    let (mut app, late) = app();
    app.world_mut()
        .entity_mut(late)
        .insert(DisabledScripts(vec![
            <Scoreboard as TypePath>::type_path().to_owned(),
        ]));
    app.update();
    assert_eq!(
        app.world().resource::<Log>().0,
        ["killer done", "early got 10"]
    );
}
