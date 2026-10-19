//! Scripts in the editor (`roadmap/scripting.md`, "The editor never runs
//! your code"): a script on a node is data, a [`ScriptInstance`] with its
//! saved fields as JSON, and what the fields are comes from the game's type
//! description ([`ScriptTypes`], read from `.oddity/types.json` after each
//! build).
//!
//! Commands, all undoable: `script.attach`, `script.remove`,
//! `script.set_enabled` and `script.set_field` (which merges while
//! scrubbing, as `component.set` does).

use std::{any::Any, borrow::Cow};

use bevy_ecs::reflect::ReflectComponent;
use bevy_ecs::{component::Component, entity::Entity, resource::Resource, world::World};
use bevy_reflect::{Reflect, ReflectDeserialize, ReflectSerialize, prelude::ReflectDefault};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{
    CommandError, EditorCommand, Invocation,
    level::require_in_level,
    registry::{
        ArgKind, ArgSpec, Args, CommandRegistry, CommandSpec, NamedCommand, entity_arg, string_arg,
    },
};

/// A script's saved fields, by name, as JSON.
#[derive(Reflect, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(transparent)]
#[reflect(opaque)]
#[reflect(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ScriptValues(pub Map<String, Value>);

/// One script on a node, in the editor.
#[derive(Reflect, Clone, Debug, PartialEq)]
pub struct ScriptInstance {
    /// The script's Rust type path, as the type description gives it.
    pub type_path: String,
    /// Whether it runs (its ☑ in the Inspector).
    pub enabled: bool,
    /// Its saved fields.
    pub values: ScriptValues,
}

/// A node's scripts, in the order they were added (which is the order they
/// run in, among scripts of the same `order`).
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[reflect(Component, Default)]
pub struct Scripts(pub Vec<ScriptInstance>);

impl Scripts {
    /// The script of type `type_path`.
    pub fn get(&self, type_path: &str) -> Option<&ScriptInstance> {
        self.0.iter().find(|script| script.type_path == type_path)
    }

    fn index(&self, type_path: &str) -> Option<usize> {
        self.0
            .iter()
            .position(|script| script.type_path == type_path)
    }
}

/// The game's scripts as its last good build described them
/// (`--oddity-describe`, kept in `.oddity/types.json`).
#[derive(Resource, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ScriptTypes {
    /// Every script type.
    #[serde(default)]
    pub scripts: Vec<ScriptType>,
    /// The game's own structs and enums its fields use, by type path.
    #[serde(default)]
    pub types: Map<String, Value>,
}

/// One script type.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ScriptType {
    /// Its Rust type path.
    pub type_path: String,
    /// Its name: the type's.
    pub name: String,
    /// Its `#[script(id = "...")]`.
    #[serde(default)]
    pub id: Option<String>,
    /// Its `order`.
    #[serde(default)]
    pub order: i32,
    /// The file it is in.
    #[serde(default)]
    pub file: Option<String>,
    /// Its saved fields, in declaration order.
    #[serde(default)]
    pub fields: Vec<ScriptField>,
}

/// One saved field of a script.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ScriptField {
    /// Its name.
    pub name: String,
    /// Its type: a short name for types the editor knows (`f32`, `Vec3`,
    /// `NodeRef`), a type path for the game's own.
    #[serde(rename = "type")]
    pub type_name: String,
    /// Its default value.
    #[serde(default)]
    pub default: Value,
}

impl ScriptTypes {
    /// The script a file holds, for dropping the file on a node: the one
    /// named like the file (`player.rs` holds `Player`), else its only one.
    pub fn in_file(&self, file: &std::path::Path) -> Option<&ScriptType> {
        let same = |script: &&ScriptType| {
            script
                .file
                .as_deref()
                .is_some_and(|path| std::path::Path::new(path) == file)
        };
        let scripts: Vec<&ScriptType> = self.scripts.iter().filter(same).collect();
        let stem = file
            .file_stem()
            .map(|stem| stem.to_string_lossy().replace('_', "").to_lowercase());
        let named = scripts
            .iter()
            .find(|script| Some(script.name.to_lowercase()) == stem)
            .copied();
        named.or(match scripts[..] {
            [only] => Some(only),
            _ => None,
        })
    }

    /// The script called `name`: its type path, its name, or its `id`.
    pub fn find(&self, name: &str) -> Option<&ScriptType> {
        self.scripts.iter().find(|script| {
            script.type_path == name || script.name == name || script.id.as_deref() == Some(name)
        })
    }
}

impl ScriptType {
    /// A new instance's values: every field at its default.
    pub fn defaults(&self) -> ScriptValues {
        ScriptValues(
            self.fields
                .iter()
                .map(|field| (field.name.clone(), field.default.clone()))
                .collect(),
        )
    }
}

