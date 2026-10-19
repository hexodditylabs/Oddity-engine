//! D1: the editor watches the code, builds with cargo's JSON messages,
//! points at each error's file and line, and has the game describe itself
//! after a good build.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use oddity_build::{
    Build, BuildEvent, BuildRequest, CargoMessage, Level, SETTLE, Watcher, parse_line, strip_ansi,
};

#[test]
fn cargo_messages_are_read() {
    let error = r#"{"reason":"compiler-message","package_id":"x","manifest_path":"/game/.oddity/runner/Cargo.toml","target":{},"message":{"rendered":"\u001b[0m\u001b[1m\u001b[38;5;9merror[E0425]\u001b[0m: cannot find value `x`\n","message":"cannot find value `x` in this scope","code":{"code":"E0425","explanation":"..."},"level":"error","spans":[{"file_name":"/game/src/player.rs","line_start":12,"column_start":9,"is_primary":true},{"file_name":"/game/src/lib.rs","line_start":1,"column_start":1,"is_primary":false}],"children":[]}}"#;
    let Some(CargoMessage::Diagnostic(diagnostic)) = parse_line(error) else {
        panic!("not read as a diagnostic");
    };
    assert_eq!(diagnostic.level, Level::Error);
    assert_eq!(diagnostic.code.as_deref(), Some("E0425"));
    assert_eq!(diagnostic.message, "cannot find value `x` in this scope");
    let location = diagnostic.location.unwrap();
    assert_eq!(location.file, Path::new("/game/src/player.rs"));
    assert_eq!((location.line, location.column), (12, 9));
    assert!(diagnostic.rendered.starts_with("error[E0425]: cannot find"));

    let relative = r#"{"reason":"compiler-message","manifest_path":"/game/Cargo.toml","message":{"rendered":"","message":"unused","code":null,"level":"warning","spans":[{"file_name":"src/lib.rs","line_start":3,"column_start":5,"is_primary":true}]}}"#;
    let Some(CargoMessage::Diagnostic(warning)) = parse_line(relative) else {
        panic!("not read as a diagnostic");
    };
    assert_eq!(warning.level, Level::Warning);
    assert_eq!(
        warning.location.unwrap().file,
        Path::new("/game/src/lib.rs"),
        "relative to the crate"
    );

    assert_eq!(
        parse_line(r#"{"reason":"compiler-artifact","executable":"/t/debug/game_runner"}"#),
        Some(CargoMessage::Executable("/t/debug/game_runner".into()))
    );
    assert_eq!(
        parse_line(r#"{"reason":"build-finished","success":false}"#),
        Some(CargoMessage::Finished { success: false })
    );
    assert_eq!(parse_line("   Compiling game v0.1.0"), None);
    assert_eq!(
        strip_ansi("\u{1b}[1m\u{1b}[32mCompiling\u{1b}[0m x"),
        "Compiling x"
    );
}

#[test]
fn a_build_starts_once_the_code_stops_changing() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src/enemies")).unwrap();
    fs::write(dir.path().join("src/lib.rs"), "").unwrap();
    let start = Instant::now();
    let mut watcher = Watcher::new(dir.path(), start);
    assert!(
        !watcher.poll(start + Duration::from_secs(5)),
        "nothing changed"
    );

    let t = start + Duration::from_secs(6);
    fs::write(dir.path().join("src/enemies/goblin.rs"), "// new").unwrap();
    assert!(!watcher.poll(t), "just changed: wait");
    fs::write(dir.path().join("src/lib.rs"), "mod enemies;").unwrap();
    assert!(
        !watcher.poll(t + Duration::from_millis(300)),
        "still changing"
    );
    assert!(!watcher.poll(t + Duration::from_millis(300) + SETTLE / 2));
    assert!(
        watcher.poll(t + Duration::from_millis(300) + SETTLE),
        "settled for half a second"
    );
    assert!(
        !watcher.poll(t + Duration::from_secs(10)),
        "once per change"
    );
}

/// A program with no dependencies standing in for a game's runner: it
/// prints a description on `--oddity-describe`.
fn runner(dir: &Path, main: &str) -> PathBuf {
    fs::create_dir_all(dir.join("runner/src")).unwrap();
    fs::write(
        dir.join("runner/Cargo.toml"),
        "[package]\nname = \"tiny_runner\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .unwrap();
    fs::write(dir.join("runner/src/main.rs"), main).unwrap();
    dir.join("runner/Cargo.toml")
}

/// Every event of a build, until it has nothing more to say.
fn finish(build: &Build) -> Vec<BuildEvent> {
    let mut events = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(300);
    while Instant::now() < deadline {
        let new = build.events();
        let done = new.iter().any(|event| {
            matches!(
                event,
                BuildEvent::Described(_)
                    | BuildEvent::Failed(_)
                    | BuildEvent::Finished { success: false }
            )
        });
        events.extend(new);
        if done {
            return events;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("the build took too long: {events:?}");
}

fn request(dir: &Path, manifest: PathBuf) -> BuildRequest {
    BuildRequest {
        dir: dir.to_path_buf(),
        manifest,
        cargo: std::env::var_os("CARGO").map_or("cargo".into(), PathBuf::from),
        types: dir.join(".oddity/types.json"),
    }
}

#[test]
fn an_error_points_at_its_file_and_line() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = runner(
        dir.path(),
        "fn main() {\n    let speed: f32 = \"fast\";\n}\n",
    );
    let events = finish(&Build::start(request(dir.path(), manifest)));

    let errors: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            BuildEvent::Diagnostic(d) if d.level == Level::Error => Some(d),
            _ => None,
        })
        .collect();
    assert!(!errors.is_empty(), "{events:?}");
    let location = errors[0].location.as_ref().unwrap();
    assert_eq!(location.file, dir.path().join("runner/src/main.rs"));
    assert_eq!(location.line, 2);
    assert!(events.contains(&BuildEvent::Finished { success: false }));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, BuildEvent::Progress(line) if line.contains("Compiling"))),
        "cargo's progress is shown too"
    );
    assert!(!dir.path().join(".oddity/types.json").exists());
}

#[test]
fn a_good_build_describes_the_game() {
    let dir = tempfile::tempdir().unwrap();
    let main = r#"fn main() {
    if std::env::args().any(|arg| arg == "--oddity-describe") {
        println!("{{\"scripts\": [], \"signals\": []}}");
    }
}
"#;
    let manifest = runner(dir.path(), main);
    let events = finish(&Build::start(request(dir.path(), manifest)));
    assert!(
        events.contains(&BuildEvent::Finished { success: true }),
        "{events:?}"
    );
    let types = dir.path().join(".oddity/types.json");
    assert!(events.contains(&BuildEvent::Described(types.clone())));
    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(types).unwrap()).unwrap();
    assert_eq!(written["scripts"], serde_json::json!([]));
}

#[test]
fn a_missing_cargo_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let mut request = request(dir.path(), dir.path().join("Cargo.toml"));
    request.cargo = dir.path().join("no-such-cargo");
    let events = finish(&Build::start(request));
    assert!(matches!(&events[..], [BuildEvent::Failed(why)] if why.contains("can't run")));
}
