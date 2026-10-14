//! Tabbed docks, drawn the way Godot 4.7 draws them.
//!
//! A dock is a [`Dock`] with a tab bar and one page per tab. Tabs and pages
//! are matched by index; only the active page is laid out (`Display::None`
//! hides the rest, so hidden pages cost no layout time).
//!
//! Two looks, chosen with [`DockStyle`]:
//!
//! - [`DockStyle::Tabs`] (side docks): tabs sit on the window background
//!   above the panel. The active tab has the panel's colour and rounded top
//!   corners, so tab and page read as one shape. No accent line: Godot's
//!   modern theme marks the active tab by colour alone.
//! - [`DockStyle::Buttons`] (Godot's bottom panel): the whole dock is one
//!   panel, and the tabs are flat buttons along its bottom edge.
//!
//! ```ignore
//! bsn! {
//!     @dock()
//!     Node { width: px(260) }
//!     Children [
//!         @dock_tabs(vec![tab("Scene"), tab("Import")])
//!         -- @dock_page(0) Children [ ... ]
//!         -- @dock_page(1) Children [ ... ]
//!     ]
//! }
//! ```
//!
//! A tab bar can be wrapped in a row with extra buttons after it (Godot puts
//! the distraction-free toggle there); a tab finds its dock by walking up to
//! the nearest [`Dock`]. Switching tabs is done by clicking or by setting
//! [`Dock::active`].

use bevy::{
    picking::{Pickable, events::PointerClick, hover::Hovered, pointer::PointerButton},
    prelude::*,
    text::FontSourceTemplate,
};
use oddity_icons::{Icon, UiIcon};

use crate::{ActivePalette, Palette, fonts::paths};

/// Height of a side dock's tab bar. The tabs are shorter and sit at its
/// bottom, which leaves Godot's 8 px of window background above them.
pub const TAB_BAR_HEIGHT: f32 = 40.0;
/// Height of a side dock's tab.
pub const TAB_HEIGHT: f32 = 32.0;
/// Height of the button bar along the bottom of a [`DockStyle::Buttons`] dock.
pub const BUTTON_BAR_HEIGHT: f32 = 36.0;
/// Corner radius of panels and tabs.
pub const RADIUS: f32 = 4.0;

/// Registers the system that paints tabs and shows the active page.
pub(crate) fn plugin(app: &mut App) {
    app.add_systems(Update, sync_docks);
}

/// A tabbed panel. `active` is the index of the shown tab.
#[derive(Component, Default, Clone, Debug, PartialEq, Eq)]
#[require(DockStyle)]
pub struct Dock {
    /// Index of the visible tab and page.
    pub active: usize,
}

/// How a dock draws its tabs. See the module docs.
#[derive(Component, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockStyle {
    /// Tabs above the panel, for side docks.
    #[default]
    Tabs,
    /// Flat buttons along the bottom of the panel, for Godot's bottom panel.
    Buttons,
}

/// A tab in a dock's tab bar.
#[derive(Component, Default, Clone, Debug)]
#[require(Hovered)]
pub struct DockTab(pub usize);

/// A dock's page, shown when its tab is active.
#[derive(Component, Default, Clone, Debug)]
pub struct DockPage(pub usize);

/// What a tab shows. Build with [`tab`].
#[derive(Clone, Debug)]
pub struct TabSpec {
    label: &'static str,
    icon: Option<Icon>,
}

/// A tab with a text label. Godot's side docks use text only.
pub fn tab(label: &'static str) -> TabSpec {
    TabSpec { label, icon: None }
}

impl TabSpec {
    /// Show an icon before the label, as Godot's 2D/3D switch does.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
}

/// The dock frame. Give it a size (`width` for side docks, `height` for
/// bottom ones) or `flex_grow`. Patch in `DockStyle::Buttons` for a bottom panel.
pub fn dock() -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            min_width: px(0),
            min_height: px(0),
            border_radius: BorderRadius::all(px(RADIUS)),
            overflow: Overflow::clip(),
        }
        Dock
    }
}

