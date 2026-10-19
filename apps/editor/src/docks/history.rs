//! The History dock: every undo step, as in Godot.
//!
//! The first row is the level as opened; each later row is one step. The
//! current state is highlighted and undone steps are dim. Clicking a row
//! undoes or redoes as many steps as it takes to get there.
//!
//! Steps you didn't make yourself (the CLI, an MCP agent, a plugin, a
//! script) carry a small icon and say who made them: "Rename Goblin · Claude".

use bevy::prelude::*;
use oddity_core::{CommandExecuted, EditorWorldExt, History, Source};
use oddity_icons::Icon;
use oddity_plugin_api::{DockSlot, DockTab, EditorAppExt, EditorMode};
use oddity_widgets::tree::{TreeEvent, TreeItem, TreeView};

use super::SECOND_TAB;

/// The History tab, and Edit › Undo and Redo.
pub struct HistoryPlugin;

impl Plugin for HistoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (jump_to_step, refresh).chain());
        if app.editor_mode() == EditorMode::Window {
            app.add_dock_tab(
                DockTab::new(DockSlot::FileSystem, "History", history_page).order(SECOND_TAB),
            )
            .add_menu_item("Edit/Undo", "history.undo")
            .add_menu_item("Edit/Redo", "history.redo");
        }
    }
}

#[derive(Component, Default, Clone)]
struct HistoryList;

fn history_page() -> impl Scene {
    bsn! {
        Node {
            flex_grow: 1.0,
            min_height: px(0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::top(px(6)),
            overflow: Overflow::scroll_y(),
        }
        TreeView
        HistoryList
    }
}

/// The icon for a step you didn't make yourself.
fn source_icon(source: &Source) -> Option<Icon> {
    match source {
        Source::User => None,
        Source::Cli => Some(Icon::godot("Terminal")),
        Source::Mcp { .. } => Some(Icon::at("bot")),
        Source::Plugin { .. } => Some(Icon::godot("EditorPlugin")),
        Source::Script { .. } => Some(Icon::godot("Script")),
    }
}

/// The rows: "Level opened", then the undo steps, then the undone ones.
fn rows(history: &History) -> Vec<TreeItem> {
    let current = history.undo_len();
    let steps = history
        .undo_steps()
        .chain(history.redo_steps())
        .map(|step| {
            let label = match step.source.who() {
                Some(who) => format!("{} · {who}", step.label),
                None => step.label.into_owned(),
            };
            (label, source_icon(step.source))
        });
    std::iter::once(("Level opened".to_owned(), None))
        .chain(steps)
        .enumerate()
        .map(|(step, (label, icon))| TreeItem {
            key: step as u64,
            depth: 0,
            label,
            icon,
            has_children: false,
            expanded: false,
            selected: step == current,
            dimmed: step > current,
            buttons: Vec::new(),
        })
        .collect()
}

fn refresh(
    mut executed: MessageReader<CommandExecuted>,
    history: Res<History>,
    mut lists: Query<&mut TreeView, With<HistoryList>>,
    added: Query<(), Added<HistoryList>>,
) {
    // Every history change comes from a command, undo and redo included.
    if executed.read().count() == 0 && added.is_empty() {
        return;
    }
    let items = rows(&history);
    for mut list in &mut lists {
        list.items.clone_from(&items);
    }
}

fn jump_to_step(
    mut events: MessageReader<TreeEvent>,
    lists: Query<(), With<HistoryList>>,
    mut commands: Commands,
) {
    for event in events.read() {
        if let TreeEvent::Select { tree, key, .. } = *event
            && lists.contains(tree)
        {
            commands.queue(move |world: &mut World| jump(world, key as usize));
        }
    }
}

/// Undo or redo until `step` steps are applied.
fn jump(world: &mut World, step: usize) {
    loop {
        let current = world.resource::<History>().undo_len();
        let result = match current.cmp(&step) {
            std::cmp::Ordering::Greater => world.undo(),
            std::cmp::Ordering::Less => world.redo(),
            std::cmp::Ordering::Equal => return,
        };
        if let Err(error) = result {
            warn!("Could not reach that step: {error}");
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use oddity_core::{commands::SpawnEntity, headless_app};
    use serde_json::json;

    use super::*;

    #[test]
    fn jumping_undoes_and_redoes_to_the_clicked_step() {
        let mut app = headless_app();
        let world = app.world_mut();
        world.spawn((Name::new("Main"), oddity_core::LevelRoot));
        for name in ["A", "B", "C"] {
            let spawn = SpawnEntity::new(world, name);
            world.execute(spawn).unwrap();
        }
        jump(world, 1);
        let history = world.resource::<History>();
        assert_eq!((history.undo_len(), history.redo_len()), (1, 2));

        let items = rows(history);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["Level opened", "Add A", "Add B", "Add C"]);
        assert!(items[1].selected && items[2].dimmed && items[3].dimmed);

        jump(world, 3);
        assert_eq!(world.resource::<History>().redo_len(), 0);
    }

    #[test]
    fn steps_you_did_not_make_say_who_made_them() {
        let mut app = headless_app();
        let world = app.world_mut();
        world.spawn((Name::new("Main"), oddity_core::LevelRoot));
        world
            .run_command("entity.spawn", json!({"name": "Crate"}))
            .unwrap();
        let agent = Source::Mcp {
            client: "Claude".into(),
        };
        world
            .run_command_as("entity.spawn", json!({"name": "Wall"}), agent)
            .unwrap();

        let items = rows(world.resource::<History>());
        assert_eq!(items[1].label, "Add Crate");
        assert_eq!(items[1].icon, None);
        assert_eq!(items[2].label, "Add Wall · Claude");
        assert_eq!(items[2].icon, Some(Icon::at("bot")));
    }

    #[test]
    fn every_source_icon_exists() {
        let sources = [
            Source::Cli,
            Source::Mcp { client: "x".into() },
            Source::Plugin { name: "x".into() },
            Source::Script { file: "x".into() },
        ];
        for source in &sources {
            let icon = source_icon(source).unwrap();
            assert!(icon.exists(), "{source:?}: no icon {}", icon.name());
        }
    }
}
