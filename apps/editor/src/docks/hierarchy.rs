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
//!
//! The "+" button (and Ctrl+A) opens Add Node: every registered node type,
//! by category, with its icon. Picking one runs `node.create`.

use bevy::{
    feathers::{
        controls::{FeathersMenu, FeathersMenuDivider, FeathersMenuItem, FeathersMenuPopup},
        display::caption,
    },
    input_focus::tab_navigation::NavAction,
    picking::Pickable,
    platform::collections::HashSet,
    prelude::*,
    reflect::TypePath,
    text::FontSourceTemplate,
    ui_widgets::{Activate, MenuAction, MenuEvent},
};
use oddity_core::{
    EditorWorldExt, LevelRoot, NodeKind, Selection,
    commands::{Batch, DespawnEntity, SetField},
    is_in_level, level_root,
    scripts::ScriptTypes,
};
use oddity_icons::{Icon, UiIcon};
use oddity_plugin_api::{DockSlot, DockTab, EditorAppExt, EditorMode, NodeType, NodeTypes};
use oddity_widgets::{
    button::{TOOL_BUTTON_SIZE, tool_button},
    field::{SearchText, search_field},
    fonts::paths,
    tree::{SelectMode, TreeButton, TreeEvent, TreeItem, TreeView},
};
use serde_json::json;

use super::{DimText, FIRST_TAB};
use crate::actions::run_command;

/// The Hierarchy tab, and the systems that keep it in step with the level.
pub struct HierarchyPlugin;

impl Plugin for HierarchyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HierarchyState>().add_systems(
            Update,
            (
                (handle_tree_events, refresh_tree).chain(),
                fill_add_node_menu,
            ),
        );
        if app.editor_mode() == EditorMode::Window {
            app.add_dock_tab(
                DockTab::new(DockSlot::Scene, "Hierarchy", hierarchy_page).order(FIRST_TAB),
            );
        }
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
fn hierarchy_page() -> impl Scene {
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
                @FeathersMenu
                Node { height: px(TOOL_BUTTON_SIZE) }
                Children [
                    @tool_button(Icon::godot("Add"))
                    AddNodeButton
                    on(|activate: On<Activate>, mut commands: Commands| {
                        commands.trigger(MenuEvent {
                            source: activate.entity,
                            action: MenuAction::Toggle,
                        });
                    })
                    --
                    @FeathersMenuPopup
                    AddNodeMenu
                ]
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

/// The "+" button, which opens Add Node.
#[derive(Component, Default, Clone)]
struct AddNodeButton;

/// Add Node's list, filled from the node types when it appears.
#[derive(Component, Default, Clone)]
struct AddNodeMenu;

/// What the tree needs to know about each node.
type NodeData<'a> = (
    &'a Name,
    Option<&'a Children>,
    Option<&'a NodeKind>,
    Option<&'a Visibility>,
);

