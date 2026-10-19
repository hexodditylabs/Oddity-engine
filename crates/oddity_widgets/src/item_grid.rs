//! An icon grid, drawn like Godot 4.7's `ItemList` in icon mode: the file
//! list of the FileSystem dock in split mode, and Unity's Project window.
//!
//! It works like [`tree`](crate::tree): the owner puts the cells in
//! [`ItemGrid::items`], cells are rebuilt when that list changes, and clicks
//! come back as [`GridEvent`] messages. The widget keeps no selection of its
//! own.
//!
//! What a cell looks like:
//!
//! ```text
//!  ╭──────────────╮
//!  │    [icon]    │
//!  │     [b]      │   b: badge, a small type icon on a generic thumbnail
//!  │              │
//!  │  file_name   │   up to two lines, then cut off
//!  ╰──────────────╯
//! ```

use bevy::{
    picking::{Pickable, PickingSettings, events::PointerClick, pointer::PointerButton},
    prelude::*,
    text::LineHeight,
};
use oddity_icons::{Icon, UiIcon, UiIconSystems};

use crate::{
    ActivePalette,
    double_click::LastClick,
    fonts::OddityFonts,
    hover::{HoverFill, hover_fill_out, hover_fill_over, hover_fill_press, hover_fill_release},
    tree::{SelectMode, select_mode},
};

/// Icon size in a cell. Godot's `docks/filesystem/thumbnail_size` default.
pub const THUMBNAIL_SIZE: f32 = 64.0;
/// Width of a cell. Godot makes the FileSystem's columns 1.5 thumbnails wide.
pub const CELL_WIDTH: f32 = THUMBNAIL_SIZE * 1.5;
/// Size of a cell's badge.
const BADGE_SIZE: f32 = 16.0;
/// Label font size.
const FONT_SIZE: f32 = 13.0;
/// Label line height, relative to the font size. Bevy's default, set
/// explicitly so [`LABEL_HEIGHT`] can rely on it.
const LINE_HEIGHT: f32 = 1.2;
/// Godot shows at most two lines of a file name under its thumbnail.
const LABEL_HEIGHT: f32 = FONT_SIZE * LINE_HEIGHT * 2.0;

pub(crate) fn plugin(app: &mut App) {
    app.add_message::<GridEvent>().add_systems(
        PostUpdate,
        // Before the icon renderer, so new rows' icons show on their first frame.
        rebuild_cells.before(UiIconSystems),
    );
}

/// One cell.
#[derive(Clone, Debug, PartialEq)]
pub struct GridItem {
    /// The owner's id for this cell (a path hash, ...).
    pub key: u64,
    /// Text under the icon.
    pub label: String,
    /// The big icon.
    pub icon: Icon,
    /// A small icon drawn on the big one, centred across it with its top
    /// edge at the big icon's middle.
    pub badge: Option<Icon>,
    /// Drawn with the selection colour.
    pub selected: bool,
}

/// The grid container. Set `items` to change what is shown. Give it
/// `Overflow::scroll_y()` and Bevy's `ScrollArea` to scroll long lists.
#[derive(Component, Default, Clone, Debug)]
#[require(Node)]
pub struct ItemGrid {
    /// Cells in display order, laid out left to right, then top to bottom.
    pub items: Vec<GridItem>,
}

/// What the user did in a grid.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridEvent {
    /// A cell was clicked.
    Select {
        /// The [`ItemGrid`] entity.
        grid: Entity,
        /// The cell's key.
        key: u64,
        /// From the modifier keys.
        mode: SelectMode,
    },
    /// A cell was double-clicked (open a folder, open a file, ...).
    Activate {
        /// The [`ItemGrid`] entity.
        grid: Entity,
        /// The cell's key.
        key: u64,
    },
}

/// A spawned cell, pointing back at its grid and key. Dragging a cell onto
/// another widget hands that widget this entity (a tree's
/// [`TreeEvent::Drop`](crate::tree::TreeEvent::Drop)), so it can tell which
/// item it got.
#[derive(Component, Clone, Copy, Debug)]
pub struct GridCell {
    /// The [`ItemGrid`] it belongs to.
    pub grid: Entity,
    /// Its item's key.
    pub key: u64,
}

