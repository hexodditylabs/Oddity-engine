//! Building the game from the editor (`roadmap/scripting.md`, task D1).
//!
//! When the open project's code changes, the editor builds its runner in the
//! background; the hammer shows how that is going, and Output lists cargo's
//! progress and the compiler's messages. Clicking a message opens its file
//! at its line in VS Code or Zed. After a good build the game describes its
//! scripts into `.oddity/types.json`, which becomes the [`ScriptTypes`] the
//! Inspector and the `script.*` commands use (also read when a game opens).
//!
//! Commands: `build.run` (build now), `build.errors` (the last build's
//! messages), `code.open` (a file at a line, in the code editor).

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

use bevy::prelude::*;
use oddity_build::{
    Build, BuildEvent, BuildRequest, Diagnostic, Level, ProjectBuildPaths, Watcher,
};
use oddity_core::{ArgKind, ArgSpec, Args, CommandError, CommandSpec, scripts::ScriptTypes};
use oddity_plugin_api::EditorAppExt;
use oddity_project::{OdditySource, Project};
use serde_json::{Value, json};

use crate::{
    actions::{Run, UiAction},
    project::CurrentProject,
};

/// Output keeps this many lines; older ones scroll away for good.
const LOG_LINES: usize = 2000;

/// Builds the game when its code changes, and on `build.run`.
pub struct BuildPlugin;

impl Plugin for BuildPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Builds>()
            .register_command::<Run<RunBuild>>()
            .register_command::<Run<OpenCode>>()
            .register_query(BUILD_ERRORS, build_errors)
            .add_systems(Update, (watch_code, follow_build).chain());
    }
}

/// How the last build went: what the hammer shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BuildStatus {
    /// Not built since the editor opened.
    #[default]
    NotBuilt,
    /// Building, or describing what was built.
    Building,
    /// Built and described.
    Built,
    /// The code doesn't compile, or the game couldn't describe itself.
    Failed,
}

impl BuildStatus {
    fn name(self) -> &'static str {
        match self {
            Self::NotBuilt => "not_built",
            Self::Building => "building",
            Self::Built => "built",
            Self::Failed => "failed",
        }
    }
}

/// One line of Output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    /// What it says.
    pub text: String,
    /// For a compiler message, how serious it is.
    pub level: Option<Level>,
    /// For a compiler message, where it points: clicking the line opens it.
    pub location: Option<(PathBuf, u32, u32)>,
    /// Progress, shown dim.
    pub quiet: bool,
}

/// The editor's builds: the status, what Output shows, and the build in
/// progress.
#[derive(Resource, Default)]
pub struct Builds {
    /// How the last build went.
    pub status: BuildStatus,
    /// What Output shows.
    pub log: Vec<LogLine>,
    /// The last build's compiler messages.
    pub diagnostics: Vec<Diagnostic>,
    watcher: Option<Watcher>,
    current: Option<Build>,
}

impl Builds {
    fn say(&mut self, line: LogLine) {
        self.log.push(line);
        let extra = self.log.len().saturating_sub(LOG_LINES);
        self.log.drain(..extra);
    }

    fn note(&mut self, text: impl Into<String>) {
        self.say(LogLine {
            text: text.into(),
            level: None,
            location: None,
            quiet: false,
        });
    }
}

/// The open project, if it is a game the editor can build.
fn game(world: &World) -> Option<&Project> {
    world
        .resource::<CurrentProject>()
        .0
        .as_ref()
        .filter(|project| project.root.join("Cargo.toml").is_file())
}

/// Where new runners get `oddity`: this checkout of the engine when the
/// editor runs from source, else its release.
fn oddity_source() -> OdditySource {
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/oddity");
    match std::path::absolute(&checkout) {
        Ok(path) if path.join("Cargo.toml").is_file() => OdditySource::Path(path),
        _ => OdditySource::release(),
    }
}

/// Start a build of the open game, stopping the one in progress.
fn start_build(world: &mut World) -> Result<(), CommandError> {
    let Some(project) = game(world).cloned() else {
        return Err(CommandError::Invalid(
            "no game is open: open a project that has a Cargo.toml".into(),
        ));
    };
    let mut builds = world.resource_mut::<Builds>();
    if let Some(build) = builds.current.take() {
        build.cancel();
    }
    builds.log.clear();
    builds.diagnostics.clear();
    if let Err(error) = project.write_runner(&oddity_source()) {
        builds.status = BuildStatus::Failed;
        builds.note(format!("Can't write the runner: {error}"));
        return Err(CommandError::Invalid(error.to_string()));
    }
    builds.status = BuildStatus::Building;
    builds.note(format!("Building {}…", project.name()));
    builds.current = Some(Build::start(BuildRequest {
        dir: project.root.clone(),
        manifest: project.runner_manifest(),
        cargo: std::env::var_os("CARGO").map_or_else(|| "cargo".into(), PathBuf::from),
        types: project.types_file(),
    }));
    Ok(())
}