/// Rebuild the tree's rows from the level, skipping collapsed subtrees.
fn refresh_tree(
    roots: Query<Entity, With<LevelRoot>>,
    nodes: Query<NodeData>,
    types: Res<NodeTypes>,
    selection: Res<Selection>,
    state: Res<HierarchyState>,
    filters: Query<&SearchText, With<HierarchyFilter>>,
    mut trees: Query<&mut TreeView, With<HierarchyTree>>,
) {
    let filter = filters.iter().next().cloned().unwrap_or_default();
    let mut items = Vec::new();
    for root in &roots {
        push_subtree(
            root, 0, &nodes, &types, &selection, &state, &filter, &mut items,
        );
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
    types: &NodeTypes,
    selection: &Selection,
    state: &HierarchyState,
    filter: &SearchText,
    items: &mut Vec<TreeItem>,
) -> bool {
    // Deleted nodes are disabled and drop out of queries here.
    let Ok((name, children, kind, visibility)) = nodes.get(entity) else {
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
        icon: Some(types.icon_for(kind)),
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
            any_child |= push_subtree(
                child,
                depth + 1,
                nodes,
                types,
                selection,
                state,
                filter,
                items,
            );
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
    selection: Res<Selection>,
    mut state: ResMut<HierarchyState>,
    mut commands: Commands,
) {
    for event in events.read() {
        match *event {
            TreeEvent::Select { tree, key, mode } => {
                let Ok(view) = trees.get(tree) else { continue };
                let entity = Entity::from_bits(key);
                let mut next = selection.clone();
                match mode {
                    SelectMode::Replace => next.select(entity),
                    SelectMode::Toggle => next.toggle(entity),
                    SelectMode::Extend => select_range(&mut next, view, entity),
                }
                // Through `selection.set`, as MCP and scripts select.
                if next != *selection {
                    let entities: Vec<u64> = next.entities().iter().map(|e| e.to_bits()).collect();
                    run_command(
                        &mut commands,
                        "selection.set",
                        json!({ "entities": entities }),
                    );
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
            TreeEvent::Drop { tree, key, dropped } => {
                if trees.contains(tree) {
                    let node = Entity::from_bits(key);
                    commands.queue(move |world: &mut World| drop_on_node(world, node, dropped));
                }
            }
        }
    }
}

/// Something dropped on a node: a script file from the FileSystem attaches
/// its script (`script.attach`), as dragging a script onto an object does
/// in Unity.
fn drop_on_node(world: &mut World, node: Entity, dropped: Entity) {
    let Some(file) = crate::docks::filesystem::file_of_cell(world, dropped) else {
        return;
    };
    let script = world
        .resource::<ScriptTypes>()
        .in_file(&file)
        .map(|script| script.type_path.clone());
    let Some(script) = script else {
        warn!(
            "{} holds no script the last build described (or several: use Add Component)",
            file.display()
        );
        return;
    };
    if let Err(error) = world.run_command(
        "script.attach",
        json!({ "entity": node.to_bits(), "script": script }),
    ) {
        warn!("Could not attach {script}: {error}");
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

/// Add a node of type `kind` under the selected node (or at the top of the
/// level) and select it. One undo step, through `node.create`.
pub fn add_node(world: &mut World, kind: &str) {
    let Some(root) = level_root(world) else {
        warn!("No level is open");
        return;
    };
    let parent = world
        .resource::<Selection>()
        .primary()
        .filter(|e| is_in_level(world, *e));
    let mut args = json!({ "type": kind });
    if let Some(parent) = parent {
        args["parent"] = json!(parent.to_bits());
    }
    match world.run_command("node.create", args) {
        Ok(output) => {
            let Some(node) = output["entity"].as_u64().map(Entity::from_bits) else {
                return;
            };
            world
                .resource_mut::<HierarchyState>()
                .collapsed
                .remove(&parent.unwrap_or(root));
            if let Err(error) =
                world.run_command("selection.set", json!({ "entities": [node.to_bits()] }))
            {
                warn!("Could not select the new {kind}: {error}");
            }
        }
        Err(error) => warn!("Could not add a {kind}: {error}"),
    }
}

/// Open Add Node, as the "+" button does (Ctrl+A).
pub fn open_add_node(world: &mut World) {
    let button = world
        .query_filtered::<Entity, With<AddNodeButton>>()
        .iter(world)
        .next();
    if let Some(button) = button {
        world.trigger(MenuEvent {
            source: button,
            action: MenuAction::Open(NavAction::First),
        });
    }
}

/// Fill Add Node with every registered type, by category, when it appears.
fn fill_add_node_menu(
    menus: Query<Entity, Added<AddNodeMenu>>,
    types: Res<NodeTypes>,
    mut commands: Commands,
) {
    for menu in &menus {
        commands
            .entity(menu)
            .queue_spawn_related_scenes::<Children>(add_node_entries(&types));
    }
}

/// One row of Add Node.
#[derive(Debug)]
enum AddNodeRow<'a> {
    /// A line between categories.
    Divider,
    /// A category's name.
    Heading(&'a str),
    /// A node type to add.
    Item(&'a NodeType),
}

/// Each category's name, then its types, with a line between categories.
fn add_node_rows(types: &NodeTypes) -> Vec<AddNodeRow<'_>> {
    let mut rows = Vec::new();
    for (index, (category, types)) in types.by_category().into_iter().enumerate() {
        if index > 0 {
            rows.push(AddNodeRow::Divider);
        }
        rows.push(AddNodeRow::Heading(category));
        rows.extend(types.into_iter().map(AddNodeRow::Item));
    }
    rows
}

fn add_node_entries(types: &NodeTypes) -> Vec<Box<dyn Scene>> {
    add_node_rows(types)
        .into_iter()
        .map(|row| -> Box<dyn Scene> {
            match row {
                AddNodeRow::Divider => Box::new(bsn! { @FeathersMenuDivider }),
                AddNodeRow::Heading(category) => Box::new(category_heading(category.to_owned())),
                AddNodeRow::Item(node_type) => Box::new(add_node_item(node_type)),
            }
        })
        .collect()
}

fn category_heading(category: String) -> impl Scene {
    bsn! {
        Node { padding: UiRect::axes(px(8), px(2)) }
        Pickable::IGNORE
        Children [
            Text(category)
            DimText
            TextFont {
                font: FontSourceTemplate::Handle(paths::UI),
                font_size: FontSize::Px(12.0),
            }
            Pickable::IGNORE
        ]
    }
}

fn add_node_item(node_type: &NodeType) -> impl Scene {
    let kind = node_type.name.to_string();
    let label = kind.clone();
    let icon = node_type.icon.clone();
    bsn! {
        @FeathersMenuItem {
            @caption: bsn_list! {
                UiIcon { icon: {icon.clone()}, size: 16.0 }
                Pickable::IGNORE
                --
                @caption(label)
            },
        }
        Node { column_gap: px(6) }
        on(move |_: On<Activate>, mut commands: Commands| {
            let kind = kind.clone();
            commands.queue(move |world: &mut World| add_node(world, &kind));
        })
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

#[cfg(test)]
mod tests {
    use bevy::ecs::entity_disabling::Disabled;
    use oddity_core::headless_app;

    use super::*;

    /// A level root with one child, `Enemies`, which is selected.
    fn level() -> (App, Entity, Entity) {
        let mut app = headless_app();
        app.add_plugins(crate::nodes::NodesPlugin)
            .init_resource::<HierarchyState>();
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
        add_node(world, "Node");
        let added = world.resource::<Selection>().primary().unwrap();
        assert_eq!(world.get::<Name>(added).unwrap().as_str(), "Node");

        // A second node next to it gets a distinct name.
        world.resource_mut::<Selection>().select(enemies);
        add_node(world, "Node");
        assert_eq!(names_under(world, enemies), ["Node", "Node2"]);
    }

    #[test]
    fn adding_a_camera_is_one_undo_step_and_it_stays_off() {
        let (mut app, _, enemies) = level();
        let world = app.world_mut();
        add_node(world, "Camera3D");
        let camera = world.resource::<Selection>().primary().unwrap();
        assert_eq!(world.get::<NodeKind>(camera).unwrap().0, "Camera3D");
        assert!(!world.get::<Camera>(camera).unwrap().is_active);
        assert_eq!(world.resource::<oddity_core::History>().undo_len(), 1);
        world.undo().unwrap();
        assert!(names_under(world, enemies).is_empty());
    }

    #[test]
    fn add_node_lists_a_plugins_type_under_its_category() {
        let (mut app, _, _) = level();
        app.register_node(
            NodeType::new::<Transform>("Waypoint", Icon::godot("Marker3D")).category("Paths"),
        );
        let types = app.world().resource::<NodeTypes>();
        let rows = add_node_rows(types);
        let heading = rows
            .iter()
            .position(|row| matches!(row, AddNodeRow::Heading("Paths")))
            .expect("the plugin's category is listed");
        assert!(matches!(rows[heading - 1], AddNodeRow::Divider));
        assert!(matches!(&rows[heading + 1], AddNodeRow::Item(t) if t.name == "Waypoint"));
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