/// The grid's own layout. Cells wrap onto new rows as the panel narrows.
fn grid_node(node: &mut Node) {
    node.flex_direction = FlexDirection::Row;
    node.flex_wrap = FlexWrap::Wrap;
    node.align_content = AlignContent::FlexStart;
    node.align_items = AlignItems::FlexStart;
}

fn rebuild_cells(
    mut commands: Commands,
    mut grids: Query<(Entity, &ItemGrid, &mut Node, Option<&Children>), Changed<ItemGrid>>,
    palette: Res<ActivePalette>,
    fonts: Res<OddityFonts>,
) {
    for (grid, view, mut node, children) in &mut grids {
        grid_node(&mut node);
        if let Some(children) = children {
            for child in children {
                commands.entity(*child).despawn();
            }
        }
        for item in &view.items {
            spawn_cell(&mut commands, grid, item, &palette, &fonts);
        }
    }
}

fn spawn_cell(
    commands: &mut Commands,
    grid: Entity,
    item: &GridItem,
    palette: &ActivePalette,
    fonts: &OddityFonts,
) {
    let base = if item.selected {
        palette.item_selected
    } else {
        Color::NONE
    };
    let cell = commands
        .spawn((
            Node {
                width: Val::Px(CELL_WIDTH),
                flex_shrink: 0.0,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(2.0),
                padding: UiRect::all(Val::Px(4.0)),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            GridCell {
                grid,
                key: item.key,
            },
            HoverFill { base },
            BackgroundColor(base),
            ChildOf(grid),
        ))
        .observe(hover_fill_over)
        .observe(hover_fill_out)
        .observe(hover_fill_press)
        .observe(hover_fill_release)
        .observe(click_cell)
        .id();

    let thumbnail = commands
        .spawn((
            UiIcon::new(item.icon.clone(), THUMBNAIL_SIZE),
            Pickable::IGNORE,
            ChildOf(cell),
        ))
        .id();
    if let Some(badge) = &item.badge {
        commands.spawn((
            UiIcon::new(badge.clone(), BADGE_SIZE),
            // On the page's blank middle, clear of the outline and folded corner.
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px((THUMBNAIL_SIZE - BADGE_SIZE) / 2.0),
                top: Val::Px(THUMBNAIL_SIZE * 0.5),
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(thumbnail),
        ));
    }

    let label = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                max_height: Val::Px(LABEL_HEIGHT),
                justify_content: JustifyContent::Center,
                overflow: Overflow::clip(),
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(cell),
        ))
        .id();
    commands.spawn((
        Text::new(item.label.clone()),
        TextFont {
            font: fonts.ui.clone().into(),
            font_size: FontSize::Px(FONT_SIZE),
            ..default()
        },
        LineHeight::RelativeToFont(LINE_HEIGHT),
        TextLayout {
            justify: Justify::Center,
            linebreak: LineBreak::AnyCharacter,
        },
        TextColor(if item.selected {
            palette.font_focus
        } else {
            palette.font
        }),
        Pickable::IGNORE,
        ChildOf(label),
    ));
}

fn click_cell(
    click: On<PointerClick>,
    cells: Query<&GridCell>,
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<PickingSettings>,
    mut last_click: ResMut<LastClick>,
    mut events: MessageWriter<GridEvent>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    let Ok(cell) = cells.get(click.event_target()) else {
        return;
    };
    if last_click.is_double(&settings, cell.grid, cell.key) {
        events.write(GridEvent::Activate {
            grid: cell.grid,
            key: cell.key,
        });
    } else {
        events.write(GridEvent::Select {
            grid: cell.grid,
            key: cell.key,
            mode: select_mode(&keys),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_cells_show_their_icons_on_the_first_frame() {
        let mut app = crate::test_app();
        plugin(&mut app);
        app.world_mut().spawn(ItemGrid {
            items: vec![GridItem {
                key: 1,
                label: "player.rs".into(),
                icon: Icon::godot("File"),
                badge: Some(Icon::godot("Script")),
                selected: false,
            }],
        });
        app.update();
        assert!(crate::every_icon_drawn(&mut app));
    }
}
