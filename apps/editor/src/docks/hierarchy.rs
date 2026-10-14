//! The Hierarchy dock: the level's node tree (Unity's Hierarchy, Godot's
//! Scene dock).
//!
//! Each frame the level under [`LevelRoot`] is read into the dock's
//! [`TreeView`]. The tree widget only rebuilds rows when that list actually
//! changes, so this stays cheap, and it is always correct after undo, redo,
//! MCP or plugin edits without any of them having to notify the dock.
//!
//! Clicks change the editor's [`Selection`]; adding, deleting and hiding
//! nodes go through undoable commands. Typing in "Filter Nodes" shows only
//! matching nodes and the parents that lead to them, as in Godot.

use bevy::{platform::collections::HashSet, prelude::*, reflect::TypePath, ui_widgets::Activate};
use oddity_core::{
    EditorWorldExt, LevelRoot, Selection,
    commands::{Batch, DespawnEntity, SetField, SpawnEntity},
    is_in_level, level_root,
};
use oddity_icons::Icon;
use oddity_widgets::{
    button::tool_button,
    field::{SearchText, search_field},
    tree::{SelectMode, TreeButton, TreeEvent, TreeItem, TreeView},
};

use crate::level::NodeIcon;

pub struct HierarchyPlugin;

impl Plugin for HierarchyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HierarchyState>()
            .add_systems(Update, (handle_tree_events, refresh_tree).chain());
    }
}

/// Which nodes are collapsed. Everything starts expanded, as in Godot.
#[derive(Resource, Default)]
struct HierarchyState {
    collapsed: HashSet<Entity>,
}

/// Marks the Hierarchy's tree, so its events aren't confused with other trees.
#[derive(Component, Default, Clone)]
struct HierarchyTree;

/// Marks the "Filter Nodes" field.
#[derive(Component, Default, Clone)]
struct HierarchyFilter;

/// Id of the visibility button on a row.
const VISIBILITY_BUTTON: u32 = 0;

/// The dock page: Godot's toolbar (add, filter, delete) above the tree.
pub fn hierarchy_page() -> impl Scene {
    bsn! {
        Node { flex_grow: 1.0, flex_direction: FlexDirection::Column, min_height: px(0) }
        Children [
            Node {
                flex_shrink: 0.0,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(4),
                padding: UiRect::axes(px(6), px(6)),
            }
            Children [
                @tool_button(Icon::godot("Add"))
                on(|_: On<Activate>, mut commands: Commands| { commands.queue(add_node); })
                --
                @search_field("Filter Nodes")
                HierarchyFilter
                Node { flex_grow: 1.0 }
                --
                @tool_button(Icon::godot("Remove"))
                on(|_: On<Activate>, mut commands: Commands| { commands.queue(delete_selection); })
            ]
            --
            Node {
                flex_grow: 1.0,
                min_height: px(0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::top(px(2)),
                overflow: Overflow::scroll_y(),
            }
            TreeView
            HierarchyTree
        ]
    }
}

/// What the tree needs to know about each node.
type NodeData<'a> = (
    &'a Name,
    Option<&'a Children>,
    Option<&'a NodeIcon>,
    Option<&'a Visibility>,
);

/// Rebuild the tree's rows from the level, skipping collapsed subtrees.
fn refresh_tree(
    roots: Query<Entity, With<LevelRoot>>,
    nodes: Query<NodeData>,
    selection: Res<Selection>,
    state: Res<HierarchyState>,
    filters: Query<&SearchText, With<HierarchyFilter>>,
    mut trees: Query<&mut TreeView, With<HierarchyTree>>,
) {
    let filter = filters.iter().next().cloned().unwrap_or_default();
    let mut items = Vec::new();
    for root in &roots {
        push_subtree(root, 0, &nodes, &selection, &state, &filter, &mut items);
    }
    for mut tree in &mut trees {
        if tree.items != items {
            tree.items.clone_from(&items);
        }
    }
}

