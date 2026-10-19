//! Commands as MCP tools: names, JSON schemas and hints, all from each
//! command's `CommandSpec`. Nothing here lists commands, so a command an
//! extension registers is a tool with no change to this crate.

use oddity_core::{ArgKind, CommandKind, CommandRegistry, CommandSpec};
use serde_json::{Map, Value, json};

/// The tool name for a command id. Clients such as Claude allow only
/// letters, digits, `_` and `-` in tool names, so `entity.spawn` becomes
/// `entity_spawn`.
pub fn tool_name(id: &str) -> String {
    id.replace('.', "_")
}

/// The command a tool name stands for.
pub fn command_for<'a>(registry: &'a CommandRegistry, tool: &str) -> Option<&'a CommandSpec> {
    registry.specs().find(|spec| tool_name(spec.id) == tool)
}

/// The JSON Schema of a command's arguments.
pub fn input_schema(spec: &CommandSpec) -> Map<String, Value> {
    let properties: Map<String, Value> = spec
        .args
        .iter()
        .map(|arg| {
            let mut schema = match arg.kind {
                ArgKind::Entity => json!({ "type": "integer", "minimum": 0 }),
                ArgKind::Entities => {
                    json!({ "type": "array", "items": { "type": "integer", "minimum": 0 } })
                }
                ArgKind::String => json!({ "type": "string" }),
                // Any JSON: the target field's own type decides.
                ArgKind::Json => json!({}),
            };
            schema["description"] = json!(arg.description);
            (arg.name.to_owned(), schema)
        })
        .collect();
    let required: Vec<&str> = spec
        .args
        .iter()
        .filter(|arg| arg.required)
        .map(|arg| arg.name)
        .collect();
    let mut schema = Map::new();
    schema.insert("type".into(), json!("object"));
    schema.insert("properties".into(), Value::Object(properties));
    if !required.is_empty() {
        schema.insert("required".into(), json!(required));
    }
    schema
}

/// The tool's description: the command's, plus its example if it has one.
/// Examples write entities as node paths, which tools don't take, so the
/// description says so.
pub fn description(spec: &CommandSpec) -> String {
    match spec.example {
        Some(example) => format!(
            "{} Example (entities shown as node paths; pass their ids): {example}",
            spec.description
        ),
        None => spec.description.to_owned(),
    }
}

/// How a tool hints at what it does, from its command's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hints {
    /// Only reads.
    pub read_only: bool,
    /// May overwrite or delete something undo can't bring back.
    pub destructive: bool,
}

/// Queries are read-only. Edits and settings can be undone, so they aren't
/// destructive; actions are when their spec says so.
pub fn hints(spec: &CommandSpec) -> Hints {
    match spec.kind {
        CommandKind::Query => Hints {
            read_only: true,
            destructive: false,
        },
        CommandKind::Edit | CommandKind::Setting => Hints {
            read_only: false,
            destructive: false,
        },
        CommandKind::Action => Hints {
            read_only: false,
            destructive: spec.destructive,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_names_have_no_dots_and_map_back() {
        let registry = CommandRegistry::with_builtins();
        assert_eq!(tool_name("history.group_begin"), "history_group_begin");
        for spec in registry.specs() {
            let found = command_for(&registry, &tool_name(spec.id)).map(|s| s.id);
            assert_eq!(found, Some(spec.id), "{} doesn't map back", spec.id);
        }
    }

    #[test]
    fn schemas_follow_the_arguments() {
        let registry = CommandRegistry::with_builtins();
        let spawn = registry.spec("entity.spawn").unwrap();
        let schema = Value::Object(input_schema(spawn));
        assert_eq!(schema["properties"]["name"]["type"], "string");
        assert_eq!(schema["properties"]["parent"]["type"], "integer");
        assert_eq!(schema["required"], json!(["name"]));
    }

    #[test]
    fn hints_follow_the_kind() {
        let registry = CommandRegistry::with_builtins();
        let hint = |id| hints(registry.spec(id).unwrap());
        assert!(hint("level.tree").read_only);
        assert!(!hint("entity.despawn").destructive, "edits can be undone");
        assert!(!hint("entity.spawn").read_only);
    }
}
