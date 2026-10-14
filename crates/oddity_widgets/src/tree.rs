//! A tree view, drawn like Godot 4.7's scene tree.
//!
//! The owner describes the visible rows as a flat list of [`TreeItem`]s in
//! display order (parents before children, depth for indentation) and puts it
//! in [`TreeView::items`]. Rows are rebuilt when that list changes. Clicks come
//! back as [`TreeEvent`] messages; the widget keeps no selection or expansion
//! state of its own, so the owner stays the single source of truth (the
//! Hierarchy dock, for instance, uses the editor's `Selection`).
//!
//! What a row looks like, left to right:
//!
//! ```text
//!  │ └─▾ ╭ [icon] Label ────────────╮  [btn][btn]
//!  guides arrow  selection pill            row buttons (e.g. visibility)
//! ```
//!
//! Guides are Godot's "relationship lines". The selection pill starts after
//! the arrow, so the lines and arrow stay visible on a selected row, and
//! stops before the row buttons, which Godot keeps outside the highlight.
//!
//! Rebuilding every row on change is simple and fast for the hundreds of
//! nodes a level usually has. Very large trees will want row virtualisation.

use bevy::{
    picking::{Pickable, events::PointerClick, pointer::PointerButton},
    prelude::*,
};
use oddity_icons::{Icon, UiIcon};

use crate::{
    ActivePalette,
    fonts::OddityFonts,
    hover::{HoverFill, hover_fill_out, hover_fill_over, hover_fill_press, hover_fill_release},
};

/// Height of one row, as in Godot.
pub const ROW_HEIGHT: f32 = 22.0;
/// Indentation per depth level, as in Godot.
pub const INDENT: f32 = 12.0;
/// Left margin before the first arrow column.
const MARGIN: f32 = 4.0;
/// Width of the expand/collapse arrow column.
const ARROW_WIDTH: f32 = 14.0;
/// Space between the pill's edge and the node icon.
const PILL_PADDING: f32 = 6.0;

pub(crate) fn plugin(app: &mut App) {
    app.add_message::<TreeEvent>().add_systems(
        PostUpdate,
        rebuild_rows.before(bevy::ui::UiSystems::Prepare),
    );
}

/// One visible row.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeItem {
    /// The owner's id for this row (an entity's bits, a path hash, ...).
    pub key: u64,
    /// Nesting depth; 0 for top-level rows.
    pub depth: u16,
    /// Text shown.
    pub label: String,
    /// Icon shown before the label, if any.
    pub icon: Option<Icon>,
    /// Whether to show an expand/collapse arrow.
    pub has_children: bool,
    /// Arrow points down when true.
    pub expanded: bool,
    /// Drawn with the selection colour.
    pub selected: bool,
    /// Drawn with dim text, for rows that are there but not in effect (an
    /// undone history step, say).
    pub dimmed: bool,
    /// Buttons on the right of the row, such as Godot's visibility eye.
    pub buttons: Vec<TreeButton>,
}

/// A button on the right of a row. Clicking it sends [`TreeEvent::Button`]
/// and does not select the row.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeButton {
    /// The owner's id for this button, returned in the event.
    pub id: u32,
    /// What it shows.
    pub icon: Icon,
}

/// The tree container. Set `items` to change what is shown.
#[derive(Component, Default, Clone, Debug)]
#[require(Node)]
pub struct TreeView {
    /// Visible rows in display order.
    pub items: Vec<TreeItem>,
}

/// How a click changes the selection, from the modifier keys held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectMode {
    /// Plain click: select only this row.
    Replace,
    /// Ctrl/Cmd+click: add or remove this row.
    Toggle,
    /// Shift+click: add this row.
    Extend,
}

/// What the user did in a tree.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeEvent {
    /// A row was clicked.
    Select {
        /// The [`TreeView`] entity.
        tree: Entity,
        /// The row's key.
        key: u64,
        /// From the modifier keys.
        mode: SelectMode,
    },
    /// An expand/collapse arrow was clicked.
    Toggle {
        /// The [`TreeView`] entity.
        tree: Entity,
        /// The row's key.
        key: u64,
    },
    /// A row was double-clicked (open, rename, ...).
    Activate {
        /// The [`TreeView`] entity.
        tree: Entity,
        /// The row's key.
        key: u64,
    },
    /// A row button was clicked.
    Button {
        /// The [`TreeView`] entity.
        tree: Entity,
        /// The row's key.
        key: u64,
        /// The button's [`TreeButton::id`].
        id: u32,
    },
}

/// A spawned row, pointing back at its tree and key.
#[derive(Component, Clone, Copy, Debug)]
struct TreeRow {
    tree: Entity,
    key: u64,
}

/// A row's expand/collapse arrow.
#[derive(Component, Clone, Copy, Debug)]
struct TreeArrow {
    tree: Entity,
    key: u64,
}

