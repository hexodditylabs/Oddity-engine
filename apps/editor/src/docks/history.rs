//! The History dock: every undo step, as in Godot.
//!
//! The first row is the level as opened; each later row is one step. The
//! current state is highlighted and undone steps are dim. Clicking a row
//! undoes or redoes as many steps as it takes to get there.

use bevy::prelude::*;
use oddity_core::{EditorWorldExt, History};
use oddity_widgets::tree::{TreeEvent, TreeItem, TreeView};

pub struct HistoryPlugin;

impl Plugin for HistoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (jump_to_step, refresh).chain());
    }
}

#[derive(Component, Default, Clone)]
struct HistoryList;

pub fn history_page() -> impl Scene {
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

/// The rows: "Level opened", then the undo steps, then the undone ones.
fn rows(history: &History) -> Vec<TreeItem> {
    let current = history.undo_len();
    std::iter::once("Level opened".into())
        .chain(history.undo_labels())
        .chain(history.redo_labels())
        .enumerate()
        .map(|(step, label)| TreeItem {
            key: step as u64,
            depth: 0,
            label: label.into_owned(),
            icon: None,
            has_children: false,
            expanded: false,
            selected: step == current,
            dimmed: step > current,
            buttons: Vec::new(),
        })
        .collect()
}

fn refresh(
    history: Res<History>,
    mut lists: Query<&mut TreeView, With<HistoryList>>,
    added: Query<(), Added<HistoryList>>,
) {
    if !history.is_changed() && added.is_empty() {
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

    use super::*;

    #[test]
    fn jumping_undoes_and_redoes_to_the_clicked_step() {
        let mut app = headless_app();
        let world = app.world_mut();
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
}