/// Add `entity` and its visible descendants to `items`, depth-first.
/// Returns whether anything was added, so a parent with no matching
/// descendant can be left out while filtering.
fn push_subtree(
    entity: Entity,
    depth: u16,
    nodes: &Query<NodeData>,
    selection: &Selection,
    state: &HierarchyState,
    filter: &SearchText,
    items: &mut Vec<TreeItem>,
) -> bool {
    // Deleted nodes are disabled and drop out of queries here.
    let Ok((name, children, icon, visibility)) = nodes.get(entity) else {
        return false;
    };
    let children: Vec<Entity> = children
        .map(|c| c.iter().filter(|child| nodes.contains(*child)).collect())
        .unwrap_or_default();
    let filtering = !filter.is_empty();
    // While filtering, everything that leads to a match is shown expanded.
    let expanded = filtering || !state.collapsed.contains(&entity);

    let index = items.len();
    items.push(TreeItem {
        key: entity.to_bits(),
        depth,
        label: name.as_str().to_owned(),
        icon: Some(icon.cloned().unwrap_or_default().0),
        has_children: !children.is_empty(),
        expanded,
        selected: selection.contains(entity),
        dimmed: false,
        buttons: visibility
            .map(|v| {
                let icon = if *v == Visibility::Hidden {
                    "GuiVisibilityHidden"
                } else {
                    "GuiVisibilityVisible"
                };
                vec![TreeButton {
                    id: VISIBILITY_BUTTON,
                    icon: Icon::godot(icon),
                }]
            })
            .unwrap_or_default(),
    });

    let mut any_child = false;
    if expanded {
        for child in children {
            any_child |= push_subtree(child, depth + 1, nodes, selection, state, filter, items);
        }
    }
    let keep = filter.matches(name.as_str()) || any_child;
    if !keep {
        items.truncate(index);
    }
    keep
}

fn handle_tree_events(
    mut events: MessageReader<TreeEvent>,
    trees: Query<&TreeView, With<HierarchyTree>>,
    mut selection: ResMut<Selection>,
    mut state: ResMut<HierarchyState>,
    mut commands: Commands,
) {
    for event in events.read() {
        match *event {
            TreeEvent::Select { tree, key, mode } => {
                let Ok(view) = trees.get(tree) else { continue };
                let entity = Entity::from_bits(key);
                match mode {
                    SelectMode::Replace => selection.select(entity),
                    SelectMode::Toggle => selection.toggle(entity),
                    SelectMode::Extend => select_range(&mut selection, view, entity),
                }
            }
            TreeEvent::Toggle { tree, key } => {
                if trees.contains(tree) {
                    let entity = Entity::from_bits(key);
                    if !state.collapsed.remove(&entity) {
                        state.collapsed.insert(entity);
                    }
                }
            }
            TreeEvent::Button { tree, key, id } => {
                if trees.contains(tree) && id == VISIBILITY_BUTTON {
                    let entity = Entity::from_bits(key);
                    commands.queue(move |world: &mut World| toggle_visibility(world, entity));
                }
            }
            TreeEvent::Activate { tree, .. } => {
                if trees.contains(tree) {
                    info!("Rename in place arrives with the inspector (Phase 3)");
                }
            }
        }
    }
}