/// A row button.
#[derive(Component, Clone, Copy, Debug)]
struct TreeRowButton {
    tree: Entity,
    key: u64,
    id: u32,
}

/// The relationship lines a row draws.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Guides {
    /// The row is its parent's last visible child, so its connector ends
    /// half way down (`└`) instead of continuing (`├`).
    last: bool,
    /// Ancestor depths whose line passes straight through this row, because
    /// that ancestor has more siblings below.
    through: Vec<u16>,
}

/// Work out every row's guides in one pass from the bottom up.
///
/// Walking backwards, `later[d]` says whether a row at depth `d` appears
/// below before anything shallower. That is exactly "this row has a later
/// sibling" for a row at depth `d`.
fn guides(items: &[TreeItem]) -> Vec<Guides> {
    let mut later: Vec<bool> = Vec::new();
    let mut out = vec![Guides::default(); items.len()];
    for (index, item) in items.iter().enumerate().rev() {
        let depth = usize::from(item.depth);
        if later.len() <= depth {
            later.resize(depth + 1, false);
        }
        out[index] = Guides {
            last: !later[depth],
            through: (1..item.depth)
                .filter(|level| later[usize::from(*level)])
                .collect(),
        };
        // Rows below at a greater depth belong to other parents for anything above.
        later.truncate(depth + 1);
        later[depth] = true;
    }
    out
}

/// X of the arrow column's centre at `depth`, where its children's lines run.
fn line_x(depth: u16) -> f32 {
    MARGIN + f32::from(depth) * INDENT + ARROW_WIDTH / 2.0
}

fn rebuild_rows(
    mut commands: Commands,
    trees: Query<(Entity, &TreeView, Option<&Children>), Changed<TreeView>>,
    palette: Res<ActivePalette>,
    fonts: Res<OddityFonts>,
) {
    for (tree, view, children) in &trees {
        if let Some(children) = children {
            for child in children {
                commands.entity(*child).despawn();
            }
        }
        for (item, guides) in view.items.iter().zip(guides(&view.items)) {
            spawn_row(&mut commands, tree, item, &guides, &palette, &fonts);
        }
    }
}

fn spawn_row(
    commands: &mut Commands,
    tree: Entity,
    item: &TreeItem,
    guides: &Guides,
    palette: &ActivePalette,
    fonts: &OddityFonts,
) {
    let row = commands
        .spawn((
            Node {
                height: Val::Px(ROW_HEIGHT),
                flex_shrink: 0.0,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                padding: UiRect {
                    left: Val::Px(MARGIN + f32::from(item.depth) * INDENT),
                    right: Val::Px(8.0),
                    ..default()
                },
                ..default()
            },
            TreeRow {
                tree,
                key: item.key,
            },
            ChildOf(tree),
        ))
        .observe(click_row)
        .id();

    // Lines first, so the selection pill paints over them.
    let line = |left: f32, top: f32, width: f32, height: f32| {
        (
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(left),
                top: Val::Px(top),
                width: Val::Px(width),
                height: Val::Px(height),
                ..default()
            },
            BackgroundColor(palette.relationship_line),
            Pickable::IGNORE,
            ChildOf(row),
        )
    };
    if item.depth > 0 {
        let middle = (ROW_HEIGHT / 2.0).floor();
        let x = line_x(item.depth - 1);
        let stem = if guides.last {
            middle + 1.0
        } else {
            ROW_HEIGHT
        };
        commands.spawn(line(x, 0.0, 1.0, stem));
        // Stop at our own arrow if there is one, else run up to the icon.
        let own_column = MARGIN + f32::from(item.depth) * INDENT;
        let end = if item.has_children {
            own_column + 2.0
        } else {
            own_column + ARROW_WIDTH + PILL_PADDING - 2.0
        };
        commands.spawn(line(x, middle, end - x, 1.0));
        for level in &guides.through {
            commands.spawn(line(line_x(level - 1), 0.0, 1.0, ROW_HEIGHT));
        }
    }

    // The arrow column is always reserved, so labels line up at each depth.
    let mut arrow = commands.spawn((
        Node {
            width: Val::Px(ARROW_WIDTH),
            height: Val::Px(ROW_HEIGHT),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            ..default()
        },
        ChildOf(row),
    ));
    if item.has_children {
        let arrow_icon = if item.expanded {
            "GuiTreeArrowDown"
        } else {
            "GuiTreeArrowRight"
        };
        arrow
            .insert(TreeArrow {
                tree,
                key: item.key,
            })
            .observe(click_arrow)
            .with_child((UiIcon::new(Icon::godot(arrow_icon), 12.0), Pickable::IGNORE));
    }

    let base = if item.selected {
        palette.item_selected
    } else {
        Color::NONE
    };
    let pill = commands
        .spawn((
            Node {
                flex_grow: 1.0,
                min_width: Val::Px(0.0),
                height: Val::Px(ROW_HEIGHT - 2.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(4.0),
                padding: UiRect::left(Val::Px(PILL_PADDING)),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                overflow: Overflow::clip(),
                ..default()
            },
            HoverFill { base },
            BackgroundColor(base),
            ChildOf(row),
        ))
        .observe(hover_fill_over)
        .observe(hover_fill_out)
        .observe(hover_fill_press)
        .observe(hover_fill_release)
        .id();
    if let Some(icon) = &item.icon {
        commands.spawn((
            UiIcon::new(icon.clone(), 16.0),
            Pickable::IGNORE,
            ChildOf(pill),
        ));
    }
    commands.spawn((
        Text::new(item.label.clone()),
        TextFont {
            font: fonts.ui.clone().into(),
            font_size: FontSize::Px(14.0),
            ..default()
        },
        TextLayout {
            linebreak: LineBreak::NoWrap,
            ..default()
        },
        TextColor(if item.selected {
            palette.font_focus
        } else if item.dimmed {
            palette.font_disabled
        } else {
            palette.font
        }),
        Pickable::IGNORE,
        ChildOf(pill),
    ));

    for button in &item.buttons {
        commands
            .spawn((
                Node {
                    width: Val::Px(20.0),
                    height: Val::Px(20.0),
                    margin: UiRect::left(Val::Px(4.0)),
                    flex_shrink: 0.0,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                TreeRowButton {
                    tree,
                    key: item.key,
                    id: button.id,
                },
                HoverFill::default(),
                ChildOf(row),
            ))
            .observe(hover_fill_over)
            .observe(hover_fill_out)
            .observe(click_button)
            .with_child((UiIcon::new(button.icon.clone(), 16.0), Pickable::IGNORE));
    }
}

fn select_mode(keys: &ButtonInput<KeyCode>) -> SelectMode {
    let toggle = if cfg!(target_os = "macos") {
        [KeyCode::SuperLeft, KeyCode::SuperRight]
    } else {
        [KeyCode::ControlLeft, KeyCode::ControlRight]
    };
    if keys.any_pressed(toggle) {
        SelectMode::Toggle
    } else if keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
        SelectMode::Extend
    } else {
        SelectMode::Replace
    }
}

