//! What sits inside the title bar, laid out after jackdaw's editor:
//!
//! `menus · level/prefab tabs ·········· level picker · [build play ⌄ pause stop reload | Embedded]`
//!
//! Oddity uses Unity's model: **levels** are the worlds you play, **prefabs**
//! are reusable objects, opened in isolation in their own tab (like Unity's
//! Prefab Mode and Unreal's Blueprint editor).
//!
//! The play controls drive [`PlayState`]; running the game itself arrives
//! with the project build pipeline. Everything here is presentation plus a
//! little state, so later phases only have to react to [`TransportAction`].

use bevy::{
    feathers::{
        controls::{ButtonVariant, FeathersButton},
        display::caption,
    },
    picking::{
        Pickable,
        events::{PointerClick, PointerOut, PointerOver, PointerPress, PointerRelease},
        pointer::PointerButton,
    },
    prelude::*,
    text::FontSourceTemplate,
    ui_widgets::Activate,
};
use oddity_core::History;
use oddity_icons::{Icon, UiIcon};
use oddity_widgets::{ActivePalette, fonts::paths};

use crate::{project::CurrentProject, title_bar::WindowDragArea};

/// Height of the controls in the bar (jackdaw uses 22).
const CONTROL_HEIGHT: f32 = 22.0;
/// Width of an icon-only control.
const ICON_BUTTON: f32 = 22.0;
/// Icon size inside controls.
const ICON: f32 = 14.0;
/// Corner radius of pills and buttons.
const RADIUS: f32 = 5.0;

pub struct TopBarPlugin;

impl Plugin for TopBarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayState>()
            .init_resource::<PlayWindow>()
            .add_message::<TransportAction>()
            .add_systems(
                Update,
                (
                    handle_transport,
                    update_transport_buttons,
                    update_window_mode_label,
                    update_level_tab,
                    style_header_controls,
                )
                    .chain(),
            );
    }
}

/// Whether the game is running in the editor.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayState {
    /// Editing.
    #[default]
    Stopped,
    /// The game is running.
    Playing,
    /// The game is running but frozen.
    Paused,
}

/// Where Play runs the game.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayWindow {
    /// Inside the editor's viewport.
    #[default]
    Embedded,
    /// In its own window.
    Windowed,
}

/// Marks a transport button with the action it sends.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct TransportButton(pub TransportAction);

/// A click on one of the transport controls.
#[derive(Message, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TransportAction {
    /// Compile the project's game code.
    #[default]
    Build,
    /// Run the game (or resume it).
    Play,
    /// Freeze or unfreeze the running game.
    Pause,
    /// Stop the game and return to editing.
    Stop,
    /// Hot-reload the running game.
    Reload,
}

impl TransportAction {
    fn icon(self) -> Icon {
        match self {
            TransportAction::Build => Icon::at("hammer"),
            TransportAction::Play => Icon::godot("Play"),
            TransportAction::Pause => Icon::godot("Pause"),
            TransportAction::Stop => Icon::godot("Stop"),
            TransportAction::Reload => Icon::godot("Reload"),
        }
    }
}

/// Everything between the app icon and the window buttons.
pub fn top_bar_content() -> impl Scene {
    bsn! {
        Node {
            flex_grow: 1.0,
            min_width: px(0),
            align_self: AlignSelf::Stretch,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(8),
            padding: UiRect::right(px(8)),
        }
        WindowDragArea
        Children [
            @menus()
            --
            @document_tabs()
            --
            @level_picker()
            --
            @transport()
        ]
    }
}

fn menus() -> impl Scene {
    bsn! {
        Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
        Children [
            @menu("File") -- @menu("Edit") -- @menu("Add") -- @menu("View")
            -- @menu("Tools") -- @menu("Window") -- @menu("Help")
        ]
    }
}

fn menu(name: &'static str) -> impl Scene {
    bsn! {
        @FeathersButton {
            @caption: bsn! { @caption(name) },
            @variant: ButtonVariant::Plain,
        }
        on(move |_: On<Activate>| info!("{name} menu: not built yet"))
    }
}

