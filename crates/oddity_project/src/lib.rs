//! The `oddity.toml` project file, Oddity's `project.godot`.
//!
//! ```toml
//! [project]
//! name = "My Game"
//! version = "0.1.0"
//!
//! [engine]
//! editor = "0.1.0"   # Oddity editor version this project is pinned to
//! bevy = "0.20.0"    # Bevy seed version
//!
//! [editor]
//! main_level = "levels/main.level"   # optional
//!
//! [input.actions]                      # the game's input map, optional
//! fire = ["Space", "MouseLeft", "GamepadSouth"]
//!
//! [input.axes2]
//! move = { left = "KeyA", right = "KeyD", up = "KeyW", down = "KeyS", stick = "LeftStick" }
//! ```
//!
//! The Hub reads `[engine] editor` to launch the right editor version.
//!
//! [`assets`] keeps the project's asset IDs, in `.import` files next to the
//! assets. [`Project::create_game`] makes a whole new game: the game crate,
//! a main level and the runner (see `template`).

pub mod assets;
mod template;

use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

pub use assets::{AssetError, AssetId, AssetIndex, IMPORT_EXTENSION, ImportFile};
pub use template::{OdditySource, crate_name, shared_target_dir};

/// The project file's name.
pub const FILE_NAME: &str = "oddity.toml";

/// The editor version this crate was built with.
pub const EDITOR_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The Bevy version Oddity is seeded from.
pub const BEVY_VERSION: &str = "0.20.0";

/// Something went wrong reading or writing a project.
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    /// Reading or writing a file failed.
    #[error("{path}: {source}")]
    Io {
        /// The file or folder.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// `oddity.toml` is not valid.
    #[error("{path} is not a valid project file: {message}")]
    Parse {
        /// The file.
        path: PathBuf,
        /// What is wrong, with line and column.
        message: String,
    },
    /// No `oddity.toml` in the folder or any folder above it.
    #[error("no {FILE_NAME} found in {0} or any folder above it")]
    NotFound(PathBuf),
    /// Creating a project where one already exists.
    #[error("{0} already contains an Oddity project")]
    AlreadyExists(PathBuf),
    /// The file parsed but its values make no sense.
    #[error("invalid project: {0}")]
    Invalid(String),
}

/// The contents of `oddity.toml`.
///
/// Unknown top-level sections are ignored, unlike unknown keys inside a
/// section: a section added by a newer editor must not stop an older one from
/// reading `[engine] editor` and saying which version the project needs.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ProjectFile {
    /// Name and version.
    pub project: ProjectSection,
    /// Which editor and Bevy version the project is pinned to.
    pub engine: EngineSection,
    /// Editor settings stored with the project.
    #[serde(default, skip_serializing_if = "EditorSection::is_empty")]
    pub editor: EditorSection,
    /// The game's named actions and axes.
    #[serde(default, skip_serializing_if = "InputSection::is_empty")]
    pub input: InputSection,
}

/// `[project]`
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectSection {
    /// Shown in the Hub and the editor's title bar.
    pub name: String,
    /// The game's own version.
    #[serde(default = "default_version")]
    pub version: String,
}

/// `[engine]`
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EngineSection {
    /// Oddity editor version, e.g. `"0.1.0"`.
    pub editor: String,
    /// Bevy version, e.g. `"0.20.0"`.
    pub bevy: String,
}

/// `[editor]`
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EditorSection {
    /// Level opened on start and run by Play, relative to the project root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_level: Option<String>,
}

impl EditorSection {
    fn is_empty(&self) -> bool {
        self.main_level.is_none()
    }
}

/// `[input]`: the input map, which the game reads at start. Bindings are
/// kept as written; the scripting runtime knows the key names and checks
/// them.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InputSection {
    /// `[input.actions]`: action name → its bindings (`"Space"`,
    /// `"MouseLeft"`, `"GamepadSouth"`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub actions: BTreeMap<String, Vec<String>>,
    /// `[input.axes2]`: two-way axes, such as `move`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub axes2: BTreeMap<String, Axis2Entry>,
}

impl InputSection {
    fn is_empty(&self) -> bool {
        self.actions.is_empty() && self.axes2.is_empty()
    }
}

/// One `[input.axes2]` entry: a binding per direction, and a stick
/// (`"LeftStick"`, `"RightStick"`, `"DPad"`). Each part is optional.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Axis2Entry {
    /// Towards -x.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<String>,
    /// Towards +x.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<String>,
    /// Towards +y.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub up: Option<String>,
    /// Towards -y.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub down: Option<String>,
    /// Added to the keys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stick: Option<String>,
}

