//! The project open in this editor.

use bevy::prelude::*;
use oddity_project::{Project, VersionMatch};

/// The open project, if any. Set from the command line:
/// `oddity-editor path/to/game` (any folder inside the project works).
#[derive(Resource, Default)]
pub struct CurrentProject(pub Option<Project>);

impl CurrentProject {
    /// Open the project containing `path`, if one is given. Problems are
    /// logged and leave no project open.
    pub fn open(path: Option<&str>) -> Self {
        let Some(path) = path.filter(|a| !a.starts_with('-')) else {
            return Self(None);
        };
        match Project::find(path) {
            Ok(project) => {
                match project.editor_version_match(oddity_project::EDITOR_VERSION) {
                    VersionMatch::Same => {}
                    VersionMatch::ProjectIsOlder => warn!(
                        "{} was made with Oddity {}; this editor is {}",
                        project.name(),
                        project.file.engine.editor,
                        oddity_project::EDITOR_VERSION
                    ),
                    VersionMatch::ProjectIsNewer => warn!(
                        "{} needs Oddity {} or newer; this editor is {}",
                        project.name(),
                        project.file.engine.editor,
                        oddity_project::EDITOR_VERSION
                    ),
                }
                Self(Some(project))
            }
            Err(error) => {
                error!("could not open project: {error}");
                Self(None)
            }
        }
    }

    /// The project's name, if one is open.
    pub fn name(&self) -> Option<&str> {
        self.0.as_ref().map(|p| p.name())
    }
}