/// The open level as a tab, with the unsaved-changes dot, and a "+" for new levels.
/// Prefabs opened in isolation will get tabs here too.
/// The empty space after the tabs drags the window.
fn document_tabs() -> impl Scene {
    bsn! {
        Node {
            flex_grow: 1.0,
            min_width: px(0),
            align_self: AlignSelf::Stretch,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(4),
        }
        WindowDragArea
        Children [
            Node {
                height: px(CONTROL_HEIGHT),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(6),
                padding: UiRect::horizontal(px(8)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(RADIUS)),
            }
            LevelTab
            HeaderControl
            Children [
                UiIcon { icon: Icon::at("mountains"), size: ICON }
                Pickable::IGNORE
                --
                Text("Untitled")
                LevelTabLabel
                TextFont {
                    font: FontSourceTemplate::Handle(paths::UI),
                    font_size: FontSize::Px(12.0),
                }
                TextColor({Color::srgba(1.0, 1.0, 1.0, 0.85)})
                Pickable::IGNORE
            ]
            --
            @icon_button(Icon::godot("Add"))
            on(|_: On<Activate>| info!("New level: arrives with levels and prefabs (Phase 3)"))
        ]
    }
}

/// Which level Play runs.
fn level_picker() -> impl Scene {
    bsn! {
        Node {
            min_width: px(120),
            height: px(CONTROL_HEIGHT),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(6),
            padding: UiRect::horizontal(px(8)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(RADIUS)),
        }
        HeaderControl
        HoverBackground
        on(hover_on) on(hover_off) on(hover_press) on(hover_release)
        on(|click: On<PointerClick>| {
            if click.button == PointerButton::Primary {
                info!("Level picker: arrives with levels and prefabs (Phase 3)");
            }
        })
        Children [
            Text("Main level")
            TextFont {
                font: FontSourceTemplate::Handle(paths::UI),
                font_size: FontSize::Px(12.0),
            }
            TextColor({Color::srgba(1.0, 1.0, 1.0, 0.85)})
            Pickable::IGNORE
            --
            Node { flex_grow: 1.0 } Pickable::IGNORE
            --
            UiIcon { icon: Icon::godot("GuiTreeArrowDown"), size: 12.0 }
            Pickable::IGNORE
        ]
    }
}

/// The build / play pill.
fn transport() -> impl Scene {
    bsn! {
        Node {
            height: px(CONTROL_HEIGHT),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(2),
            padding: UiRect::horizontal(px(3)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(RADIUS)),
        }
        HeaderControl
        Children [
            @transport_button(TransportAction::Build)
            --
            @transport_divider()
            --
            @transport_button(TransportAction::Play)
            --
            @icon_button(Icon::godot("GuiTreeArrowDown"))
            on(|_: On<Activate>| info!("Run configurations: not built yet"))
            --
            @transport_button(TransportAction::Pause)
            --
            @transport_button(TransportAction::Stop)
            --
            @transport_button(TransportAction::Reload)
            --
            @transport_divider()
            --
            @window_mode_button()
        ]
    }
}

fn transport_divider() -> impl Scene {
    bsn! {
        Node { width: px(1), height: px(12), margin: UiRect::horizontal(px(3)) }
        BackgroundColor({Color::srgba(1.0, 1.0, 1.0, 0.12)})
    }
}

fn transport_button(action: TransportAction) -> impl Scene {
    bsn! {
        @icon_button(action.icon())
        TransportButton(action)
        on(move |_: On<Activate>, mut actions: MessageWriter<TransportAction>| {
            actions.write(action);
        })
    }
}