/// Build when the code changes, and once on opening a game that has never
/// described itself.
fn watch_code(world: &mut World) {
    let Some(project) = game(world).cloned() else {
        return;
    };
    let now = Instant::now();
    let first = world.resource::<Builds>().watcher.is_none();
    if first {
        read_types(world, &project.types_file());
    }
    let changed = {
        // Looking for changes isn't one: Output redraws only when the log
        // does.
        let mut builds = world.resource_mut::<Builds>();
        builds
            .bypass_change_detection()
            .watcher
            .get_or_insert_with(|| Watcher::new(&project.root, now))
            .poll(now)
    };
    if (changed || (first && !project.types_file().is_file()))
        && let Err(error) = start_build(world)
    {
        warn!("build: {error}");
    }
}

/// Read the game's type description into [`ScriptTypes`]. A missing or
/// broken file leaves what was there.
fn read_types(world: &mut World, path: &Path) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    match serde_json::from_str::<ScriptTypes>(&text) {
        Ok(types) => world.insert_resource(types),
        Err(error) => warn!("{}: {error}", path.display()),
    }
}

/// Show what the build in progress reports.
fn follow_build(world: &mut World) {
    let described = world.resource_scope(|world, mut builds: Mut<Builds>| {
        let project = world.resource::<CurrentProject>();
        follow(&mut builds, project)
    });
    if let Some(types) = described {
        read_types(world, &types);
    }
}

/// Take in the build's news; the type description's file once there is a
/// new one.
fn follow(builds: &mut Mut<Builds>, project: &CurrentProject) -> Option<PathBuf> {
    let events = builds.current.as_ref().map(Build::events)?;
    if events.is_empty() {
        return None;
    }
    let root = project.0.as_ref().map(|project| project.root.clone());
    let mut described = None;
    for event in events {
        match event {
            BuildEvent::Progress(line) => builds.say(LogLine {
                text: line,
                level: None,
                location: None,
                quiet: true,
            }),
            BuildEvent::Diagnostic(diagnostic) => {
                let line = diagnostic_line(&diagnostic, root.as_deref());
                builds.diagnostics.push(diagnostic);
                builds.say(line);
            }
            BuildEvent::Finished { success: true } => builds.note("Built. Describing the scripts…"),
            BuildEvent::Finished { success: false } => {
                let errors = builds
                    .diagnostics
                    .iter()
                    .filter(|d| d.level == Level::Error)
                    .count();
                builds.status = BuildStatus::Failed;
                builds.current = None;
                builds.note(format!(
                    "Build failed: {errors} error{}",
                    if errors == 1 { "" } else { "s" }
                ));
            }
            BuildEvent::Described(types) => {
                builds.status = BuildStatus::Built;
                builds.current = None;
                builds.note("Ready: the Inspector knows the scripts.");
                described = Some(types);
            }
            BuildEvent::Failed(why) => {
                builds.status = BuildStatus::Failed;
                builds.current = None;
                builds.note(why);
            }
        }
    }
    described
}

/// A compiler message as Output shows it: `error[E0425]: message ·
/// src/player.rs:12:9`, the path from the project folder.
fn diagnostic_line(diagnostic: &Diagnostic, root: Option<&Path>) -> LogLine {
    let level = match diagnostic.level {
        Level::Error => "error",
        Level::Warning => "warning",
        Level::Note => "note",
    };
    let mut text = match &diagnostic.code {
        Some(code) => format!("{level}[{code}]: {}", diagnostic.message),
        None => format!("{level}: {}", diagnostic.message),
    };
    let location = diagnostic.location.as_ref().map(|at| {
        let shown = root
            .and_then(|root| at.file.strip_prefix(root).ok())
            .unwrap_or(&at.file);
        text.push_str(&format!(
            "  ·  {}:{}:{}",
            shown.display().to_string().replace('\\', "/"),
            at.line,
            at.column
        ));
        (at.file.clone(), at.line, at.column)
    });
    LogLine {
        text,
        level: Some(diagnostic.level),
        location,
        quiet: false,
    }
}

/// `build.run`'s id, which the hammer runs.
pub const BUILD_RUN: &str = "build.run";

/// `build.run`.
struct RunBuild;

impl UiAction for RunBuild {
    const SPEC: CommandSpec = CommandSpec::action(
        BUILD_RUN,
        "Build the game's code now, then have the game describe its scripts for the Inspector.",
        &[],
    );

    fn run(world: &mut World, _args: &Args) -> Result<(), CommandError> {
        start_build(world)
    }
}

const BUILD_ERRORS: CommandSpec = CommandSpec::query(
    "build.errors",
    "The last build's status and its compiler errors and warnings, with file, line and column.",
    &[],
);

fn build_errors(world: &World, _args: &Args) -> Result<Value, CommandError> {
    let builds = world.resource::<Builds>();
    let messages: Vec<Value> = builds
        .diagnostics
        .iter()
        .filter(|d| d.level != Level::Note)
        .map(|d| {
            json!({
                "level": if d.level == Level::Error { "error" } else { "warning" },
                "message": d.message,
                "code": d.code,
                "file": d.location.as_ref().map(|at| at.file.display().to_string()),
                "line": d.location.as_ref().map(|at| at.line),
                "column": d.location.as_ref().map(|at| at.column),
                "rendered": d.rendered,
            })
        })
        .collect();
    Ok(json!({ "status": builds.status.name(), "messages": messages }))
}

