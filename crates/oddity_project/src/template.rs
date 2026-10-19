//! New game projects (`roadmap/scripting.md`, "Project layout"):
//!
//! ```text
//! my_game/
//! ├── oddity.toml
//! ├── Cargo.toml            the game crate: a library, depends on `oddity`
//! ├── src/lib.rs            `pub struct GamePlugin`
//! ├── levels/main.level     a camera, a light and a floor
//! ├── .gitignore
//! ├── .cargo/config.toml    the shared target folder, and `cargo play`
//! └── .oddity/runner/       oddity::run::<my_game::GamePlugin>()
//! ```
//!
//! `.oddity/` and `.cargo/config.toml` are this machine's, so they are
//! git-ignored and written again by [`Project::write_runner`]: after a clone,
//! the editor writes them before it builds.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{BEVY_VERSION, EDITOR_VERSION, Project, ProjectError, write_atomic};

/// Where a game gets the `oddity` crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OdditySource {
    /// The engine's repository at the tag of an editor version: how
    /// released editors make projects.
    Git {
        /// The repository.
        url: String,
        /// The tag, `v` and the editor version.
        tag: String,
    },
    /// A checkout of the engine: the `crates/oddity` folder in it. For
    /// editors run from source, and tests.
    Path(PathBuf),
}

impl OdditySource {
    /// This editor's release: its repository, at its version's tag.
    pub fn release() -> Self {
        Self::Git {
            url: REPOSITORY.to_owned(),
            tag: format!("v{EDITOR_VERSION}"),
        }
    }

    /// The `oddity = ...` value in a `Cargo.toml`.
    fn dependency(&self) -> String {
        match self {
            Self::Git { url, tag } => format!("{{ git = {url:?}, tag = {tag:?} }}"),
            Self::Path(path) => format!("{{ path = {:?} }}", slash_path(path)),
        }
    }
}

/// The first line of the files the editor writes for this machine.
const WRITTEN_BY_EDITOR: &str = "# Written by the Oddity editor for this machine; git-ignored.";

/// The engine's repository.
const REPOSITORY: &str = "https://github.com/hexodditylabs/Oddity-engine";

/// The folder every project made by this editor version builds into, so
/// Bevy compiles once per machine, not once per project: in the OS's cache
/// folder, which can be emptied at any time.
pub fn shared_target_dir() -> PathBuf {
    cache_dir()
        .join("oddity")
        .join("target")
        .join(EDITOR_VERSION)
}

fn cache_dir() -> PathBuf {
    let var = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    if cfg!(windows) {
        if let Some(local) = var("LOCALAPPDATA") {
            return PathBuf::from(local);
        }
    } else if let Some(cache) = var("XDG_CACHE_HOME").filter(|_| !cfg!(target_os = "macos")) {
        return PathBuf::from(cache);
    }
    let home = var("HOME")
        .or_else(|| var("USERPROFILE"))
        .map(PathBuf::from);
    match home {
        Some(home) if cfg!(target_os = "macos") => home.join("Library").join("Caches"),
        Some(home) => home.join(".cache"),
        None => std::env::temp_dir(),
    }
}

/// A Rust crate name from a project name: `My Game!` → `my_game`.
pub fn crate_name(project: &str) -> String {
    let mut name = String::new();
    for c in project.chars() {
        if c.is_ascii_alphanumeric() {
            name.push(c.to_ascii_lowercase());
        } else if !name.is_empty() && !name.ends_with('_') {
            name.push('_');
        }
    }
    let name = name.trim_end_matches('_');
    match name.chars().next() {
        None => "game".to_owned(),
        Some(first) if first.is_ascii_digit() => format!("game_{name}"),
        Some(_) if RESERVED.contains(&name) => format!("{name}_game"),
        Some(_) => name.to_owned(),
    }
}

/// Names a crate can't take: Rust keywords, and crates every game links.
const RESERVED: [&str; 12] = [
    "self", "super", "crate", "type", "fn", "mod", "use", "test", "std", "core", "oddity", "bevy",
];

/// The profiles of the engine's own workspace, which a game doesn't inherit:
/// dependencies (Bevy) optimised even in debug builds, or the game crawls.
const PROFILES: &str = r#"[profile.dev]
opt-level = 1

[profile.dev.package."*"]
opt-level = 3

[profile.release]
codegen-units = 1
lto = "thin"
"#;