/// Show or hide a node, as Godot's eye button does. One undo step.
fn toggle_visibility(world: &mut World, entity: Entity) {
    let Some(current) = world.get::<Visibility>(entity).copied() else {
        return;
    };
    let next = if current == Visibility::Hidden {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    let name = world
        .get::<Name>(entity)
        .map_or_else(|| "node".to_owned(), |n| n.as_str().to_owned());
    let verb = if next == Visibility::Hidden {
        "Hide"
    } else {
        "Show"
    };
    // A batch of one, only to give the History dock a readable label.
    let mut step = Batch::new(format!("{verb} {name}"));
    step.push(Box::new(SetField::new(
        entity,
        Visibility::type_path(),
        "",
        Box::new(next),
    )));
    if let Err(error) = world.execute(step) {
        warn!("Could not change visibility: {error}");
    }
}

/// Shift+click: select every visible row from the current primary to `to`.
fn select_range(selection: &mut Selection, view: &TreeView, to: Entity) {
    let index_of = |entity: Entity| view.items.iter().position(|i| i.key == entity.to_bits());
    let (Some(from), Some(to_index)) = (selection.primary().and_then(index_of), index_of(to))
    else {
        selection.select(to);
        return;
    };
    let (start, end) = (from.min(to_index), from.max(to_index));
    for item in &view.items[start..=end] {
        selection.add(Entity::from_bits(item.key));
    }
    // The clicked row becomes the primary selection, as in Godot.
    selection.add(to);
}

/// Add a node under the selected node (or the level root) and select it.
/// One undo step.
pub fn add_node(world: &mut World) {
    let Some(root) = level_root(world) else {
        warn!("No level is open");
        return;
    };
    let parent = world
        .resource::<Selection>()
        .primary()
        .filter(|e| is_in_level(world, *e))
        .unwrap_or(root);

    let name = unique_child_name(world, parent, "Node");
    let spawn = SpawnEntity::new(world, name).child_of(parent);
    let node = spawn.entity();
    world.entity_mut(node).insert(NodeIcon::default());
    match world.execute(spawn) {
        Ok(_) => {
            world
                .resource_mut::<HierarchyState>()
                .collapsed
                .remove(&parent);
            world.resource_mut::<Selection>().select(node);
        }
        Err(error) => warn!("Could not add a node: {error}"),
    }
}

/// Delete the selected nodes (never the level root). One undo step.
pub fn delete_selection(world: &mut World) {
    let selected: Vec<Entity> = world.resource::<Selection>().entities().to_vec();
    let mut targets: Vec<Entity> = selected
        .iter()
        .copied()
        .filter(|e| is_in_level(world, *e) && world.get::<LevelRoot>(*e).is_none())
        .collect();
    // Deleting a parent already deletes its children; skip selected descendants.
    let all = targets.clone();
    targets.retain(|e| !has_ancestor_in(world, *e, &all));
    if targets.is_empty() {
        return;
    }

    let label = if targets.len() == 1 {
        world.get::<Name>(targets[0]).map_or_else(
            || "Delete node".to_owned(),
            |n| format!("Delete {}", n.as_str()),
        )
    } else {
        format!("Delete {} nodes", targets.len())
    };
    let mut batch = Batch::new(label);
    for entity in targets {
        batch.push(Box::new(DespawnEntity::new(world, entity)));
    }
    if let Err(error) = world.execute(batch) {
        warn!("Could not delete: {error}");
    }
}

fn has_ancestor_in(world: &World, entity: Entity, set: &[Entity]) -> bool {
    let mut current = world.get::<ChildOf>(entity).map(ChildOf::parent);
    while let Some(parent) = current {
        if set.contains(&parent) {
            return true;
        }
        current = world.get::<ChildOf>(parent).map(ChildOf::parent);
    }
    false
}

/// `base`, or `base2`, `base3`... so siblings keep distinct names, which
/// `get_node("../Name")` paths rely on.
fn unique_child_name(world: &World, parent: Entity, base: &str) -> String {
    let taken: HashSet<String> = world
        .get::<Children>(parent)
        .into_iter()
        .flat_map(|children| children.iter())
        .filter(|child| {
            world
                .get::<bevy::ecs::entity_disabling::Disabled>(*child)
                .is_none()
        })
        .filter_map(|child| world.get::<Name>(child).map(|n| n.as_str().to_owned()))
        .collect();
    if !taken.contains(base) {
        return base.to_owned();
    }
    (2..)
        .map(|n| format!("{base}{n}"))
        .find(|name| !taken.contains(name))
        .expect("an unused name always exists")
}

#[cfg(test)]
mod tests {
    use bevy::ecs::entity_disabling::Disabled;
    use oddity_core::headless_app;

    use super::*;

    /// A level root with one child, `Enemies`, which is selected.
    fn level() -> (App, Entity, Entity) {
        let mut app = headless_app();
        app.init_resource::<HierarchyState>();
        let world = app.world_mut();
        let root = world.spawn((Name::new("Main"), LevelRoot)).id();
        let enemies = world.spawn((Name::new("Enemies"), ChildOf(root))).id();
        world.resource_mut::<Selection>().select(enemies);
        (app, root, enemies)
    }

    fn names_under(world: &World, parent: Entity) -> Vec<String> {
        world
            .get::<Children>(parent)
            .into_iter()
            .flat_map(|c| c.iter())
            .filter(|e| world.get::<Disabled>(*e).is_none())
            .filter_map(|e| world.get::<Name>(e).map(|n| n.as_str().to_owned()))
            .collect()
    }

    #[test]
    fn add_node_goes_under_the_selection_and_is_selected() {
        let (mut app, _, enemies) = level();
        let world = app.world_mut();
        add_node(world);
        let added = world.resource::<Selection>().primary().unwrap();
        assert_eq!(world.get::<Name>(added).unwrap().as_str(), "Node");

        // A second node next to it gets a distinct name.
        world.resource_mut::<Selection>().select(enemies);
        add_node(world);
        assert_eq!(names_under(world, enemies), ["Node", "Node2"]);
    }

    #[test]
    fn deleting_and_undoing_brings_the_node_back() {
        let (mut app, root, _) = level();
        let world = app.world_mut();
        delete_selection(world);
        assert!(names_under(world, root).is_empty());
        world.undo().unwrap();
        assert_eq!(names_under(world, root), ["Enemies"]);
    }

    #[test]
    fn the_level_root_cannot_be_deleted() {
        let (mut app, root, _) = level();
        let world = app.world_mut();
        world.resource_mut::<Selection>().select(root);
        delete_selection(world);
        assert!(world.get::<Disabled>(root).is_none());
    }
}