/// `code.open`.
struct OpenCode;

const PATH: ArgSpec = ArgSpec {
    name: "path",
    kind: ArgKind::String,
    required: true,
    description: "The file to open",
};
const LINE: ArgSpec = ArgSpec {
    name: "line",
    kind: ArgKind::Json,
    required: false,
    description: "The line to go to, from 1",
};
const COLUMN: ArgSpec = ArgSpec {
    name: "column",
    kind: ArgKind::Json,
    required: false,
    description: "The column to go to, from 1",
};

impl UiAction for OpenCode {
    const SPEC: CommandSpec = CommandSpec::action(
        "code.open",
        "Open a file at a line in the code editor (VS Code, or else Zed).",
        &[PATH, LINE, COLUMN],
    );

    fn run(_world: &mut World, args: &Args) -> Result<(), CommandError> {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| CommandError::Invalid("code.open needs a path".into()))?;
        let number = |name: &str| args.get(name).and_then(Value::as_u64).unwrap_or(1);
        let at = format!("{path}:{}:{}", number("line"), number("column"));
        let (program, mut arguments) = code_editor().ok_or_else(|| {
            CommandError::Invalid("neither VS Code (`code`) nor Zed (`zed`) is on the PATH".into())
        })?;
        arguments.push(at);
        Command::new(&program)
            .args(&arguments)
            .spawn()
            .map(drop)
            .map_err(|error| CommandError::Invalid(format!("{}: {error}", program.display())))
    }
}

/// VS Code (`code -g file:line:column`), or else Zed (`zed file:line:column`).
fn code_editor() -> Option<(PathBuf, Vec<String>)> {
    if let Some(code) = on_path("code") {
        return Some((code, vec!["-g".into()]));
    }
    on_path("zed").map(|zed| (zed, Vec::new()))
}

/// A program on the PATH. On Windows `code` is a `.cmd` file, which
/// `Command` doesn't look for by itself.
fn on_path(name: &str) -> Option<PathBuf> {
    let extensions: &[&str] = if cfg!(windows) {
        &[".cmd", ".exe"]
    } else {
        &[""]
    };
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|dir| {
        extensions
            .iter()
            .map(|extension| dir.join(format!("{name}{extension}")))
            .find(|path| path.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_shows_messages_from_the_project_folder() {
        let diagnostic = Diagnostic {
            level: Level::Error,
            message: "cannot find value `x` in this scope".into(),
            code: Some("E0425".into()),
            location: Some(oddity_build::Location {
                file: "/games/shooter/src/player.rs".into(),
                line: 12,
                column: 9,
            }),
            rendered: String::new(),
        };
        let line = diagnostic_line(&diagnostic, Some(Path::new("/games/shooter")));
        assert_eq!(
            line.text,
            "error[E0425]: cannot find value `x` in this scope  ·  src/player.rs:12:9"
        );
        assert_eq!(
            line.location,
            Some(("/games/shooter/src/player.rs".into(), 12, 9))
        );
    }

    #[test]
    fn build_errors_reports_the_last_build() {
        let mut world = World::new();
        let mut builds = Builds {
            status: BuildStatus::Failed,
            ..default()
        };
        builds.diagnostics.push(Diagnostic {
            level: Level::Warning,
            message: "unused variable".into(),
            code: None,
            location: None,
            rendered: "warning: unused variable".into(),
        });
        world.insert_resource(builds);
        let report = build_errors(&world, &Args::new()).unwrap();
        assert_eq!(report["status"], "failed");
        assert_eq!(report["messages"][0]["level"], "warning");
        assert_eq!(report["messages"][0]["line"], Value::Null);
    }

    #[test]
    fn watching_doesnt_redraw_output_every_frame() {
        let dir = tempfile::tempdir().unwrap();
        let source = OdditySource::Path("crates/oddity".into());
        let project = Project::create_game(dir.path(), "Quiet", &source).unwrap();
        // Described already, so opening it doesn't start a build.
        std::fs::write(project.types_file(), "{}").unwrap();
        let mut app = App::new();
        app.insert_resource(CurrentProject(Some(project)))
            .init_resource::<Builds>()
            .add_systems(Update, (watch_code, follow_build).chain());
        app.update();
        let changed = |app: &App| {
            app.world()
                .get_resource_change_ticks::<Builds>()
                .unwrap()
                .changed
        };
        let before = changed(&app);
        for _ in 0..5 {
            app.update();
        }
        assert_eq!(changed(&app), before);
        assert_eq!(
            app.world().resource::<Builds>().status,
            BuildStatus::NotBuilt
        );
    }

    #[test]
    fn building_needs_a_game() {
        let mut world = World::new();
        world.init_resource::<Builds>();
        world.insert_resource(CurrentProject(None));
        assert!(matches!(
            start_build(&mut world),
            Err(CommandError::Invalid(_))
        ));
    }
}