const MAIN_LEVEL: &str = "levels/main.level";

impl Project {
    /// A new game in `dir` (created if missing): the project file, the game
    /// crate, a main level with a camera, a light and a floor, the
    /// `.gitignore`, and the runner. `oddity` comes from `source`.
    pub fn create_game(
        dir: impl AsRef<Path>,
        name: impl Into<String>,
        source: &OdditySource,
    ) -> Result<Self, ProjectError> {
        let mut project = Self::create(dir, name)?;
        project.file.editor.main_level = Some(MAIN_LEVEL.to_owned());
        project.file.input = default_input();
        project.save()?;
        let crate_name = crate_name(project.name());
        let files = [
            ("Cargo.toml", game_manifest(&crate_name, source)),
            ("src/lib.rs", GAME_LIB.to_owned()),
            (MAIN_LEVEL, MAIN_LEVEL_TEXT.to_owned()),
            (".gitignore", GITIGNORE.to_owned()),
        ];
        for (path, text) in files {
            project.write_file(path, &text)?;
        }
        project.write_runner(source)?;
        Ok(project)
    }

    /// Write this machine's files: `.oddity/runner`, which calls
    /// `oddity::run` with the game's plugin, and `.cargo/config.toml`, which
    /// points the build at the [shared target folder](shared_target_dir)
    /// and adds `cargo play`, unless that file was written by hand. The
    /// editor calls this before every build, so a fresh clone and a new
    /// editor version both just work.
    pub fn write_runner(&self, source: &OdditySource) -> Result<(), ProjectError> {
        let crate_name = crate_name(self.name());
        let files = [
            (
                ".oddity/runner/Cargo.toml",
                runner_manifest(&crate_name, source),
            ),
            (
                ".oddity/runner/src/main.rs",
                format!(
                    "fn main() -> oddity::bevy::app::AppExit {{\n    oddity::run::<{crate_name}::GamePlugin>()\n}}\n"
                ),
            ),
            (
                ".cargo/config.toml",
                format!(
                    "{WRITTEN_BY_EDITOR}\n\n\
                     [build]\n\
                     # Shared by every project of this editor version.\n\
                     target-dir = {:?}\n\n\
                     [alias]\n\
                     # Play the game from a terminal: `cargo play`.\n\
                     play = \"run --manifest-path .oddity/runner/Cargo.toml --\"\n",
                    slash_path(&shared_target_dir())
                ),
            ),
        ];
        for (path, text) in files {
            if path == ".cargo/config.toml" && !self.wrote(path) {
                continue;
            }
            self.write_file(path, &text)?;
        }
        // A new runner starts from the game's resolved versions, so it
        // builds against what the game's own builds already compiled.
        let (game_lock, runner_lock) = (
            self.root.join("Cargo.lock"),
            self.root.join(".oddity/runner/Cargo.lock"),
        );
        if game_lock.is_file() && !runner_lock.exists() {
            fs::copy(&game_lock, &runner_lock).map_err(|source| ProjectError::Io {
                path: runner_lock,
                source,
            })?;
        }
        Ok(())
    }

    /// Whether the file at `path` is missing or the editor's own: a
    /// `.cargo/config.toml` someone wrote by hand is left alone.
    fn wrote(&self, path: &str) -> bool {
        fs::read_to_string(self.root.join(path))
            .map_or(true, |text| text.starts_with(WRITTEN_BY_EDITOR))
    }

    fn write_file(&self, path: &str, text: &str) -> Result<(), ProjectError> {
        let full = self.root.join(path);
        let io = |source| ProjectError::Io {
            path: full.clone(),
            source,
        };
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).map_err(io)?;
        }
        write_atomic(&full, text.as_bytes()).map_err(io)
    }
}

/// `/` on every OS, which Cargo reads on Windows too.
fn slash_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn default_input() -> crate::InputSection {
    let mut input = crate::InputSection::default();
    input.axes2.insert(
        "move".to_owned(),
        crate::Axis2Entry {
            left: Some("KeyA".into()),
            right: Some("KeyD".into()),
            up: Some("KeyW".into()),
            down: Some("KeyS".into()),
            stick: Some("LeftStick".into()),
        },
    );
    input
}

