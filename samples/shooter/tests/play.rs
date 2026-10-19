//! The shooter's main level, played headless: the goblins come, a shot
//! goblin scores, and Space fires.

use oddity::bevy::input::{
    ButtonState,
    keyboard::{Key, KeyboardInput, NativeKey},
};
use oddity::prelude::*;
use oddity::test::Game;
use shooter::{GamePlugin, Health, Score};

fn game() -> Game {
    Game::new()
        .add_plugins(GamePlugin)
        .with_level_file("levels/main.level")
}

#[test]
fn the_goblins_come_for_the_player() {
    let mut game = game();
    let (goblin, player) = (game.find("Enemies/Goblin"), game.find("Player"));
    let distance = |game: &Game| {
        let at = |node| game.get::<Transform>(node).unwrap().translation;
        at(goblin).with_y(0.0).distance(at(player).with_y(0.0))
    };
    let before = distance(&game);
    game.step(64);
    assert!(
        distance(&game) < before - 2.0,
        "{before} → {}",
        distance(&game)
    );
}

#[test]
fn a_dead_goblin_scores() {
    let mut game = game();
    let goblin = game.find("Enemies/Goblin2");
    game.call::<Health>(goblin, |health, ctx| health.damage(1000.0, ctx));
    game.step(1);
    assert!(!game.exists(goblin));
    assert_eq!(game.world().resource::<Score>().0, 10);
}

#[test]
fn space_fires_a_bullet() {
    let mut game = game();
    game.world_mut().write_message(KeyboardInput {
        key_code: KeyCode::Space,
        logical_key: Key::Unidentified(NativeKey::Unidentified),
        state: ButtonState::Pressed,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
    game.step(1);
    assert!(!game.find("Bullet").is_none());
}
