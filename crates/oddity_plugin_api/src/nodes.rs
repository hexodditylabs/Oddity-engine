//! Node types: presets of Bevy components, each with a Godot icon and a
//! category (`roadmap/scripting.md`, "Built-in nodes"). Add Node lists them;
//! creating one runs the `node.create` command, so it is one undo step and
//! the CLI, MCP and scripts can create nodes too.
//!
//! ```ignore
//! app.register_node(
//!     NodeType::new::<(Transform, Visibility)>("Node3D", Icon::godot("Node3D")).category("3D"),
//! );
//! ```
//!
//! The first registered type also registers `node.create`, the
//! `node_types.list` query and the plain `Node` type, so they exist wherever
//! node types do, in every mode.

use std::{borrow::Cow, sync::Arc};

use bevy::{ecs::world::EntityWorldMut, platform::collections::HashSet, prelude::*};
use oddity_core::{
    ArgKind, ArgSpec, Args, CommandError, CommandRegistry, CommandSpec, EditorCommand, Invocation,
    NodeKind,
    commands::SpawnEntity,
    level_root,
    registry::{opt_entity_arg, opt_string_arg, string_arg},
};
use oddity_icons::Icon;
use serde_json::{Value, json};

/// The plain node: a name and nothing else, as in Godot.
pub const PLAIN_NODE: &str = "Node";

/// Puts a type's components on a new node.
type InsertFn = Arc<dyn Fn(&mut EntityWorldMut) + Send + Sync>;

/// A kind of node Add Node offers: its name, icon, category, and the
/// components a new one starts with.
#[derive(Clone)]
pub struct NodeType {
    /// The name, as in Godot ("Camera3D"). Also the new node's first name
    /// and its [`NodeKind`].
    pub name: Cow<'static, str>,
    /// Shown in Add Node, the Hierarchy and the Inspector.
    pub icon: Icon,
    /// Where Add Node lists it ("3D", "Lights").
    pub category: Cow<'static, str>,
    insert: InsertFn,
}

impl NodeType {
    /// A type whose new nodes start with `B`'s default: one component, or
    /// a tuple of them. Category "Other" until [`NodeType::category`].
    pub fn new<B: Bundle + Default>(name: impl Into<Cow<'static, str>>, icon: Icon) -> Self {
        Self::from_fn(name, icon, B::default)
    }

    /// A type whose new nodes start with what `make` returns, for presets
    /// that aren't all default values.
    pub fn from_fn<B: Bundle>(
        name: impl Into<Cow<'static, str>>,
        icon: Icon,
        make: impl Fn() -> B + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            icon,
            category: Cow::Borrowed("Other"),
            insert: Arc::new(move |entity| {
                entity.insert(make());
            }),
        }
    }

    /// Where Add Node lists it.
    pub fn category(mut self, category: impl Into<Cow<'static, str>>) -> Self {
        self.category = category.into();
        self
    }
}

impl std::fmt::Debug for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeType")
            .field("name", &self.name)
            .field("icon", &self.icon)
            .field("category", &self.category)
            .finish_non_exhaustive()
    }
}

/// Every registered node type, in the order they were registered.
#[derive(Resource, Debug)]
pub struct NodeTypes {
    types: Vec<NodeType>,
}

impl Default for NodeTypes {
    fn default() -> Self {
        Self {
            types: vec![NodeType::new::<()>(PLAIN_NODE, Icon::godot("Node")).category("Basic")],
        }
    }
}

impl NodeTypes {
    /// Add a type, replacing one with the same name.
    pub fn add(&mut self, node_type: NodeType) {
        match self.types.iter_mut().find(|t| t.name == node_type.name) {
            Some(existing) => *existing = node_type,
            None => self.types.push(node_type),
        }
    }

    /// The type called `name`.
    pub fn get(&self, name: &str) -> Option<&NodeType> {
        self.types.iter().find(|t| t.name == name)
    }

    /// Every type, in the order they were registered.
    pub fn iter(&self) -> impl Iterator<Item = &NodeType> {
        self.types.iter()
    }

    /// The types grouped by category, categories in the order they first
    /// appear, as Add Node lists them.
    pub fn by_category(&self) -> Vec<(&str, Vec<&NodeType>)> {
        let mut groups: Vec<(&str, Vec<&NodeType>)> = Vec::new();
        for node_type in &self.types {
            match groups.iter_mut().find(|(c, _)| *c == node_type.category) {
                Some((_, types)) => types.push(node_type),
                None => groups.push((&node_type.category, vec![node_type])),
            }
        }
        groups
    }

    /// The icon for a node of `kind`. Kinds nobody registered, such as
    /// those of a feature that is turned off, fall back to Godot's icon of
    /// that name, then to the plain node's.
    pub fn icon_for(&self, kind: Option<&NodeKind>) -> Icon {
        let kind = kind.map_or(PLAIN_NODE, |kind| kind.0.as_str());
        if let Some(node_type) = self.get(kind) {
            return node_type.icon.clone();
        }
        let godot = Icon::godot(kind.to_owned());
        if godot.exists() {
            godot
        } else {
            Icon::godot("Node")
        }
    }
}