fn game_manifest(crate_name: &str, source: &OdditySource) -> String {
    format!(
        "[package]\n\
         name = {crate_name:?}\n\
         version = \"0.1.0\"\n\
         edition = \"2024\"\n\
         publish = false\n\n\
         # A game is its own Cargo workspace.\n\
         [workspace]\n\n\
         [dependencies]\n\
         # Bevy {BEVY_VERSION}, scripts and the game program.\n\
         oddity = {}\n\n\
         {PROFILES}",
        source.dependency()
    )
}

fn runner_manifest(crate_name: &str, source: &OdditySource) -> String {
    format!(
        "# Written by the Oddity editor; git-ignored.\n\
         [package]\n\
         name = \"{crate_name}_runner\"\n\
         version = \"0.1.0\"\n\
         edition = \"2024\"\n\
         publish = false\n\n\
         [workspace]\n\n\
         [dependencies]\n\
         {crate_name} = {{ path = \"../..\" }}\n\
         oddity = {}\n\n\
         {PROFILES}",
        source.dependency()
    )
}

const GAME_LIB: &str = r#"//! The game. Scripts are structs marked `#[script]`, anywhere in this
//! crate; they register themselves. Systems and resources go in
//! `GamePlugin`.

use oddity::prelude::*;

/// Everything the game adds besides scripts.
#[derive(Default)]
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, _app: &mut App) {}
}
"#;

const GITIGNORE: &str = "# Written by the Oddity editor for this machine.\n\
                         /.oddity/\n\
                         /.cargo/config.toml\n\
                         /target/\n";

/// A camera looking at a floor, lit by the sun.
const MAIN_LEVEL_TEXT: &str = r#"{
  "format": "oddity-level",
  "version": 1,
  "nodes": [
    {
      "id": 1,
      "name": "Main",
      "parent": null,
      "kind": "Node3D",
      "components": {
        "bevy_camera::visibility::Visibility": "Inherited",
        "bevy_transform::components::transform::Transform": {
          "translation": [0.0, 0.0, 0.0],
          "rotation": [0.0, 0.0, 0.0, 1.0],
          "scale": [1.0, 1.0, 1.0]
        }
      }
    },
    {
      "id": 2,
      "name": "Camera3D",
      "parent": 1,
      "kind": "Camera3D",
      "components": {
        "bevy_camera::camera::Camera": {
          "viewport": null,
          "order": 0,
          "is_active": false,
          "computed": {
            "clip_from_view": [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            "target_info": null,
            "old_viewport_size": null,
            "old_sub_camera_view": null
          },
          "output_mode": { "Write": { "blend_state": null, "clear_color": "Default" } },
          "msaa_writeback": "Auto",
          "clear_color": "Default",
          "invert_culling": false,
          "sub_camera_view": null
        },
        "bevy_camera::camera::RenderTarget": { "Window": "Primary" },
        "bevy_camera::components::Camera3d": {
          "depth_load_op": { "Clear": 0.0 },
          "depth_texture_usages": 16
        },
        "bevy_transform::components::transform::Transform": {
          "translation": [0.0, 4.0, 8.0],
          "rotation": [-0.2297529, 0.0, 0.0, 0.97324896],
          "scale": [1.0, 1.0, 1.0]
        }
      }
    },
    {
      "id": 3,
      "name": "DirectionalLight3D",
      "parent": 1,
      "kind": "DirectionalLight3D",
      "components": {
        "bevy_light::directional_light::DirectionalLight": {
          "color": { "LinearRgba": { "red": 1.0, "green": 1.0, "blue": 1.0, "alpha": 1.0 } },
          "illuminance": 8000.0,
          "shadow_maps_enabled": true,
          "contact_shadows_enabled": false,
          "affects_lightmapped_mesh_diffuse": true,
          "shadow_depth_bias": 0.02,
          "shadow_normal_bias": 1.8
        },
        "bevy_transform::components::transform::Transform": {
          "translation": [4.0, 10.0, 2.0],
          "rotation": [-0.46270704, 0.4411521, 0.28596866, 0.713799],
          "scale": [1.0, 1.0, 1.0]
        }
      }
    },
    {
      "id": 4,
      "name": "Floor",
      "parent": 1,
      "kind": "MeshInstance3D",
      "components": {
        "oddity_level::shapes::MeshShape": {
          "shape": { "Plane": { "size": [20.0, 20.0] } },
          "color": { "Srgba": { "red": 0.35, "green": 0.4, "blue": 0.35, "alpha": 1.0 } }
        }
      }
    }
  ]
}
"#;
