//! A10 and B1: the spec's test example ("Testing your game code") passes
//! against a small level, from a level file and built in code, with the
//! shooter's Health and Enemy scripts.

use oddity::prelude::*;
use oddity::test::Game;

#[script(id = "3c1d9a07")]
struct Health {
    #[default(100.0)]
    pub max: f32,
    pub on_died: Event,
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
        if self.current <= 0.0 {
            self.on_died.invoke(ctx);
        }
    }
}

#[signal]
struct EnemyKilled {
    score: u32,
}

#[script(id = "9e4b2f15")]
struct Enemy {
    #[default(10)]
    pub score: u32,
}

impl Script for Enemy {
    fn _onready(&mut self, ctx: &mut Ctx) {
        let me = ctx.node();
        ctx.connect(me, "on_died", "die");
    }
}

#[script]
impl Enemy {
    #[listener]
    fn die(&mut self, ctx: &mut Ctx) {
        ctx.emit(EnemyKilled { score: self.score });
        ctx.destroy();
    }
}

#[derive(Resource, Default)]
struct Score(u32);

/// The game's own systems, as a game's `GamePlugin` holds them.
struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Score>()
            .add_systems(Update, count_score);
    }
}

fn count_score(mut kills: MessageReader<EnemyKilled>, mut score: ResMut<Score>) {
    for kill in kills.read() {
        score.0 += kill.score;
    }
}

fn arena() -> Game {
    Game::new().add_plugins(GamePlugin).with_level(|level| {
        level.spawn(Name::new("Player"));
        level.spawn(Name::new("Enemies")).with_children(|enemies| {
            enemies.spawn((Name::new("Goblin"), Health::default(), Enemy::default()));
            enemies.spawn((Name::new("Orc"), Health::default(), Enemy::default()));
        });
    })
}

#[test]
fn goblins_die_when_shot() {
    let mut game = Game::load("tests/levels/arena.level");
    let goblin = game.find("Enemies/Goblin");
    game.call::<Health>(goblin, |health, ctx| health.damage(1000.0, ctx));
    game.step(1);
    assert!(!game.exists(goblin));
}

#[test]
fn a_level_files_values_reach_the_scripts() {
    let mut game = Game::new()
        .add_plugins(GamePlugin)
        .with_level_file("tests/levels/arena.level");
    let orc = game.find("Enemies/Orc");
    game.call::<Health>(orc, |health, ctx| health.damage(200.0, ctx));
    game.step(1);
    assert!(game.exists(orc), "the file gave it 300 health");
    game.call::<Health>(orc, |health, ctx| health.damage(100.0, ctx));
    game.step(1);
    assert!(!game.exists(orc));
    assert_eq!(game.world().resource::<Score>().0, 30, "and a score of 30");
}

#[test]
fn goblins_die_when_shot_in_a_level_built_in_code() {
    let mut game = arena();
    let goblin = game.find("Enemies/Goblin");
    game.call::<Health>(goblin, |health, ctx| health.damage(1000.0, ctx));
    game.step(1);
    assert!(!game.exists(goblin));
}

#[test]
fn a_light_hit_leaves_it_standing_and_a_kill_scores() {
    let mut game = arena();
    let orc = game.find("Enemies/Orc");
    assert!(game.exists(orc));
    game.call::<Health>(orc, |health, ctx| health.damage(10.0, ctx));
    game.step(1);
    assert!(game.exists(orc), "health was ready: 90 left");
    assert_eq!(game.world().resource::<Score>().0, 0);

    game.call::<Health>(orc, |health, ctx| health.damage(90.0, ctx));
    game.step(1);
    assert!(!game.exists(orc));
    assert_eq!(game.world().resource::<Score>().0, 10);
}

#[test]
fn find_follows_names_from_the_level_root() {
    let game = arena();
    assert!(!game.find("Player").is_none());
    assert_eq!(game.find(""), game.level());
    assert!(game.find("Enemies/Troll").is_none());
    assert!(
        game.find("Goblin").is_none(),
        "paths start at the level root"
    );
}

#[test]
fn every_frame_lasts_the_fixed_step() {
    let mut game = arena();
    game.step(64);
    let elapsed = game.world().resource::<Time>().elapsed();
    assert_eq!(elapsed, Game::FRAME * 64);
}

#[script(id = "5b7e11e7")]
struct Bullet {
    pub speed: f32,
}

impl Script for Bullet {}

/// The spec's Player, firing once: `instantiate` takes the muzzle's
/// `GlobalTransform` as it is.
#[script]
struct Gun {
    pub bullet: Prefab,
    pub muzzle: NodeRef,
    fired: bool,
}

impl Script for Gun {
    fn _onprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        if !self.fired {
            self.fired = true;
            let at = ctx
                .global_transform_of(self.muzzle)
                .unwrap_or(ctx.global_transform());
            ctx.instantiate(&self.bullet, at);
        }
    }
}

#[test]
fn a_script_fires_a_bullet_prefab() {
    let mut game = Game::new().with_level(|level| {
        level
            .spawn((Name::new("Player"), Transform::from_xyz(3.0, 0.0, 0.0)))
            .with_child((Name::new("Muzzle"), Transform::from_xyz(0.0, 1.0, 0.0)));
    });
    let (player, muzzle) = (game.find("Player"), game.find("Player/Muzzle"));
    game.world_mut()
        .entity_mut(player.entity().unwrap())
        .insert(Gun {
            bullet: Prefab::at("tests/prefabs/bullet.prefab"),
            muzzle,
            fired: false,
        });
    game.step(1);
    let bullet = game.find("Bullet");
    assert_eq!(game.get::<Bullet>(bullet).unwrap().speed, 20.0);
    let at = game.get::<Transform>(bullet).unwrap().translation;
    assert_eq!(at, Vec3::new(3.0, 1.0, 0.0), "at the muzzle");
}
