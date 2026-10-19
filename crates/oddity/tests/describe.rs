//! C1: `--oddity-describe` prints the spec's JSON for the game's scripts,
//! and the game program reads its flags.

use oddity::prelude::*;
use oddity::{Options, describe};
use serde_json::json;

#[derive(Reflect, Default, Debug, Clone, PartialEq)]
enum Mode {
    #[default]
    Walk,
    Fly {
        height: f32,
    },
}

#[derive(Reflect, Default, Debug, Clone, PartialEq)]
struct Loadout {
    shots: u32,
    mode: Mode,
}

#[script(id = "4f9d2c71", order = -10)]
struct Player {
    #[default(6.0)]
    pub speed: f32,
    pub bullet: Prefab,
    pub muzzle: NodeRef,
    pub loadout: Loadout,
    pub on_died: Event,
    pub on_damaged: Event<f32>,
    cooldown: f32,
}

impl Script for Player {}

#[script]
impl Player {
    #[listener]
    fn die(&mut self, _ctx: &mut Ctx) {}

    #[listener]
    fn hit(&mut self, _ctx: &mut Ctx, _amount: &f32) {}
}

#[signal]
struct EnemyKilled {
    score: u32,
}

fn description() -> serde_json::Value {
    let app = App::new();
    let registry = app.world().resource::<AppTypeRegistry>().read();
    describe(&registry)
}

#[test]
fn a_script_is_described_with_its_fields_listeners_and_events() {
    let description = description();
    let player = description["scripts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|script| script["name"] == "Player")
        .unwrap();
    assert_eq!(player["type_path"], <Player as TypePath>::type_path());
    assert_eq!(player["id"], "4f9d2c71");
    assert_eq!(player["order"], -10);
    let fields: Vec<(&str, &str)> = player["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["name"].as_str().unwrap(), f["type"].as_str().unwrap()))
        .collect();
    let loadout = <Loadout as TypePath>::type_path();
    assert_eq!(
        fields,
        [
            ("speed", "f32"),
            ("bullet", "Prefab"),
            ("muzzle", "NodeRef"),
            ("loadout", loadout),
            ("on_died", "Event"),
            ("on_damaged", "Event<f32>"),
        ],
        "saved fields only, built-in types by their short names"
    );
    assert_eq!(player["fields"][0]["default"], 6.0);
    assert_eq!(player["fields"][2]["default"], json!(null));
    assert_eq!(
        player["listeners"],
        json!([{ "name": "die", "arg": null }, { "name": "hit", "arg": "f32" }])
    );
    assert_eq!(
        player["events"],
        json!([{ "field": "on_died", "arg": null }, { "field": "on_damaged", "arg": "f32" }])
    );
}

#[test]
fn the_types_fields_use_and_the_signals_are_described() {
    let description = description();
    let types = &description["types"];
    let loadout = <Loadout as TypePath>::type_path();
    let mode = <Mode as TypePath>::type_path();
    assert_eq!(
        types[loadout],
        json!({ "kind": "struct", "fields": [
            { "name": "shots", "type": "u32" },
            { "name": "mode", "type": mode }
        ] })
    );
    assert_eq!(
        types[mode],
        json!({ "kind": "enum", "variants": [
            { "name": "Walk", "kind": "unit", "fields": [] },
            { "name": "Fly", "kind": "struct", "fields": [{ "name": "height", "type": "f32" }] }
        ] })
    );
    assert!(
        description["signals"]
            .as_array()
            .unwrap()
            .contains(&json!({ "type_path": <EnemyKilled as TypePath>::type_path() }))
    );
}

#[test]
fn the_game_program_reads_its_flags_and_leaves_the_rest() {
    let parse = |args: &[&str]| Options::parse(args.iter().map(|arg| (*arg).to_owned()));
    assert_eq!(parse(&[]), Ok(Options::default()));
    assert_eq!(
        parse(&["--fullscreen", "--oddity-play", "levels/cave.level"]),
        Ok(Options {
            play: Some("levels/cave.level".into()),
            describe: false
        })
    );
    assert!(parse(&["--oddity-describe"]).unwrap().describe);
    assert!(parse(&["--oddity-play"]).is_err());
    assert!(
        parse(&["--oddity-ply", "x"])
            .unwrap_err()
            .contains("unknown flag")
    );
}