/// Register the node registry's command and query, once.
pub(crate) fn register(app: &mut App) {
    let mut registry = app
        .world_mut()
        .get_resource_or_insert_with(CommandRegistry::with_builtins);
    registry.register(
        CommandSpec::edit(
            "node.create",
            "Create a node of a registered type (see node_types.list), at the top of the level or under a parent. Returns {\"entity\": id}.",
            &[
                ArgSpec {
                    name: "type",
                    kind: ArgKind::String,
                    required: true,
                    description: "The node type's name, e.g. \"Camera3D\"",
                },
                ArgSpec {
                    name: "name",
                    kind: ArgKind::String,
                    required: false,
                    description: "The node's name; omit for the type's name, made unique among its siblings",
                },
                ArgSpec {
                    name: "parent",
                    kind: ArgKind::Entity,
                    required: false,
                    description: "Parent entity; omit to put it at the top of the level",
                },
            ],
        )
        .example(r#"{"type": "Node", "parent": "Enemies"}"#),
        build_create_node,
    );
    registry.register_query(
        CommandSpec::query(
            "node_types.list",
            "Every node type node.create can make: {\"node_types\": [{name, category, icon}]}.",
            &[],
        ),
        list_node_types,
    );
}

fn build_create_node(
    world: &mut World,
    args: &Args,
) -> Result<Box<dyn EditorCommand>, CommandError> {
    let kind = string_arg(args, "type")?;
    let node_type = world
        .get_resource::<NodeTypes>()
        .and_then(|types| types.get(kind))
        .cloned()
        .ok_or_else(|| CommandError::BadArgument {
            name: "type".into(),
            reason: format!("no node type `{kind}`; node_types.list has them all"),
        })?;
    let parent = opt_entity_arg(args, "parent")?;
    let name = match opt_string_arg(args, "name")? {
        Some(name) => name.to_owned(),
        None => {
            let siblings_of = match parent {
                Some(parent) => Some(parent),
                None => level_root(world),
            };
            unique_name(world, siblings_of, &node_type.name)
        }
    };

    let mut spawn = SpawnEntity::new(world, name.clone());
    if let Some(parent) = parent {
        spawn = spawn.child_of(parent);
    }
    // The node exists, disabled, from here on; its components go on now so
    // that undo and redo keep the very same values.
    let mut entity = world.entity_mut(spawn.entity());
    (node_type.insert)(&mut entity);
    entity.insert(NodeKind(node_type.name.to_string()));
    Ok(Box::new(CreateNode {
        spawn,
        kind: node_type.name.to_string(),
        name,
        parent,
    }))
}

/// `base`, or `base2`, `base3`, ... so siblings keep distinct names, which
/// node paths rely on.
fn unique_name(world: &World, parent: Option<Entity>, base: &str) -> String {
    let taken: HashSet<&str> = parent
        .and_then(|parent| world.get::<Children>(parent))
        .into_iter()
        .flat_map(|children| children.iter())
        .filter(|child| {
            world
                .get::<bevy::ecs::entity_disabling::Disabled>(*child)
                .is_none()
        })
        .filter_map(|child| world.get::<Name>(child).map(Name::as_str))
        .collect();
    if !taken.contains(base) {
        return base.to_owned();
    }
    (2..)
        .map(|n| format!("{base}{n}"))
        .find(|name| !taken.contains(name.as_str()))
        .expect("an unused name always exists")
}

fn list_node_types(world: &World, _args: &Args) -> Result<Value, CommandError> {
    let types: Vec<Value> = world
        .get_resource::<NodeTypes>()
        .into_iter()
        .flat_map(NodeTypes::iter)
        .map(|node_type| {
            json!({
                "name": node_type.name,
                "category": node_type.category,
                "icon": node_type.icon.name(),
            })
        })
        .collect();
    Ok(json!({ "node_types": types }))
}

/// `node.create`: a spawn whose node starts with its type's components.
struct CreateNode {
    spawn: SpawnEntity,
    kind: String,
    name: String,
    parent: Option<Entity>,
}

impl EditorCommand for CreateNode {
    fn label(&self) -> Cow<'_, str> {
        self.spawn.label()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        self.spawn.apply(world)
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        self.spawn.undo(world)
    }

    fn invocation(&self) -> Invocation {
        let mut args = json!({ "type": self.kind, "name": self.name });
        if let Some(parent) = self.parent {
            args["parent"] = json!(parent.to_bits());
        }
        Invocation::new("node.create", args)
    }

    fn output(&self) -> Value {
        self.spawn.output()
    }

    fn discard(&mut self, world: &mut World, applied: bool) {
        self.spawn.discard(world, applied);
    }
}
