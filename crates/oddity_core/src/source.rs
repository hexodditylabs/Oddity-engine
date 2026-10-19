//! Who started a command: the [`Source`] every history step keeps, so you can
//! always see, and undo, what an agent, a script or a plugin changed
//! (`roadmap/commands.md`, rule 6).

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Who started a command.
///
/// As JSON (in `history.list` and `CommandExecuted`), the variant is a
/// `kind` field: `{"kind": "mcp", "client": "Claude"}`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    /// The person at the editor: menus, shortcuts, docks and fields.
    #[default]
    User,
    /// The `oddity-editor` command line.
    Cli,
    /// An AI agent over MCP.
    Mcp {
        /// The client's name, as it introduced itself ("Claude").
        client: String,
    },
    /// An editor plugin.
    Plugin {
        /// The plugin's name.
        name: String,
    },
    /// A command script (`oddity-editor run`).
    Script {
        /// The script file, as it was given.
        file: String,
    },
}

impl Source {
    /// Who to name after a history label ("Rename Goblin · Claude"), or
    /// `None` for your own changes, which need no name.
    pub fn who(&self) -> Option<Cow<'_, str>> {
        match self {
            Self::User => None,
            Self::Cli => Some(Cow::Borrowed("CLI")),
            Self::Mcp { client } => Some(Cow::Borrowed(client)),
            Self::Plugin { name } => Some(Cow::Borrowed(name)),
            // Split on both separators: a script path written on Windows
            // may be shown on Linux.
            Self::Script { file } => file.rsplit(['/', '\\']).next().map(Cow::Borrowed),
        }
    }

    /// The source as JSON, e.g. `{"kind": "user"}`.
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).expect("a Source is plain data and always serializes")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn sources_name_who_made_the_change() {
        assert_eq!(Source::User.who(), None);
        assert_eq!(Source::Cli.who().as_deref(), Some("CLI"));
        let script = Source::Script {
            file: r"tools\fix_walls.json".into(),
        };
        assert_eq!(script.who().as_deref(), Some("fix_walls.json"));
    }

    #[test]
    fn json_names_the_kind() {
        let agent = Source::Mcp {
            client: "Claude".into(),
        };
        assert_eq!(agent.to_json(), json!({"kind": "mcp", "client": "Claude"}));
        assert_eq!(Source::User.to_json(), json!({"kind": "user"}));
        assert_eq!(
            serde_json::from_value::<Source>(agent.to_json()).unwrap(),
            agent
        );
    }
}
