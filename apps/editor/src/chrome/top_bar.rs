//! What sits inside the title bar, laid out after jackdaw's editor:
//!
//! `menus · level/prefab tabs ·········· level picker · [build play ⌄ pause stop reload | Embedded]`
//!
//! Oddity uses Unity's model: **levels** are the worlds you play, **prefabs**
//! are reusable objects, opened in isolation in their own tab (like Unity's
//! Prefab Mode and Unreal's Blueprint editor).
//!
//! The hammer runs `build.run` (see `build.rs`) and shows how the build is
//! going; the play controls drive [`PlayState`], and running the game itself
//! arrives with Play (scripting task E1). Everything here is presentation plus a
//! little state, so later phases only have to react to [`TransportAction`].
//!
//! The menus are the ones registered through `oddity_plugin_api`, and each
//! item runs its command by id, so a menu can do nothing the CLI, MCP and
//! scripts can't.

use bevy::ecs::message::Messages;
use bevy::{
    feathers::{
        controls::{
            ButtonVariant, FeathersButton, FeathersMenu, FeathersMenuItem, FeathersMenuPopup,
        },
        display::caption,
    },
    picking::{Pickable, events::PointerClick, pointer::PointerButton},
    prelude::*,
    text::FontSourceTemplate,
    ui_widgets::{Activate, MenuButton},
};
use oddity_core::{Args, CommandError, CommandExecuted, CommandSpec, EditorWorldExt, History};
use oddity_icons::{Icon, UiIcon};
use oddity_plugin_api::{EditorAppExt, EditorMode, Menu, MenuBar, MenuItem};
use oddity_widgets::{
    ActivePalette,
    button::icon_button,
    fonts::paths,
    hover::{HoverFill, hover_fill_out, hover_fill_over, hover_fill_press, hover_fill_release},
};

use super::title_bar::WindowDragArea;
use crate::{
    actions::{Run, UiAction, run_command, ui_state},
    build::{BuildStatus, Builds},
    project::CurrentProject,
};

/// Height of the controls in the bar (jackdaw uses 22).
const CONTROL_HEIGHT: f32 = 22.0;
/// Icon size inside controls.
const ICON: f32 = 14.0;
/// Corner radius of pills and buttons.
const RADIUS: f32 = 5.0;

/// The menus, document tabs, level picker and play controls.
pub struct TopBarPlugin;