fn click_row(
    click: On<PointerClick>,
    rows: Query<&TreeRow>,
    keys: Res<ButtonInput<KeyCode>>,
    mut events: MessageWriter<TreeEvent>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    let Ok(row) = rows.get(click.event_target()) else {
        return;
    };
    if click.count >= 2 {
        events.write(TreeEvent::Activate {
            tree: row.tree,
            key: row.key,
        });
    } else {
        events.write(TreeEvent::Select {
            tree: row.tree,
            key: row.key,
            mode: select_mode(&keys),
        });
    }
}

fn click_arrow(
    mut click: On<PointerClick>,
    arrows: Query<&TreeArrow>,
    mut events: MessageWriter<TreeEvent>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    if let Ok(arrow) = arrows.get(click.event_target()) {
        // Expanding must not also select the row underneath.
        click.propagate(false);
        events.write(TreeEvent::Toggle {
            tree: arrow.tree,
            key: arrow.key,
        });
    }
}

fn click_button(
    mut click: On<PointerClick>,
    buttons: Query<&TreeRowButton>,
    mut events: MessageWriter<TreeEvent>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    if let Ok(button) = buttons.get(click.event_target()) {
        // Toggling visibility must not change the selection, as in Godot.
        click.propagate(false);
        events.write(TreeEvent::Button {
            tree: button.tree,
            key: button.key,
            id: button.id,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(depths: &[u16]) -> Vec<TreeItem> {
        depths
            .iter()
            .enumerate()
            .map(|(i, depth)| TreeItem {
                key: i as u64,
                depth: *depth,
                label: String::new(),
                icon: None,
                has_children: false,
                expanded: true,
                selected: false,
                dimmed: false,
                buttons: Vec::new(),
            })
            .collect()
    }

    #[test]
    fn last_children_end_their_line() {
        // Main
        // ├ Camera
        // ├ Player
        // │ ├ Body
        // │ └ Collision
        // └ HUD
        //   └ HealthBar
        let g = guides(&rows(&[0, 1, 1, 2, 2, 1, 2]));
        let last: Vec<bool> = g.iter().map(|g| g.last).collect();
        assert_eq!(last, [true, false, false, false, true, true, true]);
    }

    #[test]
    fn lines_pass_through_under_ancestors_with_more_siblings() {
        let g = guides(&rows(&[0, 1, 1, 2, 2, 1, 2]));
        // Body and Collision sit under Player, which has a sibling (HUD) below.
        assert_eq!(g[3].through, [1]);
        assert_eq!(g[4].through, [1]);
        // HealthBar sits under HUD, the last child: nothing passes through.
        assert!(g[6].through.is_empty());
    }
}