fn default_version() -> String {
    "0.1.0".into()
}

/// How a project's pinned editor version compares to this editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionMatch {
    /// Same version.
    Same,
    /// The project wants an older editor.
    ProjectIsOlder,
    /// The project wants a newer editor.
    ProjectIsNewer,
}

/// An Oddity project on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// The folder that holds `oddity.toml`.
    pub root: PathBuf,
    /// The parsed file.
    pub file: ProjectFile,
}

impl Project {
    /// Create a new project in `dir` (created if missing) pinned to this editor.
    pub fn create(dir: impl AsRef<Path>, name: impl Into<String>) -> Result<Self, ProjectError> {
        let root = dir.as_ref().to_path_buf();
        if root.join(FILE_NAME).exists() {
            return Err(ProjectError::AlreadyExists(root));
        }
        fs::create_dir_all(&root).map_err(|source| ProjectError::Io {
            path: root.clone(),
            source,
        })?;
        let project = Self {
            root,
            file: ProjectFile {
                project: ProjectSection {
                    name: name.into(),
                    version: default_version(),
                },
                engine: EngineSection {
                    editor: EDITOR_VERSION.into(),
                    bevy: BEVY_VERSION.into(),
                },
                editor: EditorSection::default(),
                input: InputSection::default(),
            },
        };
        project.validate()?;
        project.save()?;
        Ok(project)
    }

    /// Load the project in `path` (a folder or the `oddity.toml` itself).
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ProjectError> {
        let path = path.as_ref();
        let (root, file_path) = if path.file_name().is_some_and(|n| n == FILE_NAME) {
            (
                path.parent().unwrap_or(Path::new(".")).to_path_buf(),
                path.to_path_buf(),
            )
        } else {
            (path.to_path_buf(), path.join(FILE_NAME))
        };
        let text = fs::read_to_string(&file_path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                ProjectError::NotFound(root.clone())
            } else {
                ProjectError::Io {
                    path: file_path.clone(),
                    source,
                }
            }
        })?;
        let file: ProjectFile = toml::from_str(&text).map_err(|e| ProjectError::Parse {
            path: file_path,
            message: e.to_string(),
        })?;
        let project = Self { root, file };
        project.validate()?;
        Ok(project)
    }

    /// Find the project that contains `start`, searching upward like Cargo does.
    ///
    /// A relative `start` is resolved against the working directory first, so
    /// the returned project's root is absolute.
    pub fn find(start: impl AsRef<Path>) -> Result<Self, ProjectError> {
        // Relative paths run out of parents early (`"."` has the parent `""`,
        // which has none), so the search could never leave the start folder.
        // `absolute` rather than `canonicalize`, which gives verbatim `\\?\`
        // paths on Windows that many tools refuse.
        let start = std::path::absolute(start.as_ref()).map_err(|source| ProjectError::Io {
            path: start.as_ref().to_path_buf(),
            source,
        })?;
        let start = start.as_path();
        let mut dir = Some(if start.is_file() {
            start.parent().unwrap_or(start)
        } else {
            start
        });
        while let Some(current) = dir {
            if current.join(FILE_NAME).is_file() {
                return Self::load(current);
            }
            dir = current.parent();
        }
        Err(ProjectError::NotFound(start.to_path_buf()))
    }

    /// Write `oddity.toml`.
    ///
    /// The old file stays whole until the new one is fully written, so a
    /// crash or a full disk mid-save never leaves an empty project file.
    pub fn save(&self) -> Result<(), ProjectError> {
        self.validate()?;
        let path = self.file_path();
        let text =
            toml::to_string_pretty(&self.file).map_err(|e| ProjectError::Invalid(e.to_string()))?;
        write_atomic(&path, text.as_bytes()).map_err(|source| ProjectError::Io { path, source })
    }

    /// Path to `oddity.toml`.
    pub fn file_path(&self) -> PathBuf {
        self.root.join(FILE_NAME)
    }

    /// The project's name.
    pub fn name(&self) -> &str {
        &self.file.project.name
    }

    /// How the project's pinned editor version compares to `editor`.
    pub fn editor_version_match(&self, editor: &str) -> VersionMatch {
        match compare_versions(&self.file.engine.editor, editor) {
            std::cmp::Ordering::Equal => VersionMatch::Same,
            std::cmp::Ordering::Less => VersionMatch::ProjectIsOlder,
            std::cmp::Ordering::Greater => VersionMatch::ProjectIsNewer,
        }
    }

    fn validate(&self) -> Result<(), ProjectError> {
        if self.file.project.name.trim().is_empty() {
            return Err(ProjectError::Invalid(
                "[project] name cannot be empty".into(),
            ));
        }
        for (key, value) in [
            ("[engine] editor", &self.file.engine.editor),
            ("[engine] bevy", &self.file.engine.bevy),
        ] {
            if parse_version(value).is_none() {
                return Err(ProjectError::Invalid(format!(
                    "{key} must be a version like 1.2.3, not {value:?}"
                )));
            }
        }
        if let Some(level) = &self.file.editor.main_level {
            check_main_level(level)?;
        }
        Ok(())
    }
}