impl Plugin for TopBarPlugin {
    fn build(&self, app: &mut App) {
        if app.editor_mode() == EditorMode::Window {
            // The menu bar, left to right. Features add their items to these.
            for menu in ["File", "Edit", "Add", "View", "Tools", "Window", "Help"] {
                app.add_menu(menu);
            }
        }
        app.register_command::<Run<Start>>()
            .register_command::<Run<Pause>>()
            .register_command::<Run<Stop>>()
            .register_command::<Run<Reload>>()
            .register_command::<Run<ToggleWindow>>();
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
                    paint_header_details,
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
    /// The command its button runs.
    fn command(self) -> &'static str {
        match self {
            TransportAction::Build => crate::build::BUILD_RUN,
            TransportAction::Play => Start::SPEC.id,
            TransportAction::Pause => Pause::SPEC.id,
            TransportAction::Stop => Stop::SPEC.id,
            TransportAction::Reload => Reload::SPEC.id,
        }
    }

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
pub fn top_bar_content(menus: MenuBar) -> impl Scene {
    bsn! {
        Node {
            flex_grow: 1.0,
            min_width: px(0),
            align_self: AlignSelf::Stretch,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(8),
            padding: UiRect::right(px(8)),
            // In a narrow window the controls are cut off at the right
            // instead of pushing the window buttons off screen.
            overflow: Overflow::clip(),
        }
        WindowDragArea
        Children [
            @menu_bar(menus) Node { flex_shrink: 0.0 }
            --
            @document_tabs() Node { flex_shrink: 0.0 }
            --
            @level_picker() Node { flex_shrink: 0.0 }
            --
            @transport() Node { flex_shrink: 0.0 }
        ]
    }
}

fn menu_bar(bar: MenuBar) -> impl Scene {
    let menus: Vec<_> = bar.menus().iter().map(menu).collect();
    bsn! {
        Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
        Children [ {menus} ]
    }
}

/// A menu's name in the bar. A menu with items opens them below it; one
/// with none yet is just its name.
fn menu(menu: &Menu) -> Box<dyn Scene> {
    let name = menu.name.clone().into_owned();
    let button = bsn! {
        @FeathersButton {
            @caption: bsn! { @caption(name) },
            @variant: ButtonVariant::Plain,
        }
    };
    if menu.items.is_empty() {
        return Box::new(button);
    }
    let items: Vec<_> = menu.items.iter().map(menu_item).collect();
    Box::new(bsn! {
        @FeathersMenu
        Children [
            @{button} MenuButton
            --
            @FeathersMenuPopup
            Children [ {items} ]
        ]
    })
}

fn menu_item(item: &MenuItem) -> impl Scene {
    let label = item.label.clone().into_owned();
    let command = item.command.clone();
    bsn! {
        @FeathersMenuItem { @caption: bsn! { @caption(label) } }
        on(move |_: On<Activate>, mut commands: Commands| {
            let command = command.clone();
            commands.queue(move |world: &mut World| {
                if let Err(error) = world.run_command(&command, serde_json::json!({})) {
                    warn!("{command}: {error}");
                }
            });
        })
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
                HeaderText
                TextFont {
                    font: FontSourceTemplate::Handle(paths::UI),
                    font_size: FontSize::Px(12.0),
                }
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
        HoverFill
        on(hover_fill_over) on(hover_fill_out) on(hover_fill_press) on(hover_fill_release)
        on(|click: On<PointerClick>| {
            if click.button == PointerButton::Primary {
                info!("Level picker: arrives with levels and prefabs (Phase 3)");
            }
        })
        Children [
            Text("Main level")
            HeaderText
            TextFont {
                font: FontSourceTemplate::Handle(paths::UI),
                font_size: FontSize::Px(12.0),
            }
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
        TransportDivider
        BackgroundColor
    }
}

fn transport_button(action: TransportAction) -> impl Scene {
    bsn! {
        @icon_button(action.icon())
        TransportButton(action)
        on(move |_: On<Activate>, mut commands: Commands| {
            run_command(&mut commands, action.command(), serde_json::json!({}));
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
        HoverFill
        on(hover_fill_over) on(hover_fill_out) on(hover_fill_press) on(hover_fill_release)
        on(|click: On<PointerClick>, mut commands: Commands| {
            if click.button == PointerButton::Primary {
                run_command(&mut commands, ToggleWindow::SPEC.id, serde_json::json!({}));
            }
        })
        Children [
            Text("Embedded")
            WindowModeLabel
            TextFont {
                font: FontSourceTemplate::Handle(paths::UI),
                font_size: FontSize::Px(11.0),
            }
            Pickable::IGNORE
        ]
    }
}

/// A pill or tab in the bar: filled and outlined in the theme's button colours.
#[derive(Component, Default, Clone)]
struct HeaderControl;

#[derive(Component, Default, Clone)]
struct LevelTab;

#[derive(Component, Default, Clone)]
struct LevelTabLabel;

#[derive(Component, Default, Clone)]
struct WindowModeLabel;

/// Text in the bar's pills.
#[derive(Component, Default, Clone)]
struct HeaderText;

/// The thin line between groups of play controls.
#[derive(Component, Default, Clone)]
struct TransportDivider;

/// Colour the bar's text and dividers from the theme as they appear.
fn paint_header_details(
    palette: Res<ActivePalette>,
    mut texts: Query<
        (&mut TextColor, Has<WindowModeLabel>),
        Or<(Added<HeaderText>, Added<WindowModeLabel>)>,
    >,
    mut dividers: Query<&mut BackgroundColor, Added<TransportDivider>>,
) {
    for (mut colour, secondary) in &mut texts {
        colour.0 = if secondary {
            palette.font_secondary
        } else {
            palette.font
        };
    }
    for mut background in &mut dividers {
        background.0 = palette.button_border;
    }
}

/// Send a play control's message: what the transport buttons do.
fn send_transport(
    world: &mut World,
    action: TransportAction,
    id: &str,
) -> Result<(), CommandError> {
    if !world.contains_resource::<Messages<TransportAction>>() {
        return Err(CommandError::Invalid(format!(
            "{id} needs the editor window"
        )));
    }
    world.write_message(action);
    Ok(())
}

/// One `play.*` command per play control.
macro_rules! transport_command {
    ($name:ident, $action:expr, $id:literal, $description:literal) => {
        #[doc = concat!("`", $id, "`.")]
        struct $name;

        impl UiAction for $name {
            const SPEC: CommandSpec = CommandSpec::action($id, $description, &[]);

            fn run(world: &mut World, _args: &Args) -> Result<(), CommandError> {
                send_transport(world, $action, Self::SPEC.id)
            }
        }
    };
}

transport_command!(
    Start,
    TransportAction::Play,
    "play.start",
    "Run the game, or resume it."
);
transport_command!(
    Pause,
    TransportAction::Pause,
    "play.pause",
    "Pause or unpause the running game."
);
transport_command!(
    Stop,
    TransportAction::Stop,
    "play.stop",
    "Stop the game and go back to editing."
);
transport_command!(
    Reload,
    TransportAction::Reload,
    "play.reload",
    "Hot-reload the running game."
);

/// `play.toggle_window`.
struct ToggleWindow;

impl UiAction for ToggleWindow {
    const SPEC: CommandSpec = CommandSpec::action(
        "play.toggle_window",
        "Switch where Play runs the game: inside the editor's viewport or in its own window.",
        &[],
    );

    fn run(world: &mut World, _args: &Args) -> Result<(), CommandError> {
        let mut mode = ui_state::<PlayWindow>(world, Self::SPEC.id)?;
        *mode = match *mode {
            PlayWindow::Embedded => PlayWindow::Windowed,
            PlayWindow::Windowed => PlayWindow::Embedded,
        };
        Ok(())
    }
}

/// Paint pills once, from the theme (they are spawned before the palette is read).
fn style_header_controls(
    palette: Res<ActivePalette>,
    mut controls: Query<
        (
            &mut BackgroundColor,
            &mut BorderColor,
            Option<&mut HoverFill>,
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
            // The hammer runs `build.run` itself.
            TransportAction::Build => {}
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
/// Reload are dimmed while stopped. The hammer glows while building, and is
/// red when the code doesn't build.
fn update_transport_buttons(
    state: Res<PlayState>,
    builds: Res<Builds>,
    mut shown: Local<Option<BuildStatus>>,
    palette: Res<ActivePalette>,
    buttons: Query<(Entity, &TransportButton, &Children)>,
    added: Query<(), Added<TransportButton>>,
    mut backgrounds: Query<(&mut BackgroundColor, &mut HoverFill)>,
    mut images: Query<&mut ImageNode>,
) {
    // `Builds` changes with every line of output; the hammer only with the
    // status.
    let status_changed = *shown != Some(builds.status);
    if !state.is_changed() && !status_changed && added.is_empty() {
        return;
    }
    *shown = Some(builds.status);
    for (entity, TransportButton(action), children) in &buttons {
        let active = matches!(
            (*state, action, builds.status),
            (PlayState::Playing, TransportAction::Play, _)
                | (PlayState::Paused, TransportAction::Pause, _)
                | (_, TransportAction::Build, BuildStatus::Building)
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
        } else if *action == TransportAction::Build && builds.status == BuildStatus::Failed {
            palette.error
        } else if enabled {
            palette.font
        } else {
            palette.font_disabled
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
    mut executed: MessageReader<CommandExecuted>,
    history: Res<History>,
    project: Res<CurrentProject>,
    mut labels: Query<&mut Text, With<LevelTabLabel>>,
    added: Query<(), Added<LevelTabLabel>>,
) {
    // Only a command can change what is unsaved, whoever ran it.
    if executed.read().count() == 0 && !project.is_changed() && added.is_empty() {
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

#[cfg(test)]
mod tests {
    use oddity_core::{
        CommandRegistry, EditorWorldExt, LevelRoot, Source, commands::RenameEntity, headless_app,
    };

    use super::*;

    #[test]
    fn every_menu_item_runs_a_registered_command() {
        let mut app = headless_app();
        app.insert_resource(EditorMode::Window)
            .add_plugins((TopBarPlugin, crate::docks::DocksPlugin));
        let bar = app.world().resource::<MenuBar>();
        let names: Vec<&str> = bar.menus().iter().map(|m| m.name.as_ref()).collect();
        assert_eq!(
            names,
            ["File", "Edit", "Add", "View", "Tools", "Window", "Help"]
        );
        let registry = app.world().resource::<CommandRegistry>();
        let items: Vec<&MenuItem> = bar.menus().iter().flat_map(|m| &m.items).collect();
        assert!(!items.is_empty());
        for item in items {
            assert!(
                registry.spec(&item.command).is_some(),
                "{} runs `{}`, which isn't registered",
                item.label,
                item.command
            );
        }
    }

    #[test]
    fn headless_has_no_menus() {
        let mut app = headless_app();
        app.add_plugins((TopBarPlugin, crate::docks::DocksPlugin));
        assert!(app.world().get_resource::<MenuBar>().is_none());
    }

    #[test]
    fn the_unsaved_dot_follows_commands_from_any_source() {
        let mut app = headless_app();
        app.init_resource::<CurrentProject>()
            .add_systems(Update, update_level_tab);
        let world = app.world_mut();
        let label = world.spawn((Text::default(), LevelTabLabel)).id();
        let root = world.spawn((Name::new("Main"), LevelRoot)).id();
        let shown = |app: &App| app.world().get::<Text>(label).unwrap().0.clone();

        app.update();
        assert_eq!(shown(&app), "Untitled");

        let agent = Source::Mcp {
            client: "Claude".into(),
        };
        app.world_mut()
            .execute_as(RenameEntity::new(root, "Level"), agent)
            .unwrap();
        app.update();
        assert_eq!(shown(&app), "Untitled •");

        app.world_mut().undo().unwrap();
        app.update();
        assert_eq!(shown(&app), "Untitled");
    }
}