/// Embedded / Windowed toggle for where Play runs.
fn window_mode_button() -> impl Scene {
    bsn! {
        Node {
            height: px(CONTROL_HEIGHT - 4.0),
            padding: UiRect::horizontal(px(6)),
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(px(RADIUS - 1.0)),
        }
        HoverBackground
        on(hover_on) on(hover_off) on(hover_press) on(hover_release)
        on(|click: On<PointerClick>, mut mode: ResMut<PlayWindow>| {
            if click.button == PointerButton::Primary {
                *mode = match *mode {
                    PlayWindow::Embedded => PlayWindow::Windowed,
                    PlayWindow::Windowed => PlayWindow::Embedded,
                };
            }
        })
        Children [
            Text("Embedded")
            WindowModeLabel
            TextFont {
                font: FontSourceTemplate::Handle(paths::UI),
                font_size: FontSize::Px(11.0),
            }
            TextColor({Color::srgba(1.0, 1.0, 1.0, 0.55)})
            Pickable::IGNORE
        ]
    }
}

/// A small square ghost button with one icon. Sends `Activate` on click,
/// like Feathers buttons, so callers attach `on(|_: On<Activate>| ..)`.
fn icon_button(icon: Icon) -> impl Scene {
    bsn! {
        Node {
            width: px(ICON_BUTTON),
            height: px(CONTROL_HEIGHT - 4.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(px(RADIUS - 1.0)),
        }
        HoverBackground
        on(hover_on) on(hover_off) on(hover_press) on(hover_release)
        on(|click: On<PointerClick>, mut commands: Commands| {
            if click.button == PointerButton::Primary {
                commands.trigger(Activate { entity: click.event_target() });
            }
        })
        Children [
            UiIcon { icon: {icon.clone()}, size: ICON }
            Pickable::IGNORE
        ]
    }
}

/// A pill or tab in the bar: filled and outlined in the theme's button colours.
#[derive(Component, Default, Clone)]
struct HeaderControl;

/// Gets a subtle background on hover and press, then returns to `base`.
#[derive(Component, Clone)]
struct HoverBackground {
    base: Color,
}

impl Default for HoverBackground {
    fn default() -> Self {
        Self { base: Color::NONE }
    }
}

#[derive(Component, Default, Clone)]
struct LevelTab;

#[derive(Component, Default, Clone)]
struct LevelTabLabel;

#[derive(Component, Default, Clone)]
struct WindowModeLabel;

fn hover_on(
    over: On<PointerOver>,
    palette: Res<ActivePalette>,
    mut backgrounds: Query<&mut BackgroundColor, With<HoverBackground>>,
) {
    if let Ok(mut bg) = backgrounds.get_mut(over.event_target()) {
        bg.0 = palette.flat_hover;
    }
}

fn hover_off(
    out: On<PointerOut>,
    mut backgrounds: Query<(&mut BackgroundColor, &HoverBackground)>,
) {
    if let Ok((mut bg, hover)) = backgrounds.get_mut(out.event_target()) {
        bg.0 = hover.base;
    }
}

fn hover_press(
    press: On<PointerPress>,
    palette: Res<ActivePalette>,
    mut backgrounds: Query<&mut BackgroundColor, With<HoverBackground>>,
) {
    if let Ok(mut bg) = backgrounds.get_mut(press.event_target()) {
        bg.0 = palette.flat_pressed;
    }
}

fn hover_release(
    release: On<PointerRelease>,
    palette: Res<ActivePalette>,
    mut backgrounds: Query<&mut BackgroundColor, With<HoverBackground>>,
) {
    if let Ok(mut bg) = backgrounds.get_mut(release.event_target()) {
        bg.0 = palette.flat_hover;
    }
}

/// Paint pills once, from the theme (they are spawned before the palette is read).
fn style_header_controls(
    palette: Res<ActivePalette>,
    mut controls: Query<
        (
            &mut BackgroundColor,
            &mut BorderColor,
            Option<&mut HoverBackground>,
        ),
        Added<HeaderControl>,
    >,
) {
    for (mut bg, mut border, hover) in &mut controls {
        bg.0 = palette.button_normal;
        *border = BorderColor::all(palette.button_border);
        // Pills that also react to hover go back to their fill, not to clear.
        if let Some(mut hover) = hover {
            hover.base = palette.button_normal;
        }
    }
}

fn handle_transport(
    mut actions: MessageReader<TransportAction>,
    mut state: ResMut<PlayState>,
    mode: Res<PlayWindow>,
    project: Res<CurrentProject>,
) {
    for action in actions.read() {
        let name = project.name().unwrap_or("the project");
        match action {
            TransportAction::Build => {
                info!("Build {name}: the project build pipeline arrives with scripting");
            }
            TransportAction::Play => {
                if *state != PlayState::Playing {
                    info!("Play {name} ({mode:?})");
                    *state = PlayState::Playing;
                }
            }
            TransportAction::Pause => match *state {
                PlayState::Playing => *state = PlayState::Paused,
                PlayState::Paused => *state = PlayState::Playing,
                PlayState::Stopped => {}
            },
            TransportAction::Stop => {
                if *state != PlayState::Stopped {
                    info!("Stop");
                    *state = PlayState::Stopped;
                }
            }
            TransportAction::Reload => {
                if *state != PlayState::Stopped {
                    info!("Hot reload: arrives with scripting");
                }
            }
        }
    }
}

/// Play glows while running; Pause glows while paused; Pause, Stop and
/// Reload are dimmed while stopped.
fn update_transport_buttons(
    state: Res<PlayState>,
    palette: Res<ActivePalette>,
    buttons: Query<(Entity, &TransportButton, &Children)>,
    added: Query<(), Added<TransportButton>>,
    mut backgrounds: Query<(&mut BackgroundColor, &mut HoverBackground)>,
    mut images: Query<&mut ImageNode>,
) {
    if !state.is_changed() && added.is_empty() {
        return;
    }
    for (entity, TransportButton(action), children) in &buttons {
        let active = matches!(
            (*state, action),
            (PlayState::Playing, TransportAction::Play)
                | (PlayState::Paused, TransportAction::Pause)
        );
        let enabled = match action {
            TransportAction::Build | TransportAction::Play => true,
            TransportAction::Pause | TransportAction::Stop | TransportAction::Reload => {
                *state != PlayState::Stopped
            }
        };
        if let Ok((mut bg, mut hover)) = backgrounds.get_mut(entity) {
            hover.base = if active {
                palette.highlight
            } else {
                Color::NONE
            };
            bg.0 = hover.base;
        }
        let tint = if active && *action == TransportAction::Play {
            palette.success
        } else if enabled {
            Color::WHITE
        } else {
            Color::srgba(1.0, 1.0, 1.0, 0.35)
        };
        for child in children {
            if let Ok(mut image) = images.get_mut(*child) {
                image.color = tint;
            }
        }
    }
}

fn update_window_mode_label(
    mode: Res<PlayWindow>,
    mut labels: Query<&mut Text, With<WindowModeLabel>>,
) {
    if !mode.is_changed() {
        return;
    }
    let label = match *mode {
        PlayWindow::Embedded => "Embedded",
        PlayWindow::Windowed => "Windowed",
    };
    for mut text in &mut labels {
        if text.0 != label {
            text.0 = label.to_owned();
        }
    }
}

/// The tab shows the main level's name, with a dot when there are unsaved changes.
fn update_level_tab(
    history: Res<History>,
    project: Res<CurrentProject>,
    mut labels: Query<&mut Text, With<LevelTabLabel>>,
    added: Query<(), Added<LevelTabLabel>>,
) {
    if !history.is_changed() && !project.is_changed() && added.is_empty() {
        return;
    }
    let level = project
        .0
        .as_ref()
        .and_then(|p| p.file.editor.main_level.as_deref())
        .and_then(|path| std::path::Path::new(path).file_stem())
        .and_then(|stem| stem.to_str())
        .unwrap_or("Untitled")
        .to_owned();
    let label = if history.is_dirty() {
        format!("{level} •")
    } else {
        level
    };
    for mut text in &mut labels {
        if text.0 != label {
            text.0.clone_from(&label);
        }
    }
}