/// Replace the file at `path` with `bytes`. The old file stays whole until
/// the new one is fully written, so a crash or a full disk mid-save never
/// leaves an empty or half-written file.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    // Same folder as the target, because a rename is only atomic within one
    // filesystem; a dot name, so scans skip it. `fs::rename` replaces an
    // existing file on Windows too.
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let written = write_synced(&temp, bytes).and_then(|()| fs::rename(&temp, path));
    if written.is_err() {
        // Best effort: the original error is the one worth reporting.
        let _ = fs::remove_file(&temp);
    }
    written
}

/// Write `bytes` to a new file at `path` and flush them to disk, so a rename
/// that follows can't land before the contents do.
fn write_synced(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// `[editor] main_level` must name a file inside the project on every OS.
fn check_main_level(level: &str) -> Result<(), ProjectError> {
    check_relative(level)
        .map_err(|why| ProjectError::Invalid(format!("[editor] main_level {level:?} {why}")))
}

/// A path stored in a project file must name a file inside the project on
/// every OS; the error says why it doesn't.
///
/// The text is checked rather than `Path`, because `Path` follows the OS the
/// editor runs on: `/levels/x.level` isn't absolute on Windows and
/// `C:\levels\x.level` isn't absolute on Linux, yet the same project
/// must mean the same file on both.
pub(crate) fn check_relative(path: &str) -> Result<(), String> {
    if path.contains('\\') {
        return Err("must use / to separate folders, not \\".into());
    }
    // A leading `/` is a root (or a `//server` share); a `:` is a Windows
    // drive such as `C:`, and isn't allowed in Windows file names anyway.
    if path.starts_with('/') || path.contains(':') {
        return Err("must be relative to the project folder".into());
    }
    if path.split('/').any(|part| part == "..") {
        return Err("cannot use .. to point outside the project folder".into());
    }
    Ok(())
}

/// A version, ordered by semver precedence (field order matters for `Ord`).
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Version<'a> {
    core: (u64, u64, u64),
    /// Before `pre`, so a release sorts above its own pre-releases.
    is_release: bool,
    /// Semver compares dot-separated identifiers, numeric ones as numbers.
    /// Whole-string order is enough for `alpha < beta < rc` without a semver
    /// dependency, at the cost of `rc.10` sorting before `rc.2`.
    pre: Option<&'a str>,
}