/// Register the `script.*` commands.
pub(crate) fn register(registry: &mut CommandRegistry) {
    registry.register_named::<AttachScript>();
    registry.register_named::<RemoveScript>();
    registry.register_named::<SetScriptEnabled>();
    registry.register_named::<SetScriptField>();
}

const ENTITY: ArgSpec = ArgSpec {
    name: "entity",
    kind: ArgKind::Entity,
    required: true,
    description: "The node",
};
const SCRIPT: ArgSpec = ArgSpec {
    name: "script",
    kind: ArgKind::String,
    required: true,
    description: "The script: its name, type path or id",
};

/// The script `name` names, by the type description, else as attached.
fn script_type_path(world: &World, entity: Entity, name: &str) -> Result<String, CommandError> {
    if let Some(found) = world
        .get_resource::<ScriptTypes>()
        .and_then(|types| types.find(name))
    {
        return Ok(found.type_path.clone());
    }
    let attached = world
        .get::<Scripts>(entity)
        .and_then(|scripts| {
            scripts.0.iter().find(|script| {
                script.type_path == name || script.type_path.rsplit("::").next() == Some(name)
            })
        })
        .map(|script| script.type_path.clone());
    attached.ok_or_else(|| {
        CommandError::Invalid(format!(
            "no script `{name}`: build the game so the editor knows its scripts"
        ))
    })
}

fn short(type_path: &str) -> &str {
    type_path.rsplit("::").next().unwrap_or(type_path)
}

/// Run `change` on `entity`'s script `type_path`.
fn with_script<R>(
    world: &mut World,
    entity: Entity,
    type_path: &str,
    change: impl FnOnce(&mut ScriptInstance) -> R,
) -> Result<R, CommandError> {
    require_in_level(world, entity)?;
    let mut scripts = world
        .get_mut::<Scripts>(entity)
        .ok_or_else(|| missing(entity, type_path))?;
    let index = scripts
        .index(type_path)
        .ok_or_else(|| missing(entity, type_path))?;
    Ok(change(&mut scripts.0[index]))
}

fn missing(entity: Entity, type_path: &str) -> CommandError {
    CommandError::MissingComponent {
        entity,
        component: type_path.to_owned(),
    }
}

/// `script.attach`: add a script to a node, with its fields at their
/// defaults.
pub struct AttachScript {
    entity: Entity,
    instance: ScriptInstance,
    /// Whether this added the node's `Scripts`, which undo then removes.
    added_scripts: bool,
}

impl NamedCommand for AttachScript {
    const SPEC: CommandSpec = CommandSpec::edit(
        "script.attach",
        "Attach one of the game's scripts to a node, with its fields at their defaults.",
        &[ENTITY, SCRIPT],
    )
    .example(r#"{"entity": "Player", "script": "Mover"}"#);

    fn from_args(world: &mut World, args: &Args) -> Result<Self, CommandError> {
        let entity = entity_arg(args, "entity")?;
        let name = string_arg(args, "script")?;
        let script = world
            .get_resource::<ScriptTypes>()
            .and_then(|types| types.find(name))
            .ok_or_else(|| {
                CommandError::Invalid(format!(
                    "no script `{name}`: build the game so the editor knows its scripts"
                ))
            })?;
        Ok(Self {
            entity,
            instance: ScriptInstance {
                type_path: script.type_path.clone(),
                enabled: true,
                values: script.defaults(),
            },
            added_scripts: false,
        })
    }
}

impl EditorCommand for AttachScript {
    fn label(&self) -> Cow<'_, str> {
        format!("Attach {}", short(&self.instance.type_path)).into()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        require_in_level(world, self.entity)?;
        let mut node = world.entity_mut(self.entity);
        self.added_scripts = !node.contains::<Scripts>();
        let mut entry = node.entry::<Scripts>().or_default();
        let mut scripts = entry.get_mut();
        if scripts.get(&self.instance.type_path).is_some() {
            return Err(CommandError::AlreadyHasComponent {
                entity: self.entity,
                component: self.instance.type_path.clone(),
            });
        }
        scripts.0.push(self.instance.clone());
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        let mut node = world
            .get_entity_mut(self.entity)
            .map_err(|_| CommandError::NoEntity(self.entity))?;
        if self.added_scripts {
            node.remove::<Scripts>();
        } else if let Some(mut scripts) = node.get_mut::<Scripts>() {
            scripts
                .0
                .retain(|script| script.type_path != self.instance.type_path);
        }
        Ok(())
    }