/// Tabs for a [`DockStyle::Tabs`] dock. Put it before the pages. When it
/// shares a row with other buttons, patch in `Node { flex_grow: 1.0 }`.
pub fn dock_tabs(tabs: Vec<TabSpec>) -> impl Scene {
    let tabs: Vec<_> = tabs
        .into_iter()
        .enumerate()
        .map(|(index, spec)| dock_tab(index, spec, TAB_HEIGHT, 16.0))
        .collect();
    bsn! {
        Node {
            height: px(TAB_BAR_HEIGHT),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexEnd,
        }
        Children [ {tabs} ]
    }
}

/// Buttons for a [`DockStyle::Buttons`] dock. Put it after the pages.
pub fn dock_buttons(tabs: Vec<TabSpec>) -> impl Scene {
    let tabs: Vec<_> = tabs
        .into_iter()
        .enumerate()
        .map(|(index, spec)| dock_tab(index, spec, 28.0, 8.0))
        .collect();
    bsn! {
        Node {
            height: px(BUTTON_BAR_HEIGHT),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(2),
            padding: UiRect::horizontal(px(4)),
        }
        Children [ {tabs} ]
    }
}

fn dock_tab(index: usize, spec: TabSpec, height: f32, padding: f32) -> impl Scene {
    let icon = spec.icon.map(|icon| {
        bsn! {
            UiIcon { icon: {icon.clone()}, size: 16.0 }
            Pickable::IGNORE
        }
    });
    bsn! {
        Node {
            height: px(height),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(6),
            padding: UiRect::horizontal(px(padding)),
        }
        DockTab({index})
        BackgroundColor(Color::NONE)
        on(select_tab)
        Children [
            {icon}
            --
            Text({spec.label})
            TextFont {
                font: FontSourceTemplate::Handle(paths::UI),
                font_size: FontSize::Px(14.0),
            }
            TextColor(Color::WHITE)
            Pickable::IGNORE
        ]
    }
}

/// A page of the dock with this tab index.
pub fn dock_page(index: usize) -> impl Scene {
    bsn! {
        Node {
            flex_grow: 1.0,
            min_height: px(0),
            flex_direction: FlexDirection::Column,
            overflow: Overflow::clip(),
        }
        DockPage({index})
        BackgroundColor(Color::NONE)
    }
}

/// The panel under a dock's tabs when every tab shows the same content, as
/// with Godot's 3D/2D switch: the view changes mode, not page. Read
/// [`Dock::active`] to know which tab is chosen.
pub fn dock_body() -> impl Scene {
    bsn! {
        Node {
            flex_grow: 1.0,
            min_height: px(0),
            flex_direction: FlexDirection::Column,
            overflow: Overflow::clip(),
        }
        DockBody
        BackgroundColor(Color::NONE)
    }
}

/// See [`dock_body`].
#[derive(Component, Default, Clone, Debug)]
pub struct DockBody;

/// The rounded panel shape under a [`DockStyle::Tabs`] bar. Square where
/// the first tab joins it, so tab and panel read as one shape.
fn panel_radius(active: usize) -> BorderRadius {
    let top_left = if active == 0 { 0.0 } else { RADIUS };
    BorderRadius::px(top_left, RADIUS, RADIUS, RADIUS)
}

/// The dock an entity belongs to: the nearest [`Dock`] ancestor.
fn owning_dock(
    entity: Entity,
    parents: &Query<&ChildOf>,
    docks: &Query<(&Dock, &DockStyle)>,
) -> Option<(Dock, DockStyle)> {
    parents
        .iter_ancestors(entity)
        .find_map(|e| docks.get(e).ok())
        .map(|(dock, style)| (dock.clone(), *style))
}

fn select_tab(
    click: On<PointerClick>,
    tabs: Query<&DockTab>,
    parents: Query<&ChildOf>,
    mut docks: Query<&mut Dock>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    let tab = click.event_target();
    let Ok(DockTab(index)) = tabs.get(tab) else {
        return;
    };
    if let Some(mut dock) = parents
        .iter_ancestors(tab)
        .find(|e| docks.contains(*e))
        .and_then(|e| docks.get_mut(e).ok())
    {
        dock.set_if_neq(Dock { active: *index });
    }
}

/// How a tab looks in each state.
struct TabLook {
    background: Color,
    text: Color,
    icon_alpha: f32,
    radius: BorderRadius,
}