fn parse_version(text: &str) -> Option<Version<'_>> {
    // Build metadata doesn't take part in precedence.
    let text = text.split('+').next()?;
    let (core, pre) = match text.split_once('-') {
        Some((_, "")) => return None,
        Some((core, pre)) => (core, Some(pre)),
        None => (text, None),
    };
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let core = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(Version {
        core,
        is_release: pre.is_none(),
        pre,
    })
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    parse_version(a).cmp(&parse_version(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let created = Project::create(dir.path().join("my_game"), "My Game").unwrap();
        let loaded = Project::load(dir.path().join("my_game")).unwrap();
        assert_eq!(created, loaded);
        assert_eq!(loaded.name(), "My Game");
        assert_eq!(loaded.file.engine.editor, EDITOR_VERSION);
        assert_eq!(loaded.file.engine.bevy, "0.20.0");
        // Loading via the file path works too.
        assert_eq!(Project::load(loaded.file_path()).unwrap(), created);
    }

    #[test]
    fn written_file_is_readable() {
        let dir = tempfile::tempdir().unwrap();
        let project = Project::create(dir.path(), "Readable").unwrap();
        let text = fs::read_to_string(project.file_path()).unwrap();
        assert!(text.contains("[project]"));
        assert!(text.contains("name = \"Readable\""));
        assert!(text.contains("[engine]"));
        assert!(!text.contains("[editor]"), "empty sections are left out");
    }

    #[test]
    fn the_input_map_loads_and_survives_a_save() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(FILE_NAME),
            "[project]\nname = \"Input\"\n\
             [engine]\neditor = \"0.1.0\"\nbevy = \"0.20.0\"\n\
             [input.actions]\nfire = [\"Space\", \"MouseLeft\"]\n\
             [input.axes2]\nmove = { left = \"KeyA\", right = \"KeyD\", stick = \"LeftStick\" }\n",
        )
        .unwrap();
        let project = Project::load(dir.path()).unwrap();
        assert_eq!(project.file.input.actions["fire"], ["Space", "MouseLeft"]);
        let movement = &project.file.input.axes2["move"];
        assert_eq!(movement.left.as_deref(), Some("KeyA"));
        assert_eq!(movement.up, None);
        assert_eq!(movement.stick.as_deref(), Some("LeftStick"));

        project.save().unwrap();
        assert_eq!(Project::load(dir.path()).unwrap(), project);
    }

    #[test]
    fn create_refuses_to_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        Project::create(dir.path(), "First").unwrap();
        assert!(matches!(
            Project::create(dir.path(), "Second"),
            Err(ProjectError::AlreadyExists(_))
        ));
    }

    #[test]
    fn find_searches_upward() {
        let dir = tempfile::tempdir().unwrap();
        Project::create(dir.path(), "Nested").unwrap();
        let deep = dir.path().join("src/scripts/player");
        fs::create_dir_all(&deep).unwrap();
        assert_eq!(Project::find(&deep).unwrap().name(), "Nested");

        let empty = tempfile::tempdir().unwrap();
        assert!(matches!(
            Project::find(empty.path()),
            Err(ProjectError::NotFound(_))
        ));
    }

    #[test]
    fn save_keeps_edits() {
        let dir = tempfile::tempdir().unwrap();
        let mut project = Project::create(dir.path(), "Edit").unwrap();
        project.file.editor.main_level = Some("levels/main.level".into());
        project.save().unwrap();
        let loaded = Project::load(dir.path()).unwrap();
        assert_eq!(
            loaded.file.editor.main_level.as_deref(),
            Some("levels/main.level")
        );
    }

    #[test]
    fn rejects_bad_files() {
        let dir = tempfile::tempdir().unwrap();
        let write = |text: &str| fs::write(dir.path().join(FILE_NAME), text).unwrap();

        write("[project]\nname = \"X\"\n");
        assert!(matches!(
            Project::load(dir.path()),
            Err(ProjectError::Parse { .. })
        ));

        write("[project]\nname = \"\"\n[engine]\neditor = \"0.1.0\"\nbevy = \"0.20.0\"\n");
        assert!(matches!(
            Project::load(dir.path()),
            Err(ProjectError::Invalid(_))
        ));

        write("[project]\nname = \"X\"\n[engine]\neditor = \"latest\"\nbevy = \"0.20.0\"\n");
        assert!(matches!(
            Project::load(dir.path()),
            Err(ProjectError::Invalid(_))
        ));

        write(
            "[project]\nname = \"X\"\ntypo = 1\n[engine]\neditor = \"0.1.0\"\nbevy = \"0.20.0\"\n",
        );
        assert!(
            matches!(Project::load(dir.path()), Err(ProjectError::Parse { .. })),
            "unknown keys are caught, so typos don't silently do nothing"
        );
    }

    #[test]
    fn version_matching() {
        let dir = tempfile::tempdir().unwrap();
        let mut project = Project::create(dir.path(), "V").unwrap();
        project.file.engine.editor = "0.1.0".into();
        assert_eq!(project.editor_version_match("0.1.0"), VersionMatch::Same);
        assert_eq!(
            project.editor_version_match("0.2.0"),
            VersionMatch::ProjectIsOlder
        );
        assert_eq!(
            project.editor_version_match("0.0.9"),
            VersionMatch::ProjectIsNewer
        );
        assert_eq!(
            project.editor_version_match("0.10.0"),
            VersionMatch::ProjectIsOlder
        );
    }

    /// Restores the working directory when dropped, even if the test panics,
    /// so the temporary folder can be deleted (Windows refuses to delete the
    /// current directory).
    struct CurrentDirGuard(PathBuf);

    impl CurrentDirGuard {
        fn enter(dir: &Path) -> Self {
            let previous = std::env::current_dir().unwrap();
            std::env::set_current_dir(dir).unwrap();
            Self(previous)
        }
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }

    // The only test that changes the working directory; every other test
    // uses absolute temporary paths, so running in parallel is safe.
    #[test]
    fn find_searches_upward_from_relative_paths() {
        let dir = tempfile::tempdir().unwrap();
        Project::create(dir.path(), "Relative").unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let same_folder = |a: &Path, b: &Path| {
            assert_eq!(fs::canonicalize(a).unwrap(), fs::canonicalize(b).unwrap());
        };

        // `oddity-editor .` run from inside `src`.
        let from_src = {
            let _cwd = CurrentDirGuard::enter(&src);
            Project::find(".").unwrap()
        };
        assert!(from_src.root.is_absolute());
        same_folder(&from_src.root, dir.path());

        // `oddity-editor src` run from the project root.
        let from_root = {
            let _cwd = CurrentDirGuard::enter(dir.path());
            Project::find("src").unwrap()
        };
        assert!(
            from_root.root.is_absolute(),
            "root {:?} must still work after the working directory changes",
            from_root.root
        );
        same_folder(&from_root.root, dir.path());
    }

    #[test]
    fn unknown_sections_from_newer_editors_still_load() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(FILE_NAME),
            "[project]\nname = \"Future\"\n\
             [engine]\neditor = \"99.0.0\"\nbevy = \"0.20.0\"\n\
             [steam]\napp_id = 480\n",
        )
        .unwrap();
        let project = Project::load(dir.path()).unwrap();
        assert_eq!(project.file.engine.editor, "99.0.0");
        assert_eq!(
            project.editor_version_match(EDITOR_VERSION),
            VersionMatch::ProjectIsNewer
        );
    }

    #[test]
    fn main_level_must_stay_inside_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let mut project = Project::create(dir.path(), "Levels").unwrap();
        let mut check = |level: &str| {
            project.file.editor.main_level = Some(level.into());
            project.save()
        };

        for level in ["levels/main.level", "main.level", "levels/./main.level"] {
            assert!(check(level).is_ok(), "{level:?} should be accepted");
        }
        for level in [
            "/levels/main.level",
            "../../x.level",
            "levels/../../x.level",
            "C:/levels/x.level",
            "C:x.level",
            "//server/share/x.level",
        ] {
            assert!(
                matches!(check(level), Err(ProjectError::Invalid(_))),
                "{level:?} should be rejected"
            );
        }
        for level in [r"C:\levels\x.level", r"levels\main.level"] {
            match check(level) {
                Err(ProjectError::Invalid(message)) => {
                    assert!(message.contains('/'), "{message:?} should say to use /")
                }
                other => panic!("{level:?} should be rejected, got {other:?}"),
            }
        }
    }

    #[test]
    fn save_replaces_the_file_instead_of_rewriting_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut project = Project::create(dir.path(), "Before").unwrap();
        // A second name for the current file shows whether save truncates
        // it in place (a crash mid-write would leave it empty) or swaps a
        // fully written file in, leaving the old one whole.
        let old = dir.path().join("old.toml");
        fs::hard_link(project.file_path(), &old).unwrap();

        project.file.project.name = "After".into();
        project.save().unwrap();

        assert!(fs::read_to_string(&old).unwrap().contains("\"Before\""));
        assert_eq!(Project::load(dir.path()).unwrap().name(), "After");
        let mut names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        names.sort();
        assert_eq!(names, [FILE_NAME, "old.toml"], "no temporary file is left");
    }

    #[test]
    fn pre_releases_are_older_than_their_release() {
        let dir = tempfile::tempdir().unwrap();
        let mut project = Project::create(dir.path(), "Pre").unwrap();
        let mut compare = |pinned: &str, editor: &str| {
            project.file.engine.editor = pinned.into();
            project.editor_version_match(editor)
        };
        assert_eq!(
            compare("0.2.0-alpha", "0.2.0"),
            VersionMatch::ProjectIsOlder
        );
        assert_eq!(
            compare("0.2.0", "0.2.0-alpha"),
            VersionMatch::ProjectIsNewer
        );
        assert_eq!(
            compare("0.2.0-alpha", "0.2.0-beta"),
            VersionMatch::ProjectIsOlder
        );
        assert_eq!(
            compare("0.2.0-alpha", "0.1.9"),
            VersionMatch::ProjectIsNewer
        );
        assert_eq!(compare("0.2.0-rc", "0.2.0-rc"), VersionMatch::Same);
        assert_eq!(compare("0.2.0+build.5", "0.2.0"), VersionMatch::Same);
    }
}
