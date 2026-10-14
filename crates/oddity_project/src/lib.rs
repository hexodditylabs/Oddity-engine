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
//! ```
//!
//! The Hub reads `[engine] editor` to launch the right editor version.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

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
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectFile {
    /// Name and version.
    pub project: ProjectSection,
    /// Which editor and Bevy version the project is pinned to.
    pub engine: EngineSection,
    /// Editor settings stored with the project.
    #[serde(default, skip_serializing_if = "EditorSection::is_empty")]
    pub editor: EditorSection,
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
    pub fn find(start: impl AsRef<Path>) -> Result<Self, ProjectError> {
        let start = start.as_ref();
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
    pub fn save(&self) -> Result<(), ProjectError> {
        self.validate()?;
        let path = self.file_path();
        let text =
            toml::to_string_pretty(&self.file).map_err(|e| ProjectError::Invalid(e.to_string()))?;
        fs::write(&path, text).map_err(|source| ProjectError::Io { path, source })
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
        if let Some(level) = &self.file.editor.main_level
            && Path::new(level).is_absolute()
        {
            return Err(ProjectError::Invalid(
                "[editor] main_level must be relative to the project folder".into(),
            ));
        }
        Ok(())
    }
}

fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let version = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(version)
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
}