    fn invocation(&self) -> Invocation {
        Invocation::new(
            Self::SPEC.id,
            json!({ "entity": self.entity.to_bits(), "script": self.instance.type_path }),
        )
    }
}

/// `script.remove`: take a script off a node.
pub struct RemoveScript {
    entity: Entity,
    type_path: String,
    /// Where it was and what it held, for undo.
    removed: Option<(usize, ScriptInstance)>,
    removed_scripts: bool,
}

impl NamedCommand for RemoveScript {
    const SPEC: CommandSpec = CommandSpec::edit(
        "script.remove",
        "Remove a script from a node.",
        &[ENTITY, SCRIPT],
    )
    .example(r#"{"entity": "Enemies/Goblin", "script": "Mover"}"#);

    fn from_args(world: &mut World, args: &Args) -> Result<Self, CommandError> {
        let entity = entity_arg(args, "entity")?;
        let type_path = script_type_path(world, entity, string_arg(args, "script")?)?;
        Ok(Self {
            entity,
            type_path,
            removed: None,
            removed_scripts: false,
        })
    }
}

impl EditorCommand for RemoveScript {
    fn label(&self) -> Cow<'_, str> {
        format!("Remove {}", short(&self.type_path)).into()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        require_in_level(world, self.entity)?;
        let mut node = world.entity_mut(self.entity);
        let mut scripts = node
            .get_mut::<Scripts>()
            .ok_or_else(|| missing(self.entity, &self.type_path))?;
        let index = scripts
            .index(&self.type_path)
            .ok_or_else(|| missing(self.entity, &self.type_path))?;
        let script = scripts.0.remove(index);
        self.removed_scripts = scripts.0.is_empty();
        self.removed = Some((index, script));
        if self.removed_scripts {
            node.remove::<Scripts>();
        }
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        let Some((index, script)) = self.removed.clone() else {
            return Ok(());
        };
        let mut node = world
            .get_entity_mut(self.entity)
            .map_err(|_| CommandError::NoEntity(self.entity))?;
        let mut scripts = node.entry::<Scripts>().or_default();
        let scripts = &mut scripts.get_mut().0;
        scripts.insert(index.min(scripts.len()), script);
        Ok(())
    }

    fn invocation(&self) -> Invocation {
        Invocation::new(
            Self::SPEC.id,
            json!({ "entity": self.entity.to_bits(), "script": self.type_path }),
        )
    }
}

/// `script.set_enabled`: switch a script on or off (its ☑).
pub struct SetScriptEnabled {
    entity: Entity,
    type_path: String,
    enabled: bool,
    was: Option<bool>,
}

impl NamedCommand for SetScriptEnabled {
    const SPEC: CommandSpec = CommandSpec::edit(
        "script.set_enabled",
        "Switch a script on a node on or off: a switched-off script doesn't run.",
        &[
            ENTITY,
            SCRIPT,
            ArgSpec {
                name: "enabled",
                kind: ArgKind::Json,
                required: true,
                description: "true to run it, false not to",
            },
        ],
    )
    .example(r#"{"entity": "Enemies/Goblin", "script": "Mover", "enabled": false}"#);

    fn from_args(world: &mut World, args: &Args) -> Result<Self, CommandError> {
        let entity = entity_arg(args, "entity")?;
        let type_path = script_type_path(world, entity, string_arg(args, "script")?)?;
        let enabled = args
            .get("enabled")
            .and_then(Value::as_bool)
            .ok_or_else(|| CommandError::BadArgument {
                name: "enabled".into(),
                reason: "expected true or false".into(),
            })?;
        Ok(Self {
            entity,
            type_path,
            enabled,
            was: None,
        })
    }
}

impl EditorCommand for SetScriptEnabled {
    fn label(&self) -> Cow<'_, str> {
        let verb = if self.enabled { "Enable" } else { "Disable" };
        format!("{verb} {}", short(&self.type_path)).into()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        let enabled = self.enabled;
        let was = with_script(world, self.entity, &self.type_path, |script| {
            std::mem::replace(&mut script.enabled, enabled)
        })?;
        self.was.get_or_insert(was);
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        let was = self.was.unwrap_or(!self.enabled);
        with_script(world, self.entity, &self.type_path, |script| {
            script.enabled = was;
        })
    }

    fn invocation(&self) -> Invocation {
        Invocation::new(
            Self::SPEC.id,
            json!({
                "entity": self.entity.to_bits(),
                "script": self.type_path,
                "enabled": self.enabled,
            }),
        )
    }
}

/// `script.set_field`: change one of a script's saved fields.
pub struct SetScriptField {
    entity: Entity,
    type_path: String,
    field: String,
    value: Value,
    /// The value before, or `None` if the field wasn't set.
    was: Option<Option<Value>>,
}

