//! One build of the game, in the background (`roadmap/scripting.md`,
//! "Build and describe"): cargo with JSON messages, then, if it worked, the
//! built program with `--oddity-describe`, whose output becomes
//! `.oddity/types.json`.

use std::{
    fs,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

use crate::messages::{CargoMessage, Diagnostic, parse_line, strip_ansi};

/// What a build reports, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildEvent {
    /// One of cargo's progress lines: "Compiling shooter v0.1.0".
    Progress(String),
    /// A compiler message.
    Diagnostic(Diagnostic),
    /// Cargo finished.
    Finished {
        /// Whether the game built.
        success: bool,
    },
    /// The game described its types; they are in this file.
    Described(PathBuf),
    /// Something other than the code went wrong: cargo couldn't be run, or
    /// the game couldn't describe itself.
    Failed(String),
}

/// What to build, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildRequest {
    /// Where cargo runs: the project folder, so its `.cargo/config.toml`
    /// (the shared target folder) applies.
    pub dir: PathBuf,
    /// The `Cargo.toml` of the program to build: the project's runner.
    pub manifest: PathBuf,
    /// The `cargo` to run.
    pub cargo: PathBuf,
    /// Where to write the game's type description after a good build.
    pub types: PathBuf,
}

/// A build in progress. Dropping it stops cargo, so closing the editor
/// leaves nothing running.
pub struct Build {
    // Behind a lock so a build can live in shared state (a Bevy resource).
    events: Mutex<Receiver<BuildEvent>>,
    cargo: Arc<Mutex<Option<Child>>>,
}

impl Build {
    /// Start building in the background.
    pub fn start(request: BuildRequest) -> Self {
        let (send, events) = mpsc::channel();
        let cargo = Arc::new(Mutex::new(None));
        let running = cargo.clone();
        thread::spawn(move || run(&request, &send, &running));
        Self {
            events: Mutex::new(events),
            cargo,
        }
    }

    /// What happened since the last call.
    pub fn events(&self) -> Vec<BuildEvent> {
        self.events
            .lock()
            .map(|events| events.try_iter().collect())
            .unwrap_or_default()
    }

    /// Stop cargo, for a newer build. Its remaining events are dropped with
    /// the build. Once cargo is done this does nothing.
    pub fn cancel(&self) {
        if let Some(child) = self.cargo.lock().ok().as_mut().and_then(|c| c.as_mut()) {
            let _ = child.kill();
        }
    }
}

impl Drop for Build {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn run(request: &BuildRequest, send: &Sender<BuildEvent>, running: &Arc<Mutex<Option<Child>>>) {
    let spawned = Command::new(&request.cargo)
        .args(["build", "--message-format=json-diagnostic-rendered-ansi"])
        .arg("--manifest-path")
        .arg(&request.manifest)
        .current_dir(&request.dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => {
            let cargo = request.cargo.display();
            let _ = send.send(BuildEvent::Failed(format!("can't run {cargo}: {error}")));
            return;
        }
    };
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        return;
    };
    if let Ok(mut slot) = running.lock() {
        *slot = Some(child);
    }

    let progress = send.clone();
    let progress = thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let line = strip_ansi(line.trim());
            if !line.is_empty() {
                let _ = progress.send(BuildEvent::Progress(line));
            }
        }
    });
    let mut executable = None;
    let mut success = false;
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        match parse_line(&line) {
            Some(CargoMessage::Diagnostic(diagnostic)) => {
                let _ = send.send(BuildEvent::Diagnostic(diagnostic));
            }
            Some(CargoMessage::Executable(path)) => executable = Some(path),
            Some(CargoMessage::Finished { success: done }) => success = done,
            Some(CargoMessage::Other) | None => {}
        }
    }
    let _ = progress.join();
    let exited = running
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
        .and_then(|mut child| child.wait().ok());
    let success = success && exited.is_some_and(|status| status.success());
    let _ = send.send(BuildEvent::Finished { success });
    if !success {
        return;
    }
    let event = match executable {
        Some(executable) => match describe(&executable, request) {
            Ok(()) => BuildEvent::Described(request.types.clone()),
            Err(why) => BuildEvent::Failed(why),
        },
        None => BuildEvent::Failed("the build made no program to describe".into()),
    };
    let _ = send.send(event);
}

/// Run the game with `--oddity-describe` and keep what it prints.
fn describe(executable: &PathBuf, request: &BuildRequest) -> Result<(), String> {
    let output = Command::new(executable)
        .arg("--oddity-describe")
        .current_dir(&request.dir)
        .output()
        .map_err(|error| format!("can't run {}: {error}", executable.display()))?;
    if !output.status.success() {
        return Err(format!(
            "the game couldn't describe itself: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .map_err(|error| format!("the game's description isn't JSON: {error}"))?;
    if let Some(parent) = request.types.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(&request.types, &output.stdout)
        .map_err(|error| format!("{}: {error}", request.types.display()))
}