fn tab_look(palette: &Palette, style: DockStyle, active: bool, hovered: bool) -> TabLook {
    match style {
        DockStyle::Tabs => TabLook {
            background: match (active, hovered) {
                (true, _) => palette.surface_base,
                (false, true) => palette.surface_low,
                (false, false) => Color::NONE,
            },
            text: match (active, hovered) {
                (true, _) => palette.font,
                (false, true) => palette.font_hover,
                (false, false) => palette.font_secondary,
            },
            icon_alpha: if active || hovered { 1.0 } else { 0.6 },
            radius: BorderRadius::px(RADIUS, RADIUS, 0.0, 0.0),
        },
        DockStyle::Buttons => TabLook {
            background: match (active, hovered) {
                (true, _) => palette.button_normal,
                (false, true) => palette.flat_hover,
                (false, false) => Color::NONE,
            },
            text: if active || hovered {
                palette.font_hover
            } else {
                palette.font
            },
            icon_alpha: 1.0,
            radius: BorderRadius::all(px(RADIUS)),
        },
    }
}

/// Paint tabs and frames from the theme and show only the active page.
///
/// Runs every frame over the handful of tabs there are, and only writes
/// values that differ, so nothing downstream sees a change unless one happened.
fn sync_docks(
    palette: Res<ActivePalette>,
    parents: Query<&ChildOf>,
    docks: Query<(&Dock, &DockStyle)>,
    mut frames: Query<
        (&DockStyle, &mut BackgroundColor),
        (With<Dock>, Without<DockTab>, Without<DockPage>),
    >,
    mut tabs: Query<
        (
            Entity,
            &DockTab,
            &Hovered,
            &mut BackgroundColor,
            &mut Node,
            &Children,
        ),
        (Without<Dock>, Without<DockPage>),
    >,
    mut pages: Query<
        (Entity, &DockPage, &mut BackgroundColor, &mut Node),
        (Without<Dock>, Without<DockTab>, Without<DockBody>),
    >,
    mut bodies: Query<
        (Entity, &mut BackgroundColor, &mut Node),
        (With<DockBody>, Without<Dock>, Without<DockTab>),
    >,
    mut labels: Query<&mut TextColor>,
    mut icons: Query<&mut ImageNode>,
) {
    for (style, mut background) in &mut frames {
        // A tabs dock is transparent so the window shows around its tabs;
        // a buttons dock is a single panel.
        let colour = match style {
            DockStyle::Tabs => Color::NONE,
            DockStyle::Buttons => palette.surface_base,
        };
        background.set_if_neq(BackgroundColor(colour));
    }

    for (entity, DockTab(index), hovered, mut background, mut node, children) in &mut tabs {
        let Some((dock, style)) = owning_dock(entity, &parents, &docks) else {
            continue;
        };
        let look = tab_look(&palette, style, dock.active == *index, hovered.get());
        background.set_if_neq(BackgroundColor(look.background));
        if node.border_radius != look.radius {
            node.border_radius = look.radius;
        }
        for child in children {
            if let Ok(mut colour) = labels.get_mut(*child) {
                colour.set_if_neq(TextColor(look.text));
            }
            if let Ok(mut image) = icons.get_mut(*child) {
                let tint = Color::WHITE.with_alpha(look.icon_alpha);
                if image.color != tint {
                    image.color = tint;
                }
            }
        }
    }

    for (entity, DockPage(index), mut background, mut node) in &mut pages {
        let Some((dock, style)) = owning_dock(entity, &parents, &docks) else {
            continue;
        };
        let active = dock.active == *index;
        let display = if active { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        if style == DockStyle::Tabs {
            background.set_if_neq(BackgroundColor(palette.surface_base));
            let radius = panel_radius(dock.active);
            if node.border_radius != radius {
                node.border_radius = radius;
            }
        }
    }

    for (entity, mut background, mut node) in &mut bodies {
        let Some((dock, _)) = owning_dock(entity, &parents, &docks) else {
            continue;
        };
        background.set_if_neq(BackgroundColor(palette.surface_base));
        let radius = panel_radius(dock.active);
        if node.border_radius != radius {
            node.border_radius = radius;
        }
    }
}