impl SetScriptField {
    /// Set `field` (a name, or a dotted path into a struct: `stats.level`)
    /// of `entity`'s script `type_path` to `value`.
    pub fn new(
        entity: Entity,
        type_path: impl Into<String>,
        field: impl Into<String>,
        value: Value,
    ) -> Self {
        Self {
            entity,
            type_path: type_path.into(),
            field: field.into(),
            value,
            was: None,
        }
    }
}

impl NamedCommand for SetScriptField {
    const SPEC: CommandSpec = CommandSpec::edit(
        "script.set_field",
        "Set a saved field of a script on a node. Nested fields take a dotted path (stats.level).",
        &[
            ENTITY,
            SCRIPT,
            ArgSpec {
                name: "field",
                kind: ArgKind::String,
                required: true,
                description: "The field's name, or a dotted path into a struct",
            },
            ArgSpec {
                name: "value",
                kind: ArgKind::Json,
                required: true,
                description: "The new value, as the level file writes it",
            },
        ],
    )
    .example(r#"{"entity": "Enemies/Goblin", "script": "Mover", "field": "speed", "value": 7.5}"#);

    fn from_args(world: &mut World, args: &Args) -> Result<Self, CommandError> {
        let entity = entity_arg(args, "entity")?;
        let type_path = script_type_path(world, entity, string_arg(args, "script")?)?;
        let field = string_arg(args, "field")?.to_owned();
        let value = args
            .get("value")
            .cloned()
            .ok_or_else(|| CommandError::MissingArgument("value".into()))?;
        Ok(Self::new(entity, type_path, field, value))
    }
}

/// The value at `path` in `values`, creating objects on the way.
fn slot<'a>(values: &'a mut Map<String, Value>, path: &str) -> Result<&'a mut Value, String> {
    let mut steps = path.split('.');
    let first = steps.next().unwrap_or_default();
    let mut at = values.entry(first).or_insert(Value::Null);
    for step in steps {
        if at.is_null() {
            *at = Value::Object(Map::new());
        }
        at = at
            .as_object_mut()
            .ok_or_else(|| format!("`{path}` goes inside a value that isn't a struct"))?
            .entry(step)
            .or_insert(Value::Null);
    }
    Ok(at)
}

fn read<'a>(values: &'a Map<String, Value>, path: &str) -> Option<&'a Value> {
    let mut steps = path.split('.');
    let mut at = values.get(steps.next()?)?;
    for step in steps {
        at = at.get(step)?;
    }
    Some(at)
}

impl EditorCommand for SetScriptField {
    fn label(&self) -> Cow<'_, str> {
        format!("Set {}.{}", short(&self.type_path), self.field).into()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        let (field, value) = (self.field.clone(), self.value.clone());
        let was = with_script(world, self.entity, &self.type_path, |script| {
            let was = read(&script.values.0, &field).cloned();
            slot(&mut script.values.0, &field).map(|slot| *slot = value)?;
            Ok::<_, String>(was)
        })?
        .map_err(|reason| CommandError::BadPath {
            path: self.field.clone(),
            reason,
        })?;
        self.was.get_or_insert(was);
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        let Some(was) = self.was.clone() else {
            return Ok(());
        };
        let field = self.field.clone();
        with_script(world, self.entity, &self.type_path, |script| match was {
            Some(value) => {
                if let Ok(slot) = slot(&mut script.values.0, &field) {
                    *slot = value;
                }
            }
            None => {
                remove(&mut script.values.0, &field);
            }
        })
    }

    fn invocation(&self) -> Invocation {
        Invocation::new(
            Self::SPEC.id,
            json!({
                "entity": self.entity.to_bits(),
                "script": self.type_path,
                "field": self.field,
                "value": self.value,
            }),
        )
    }

    /// Scrubbing the same field: the newest value wins, and undo goes back
    /// to the value before the first.
    fn merge(&mut self, next: &dyn EditorCommand) -> bool {
        let Some(next) = (next as &dyn Any).downcast_ref::<SetScriptField>() else {
            return false;
        };
        if (next.entity, &next.type_path, &next.field)
            != (self.entity, &self.type_path, &self.field)
        {
            return false;
        }
        self.value = next.value.clone();
        true
    }
}

/// Take away a field that wasn't there before.
fn remove(values: &mut Map<String, Value>, path: &str) {
    match path.rsplit_once('.') {
        None => {
            values.remove(path);
        }
        Some((parent, last)) => {
            if let Ok(Value::Object(fields)) = slot(values, parent) {
                fields.remove(last);
            }
        }
    }
}
