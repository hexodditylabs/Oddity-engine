//! P2's exit test: an extension's command is there in every mode, and its
//! tab and menu item only when the editor has a window.

use std::borrow::Cow;

use bevy::prelude::*;
use oddity_core::{
    Args, CommandError, CommandRegistry, CommandSpec, EditorCommand, EditorWorldExt, Invocation,
    NamedCommand, OddityCorePlugin,
};
use oddity_plugin_api::{DockSlot, DockTab, DockTabs, EditorAppExt, EditorMode, MenuBar};
use serde_json::json;

/// How many times `test.ping` ran.
#[derive(Resource, Default)]
struct Pings(u32);

struct Ping;

impl EditorCommand for Ping {
    fn label(&self) -> Cow<'_, str> {
        "Ping".into()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        world.get_resource_or_init::<Pings>().0 += 1;
        Ok(())
    }

    fn undo(&mut self, _world: &mut World) -> Result<(), CommandError> {
        Ok(())
    }

    fn invocation(&self) -> Invocation {
        Invocation::new("test.ping", json!({}))
    }

    fn undoable(&self) -> bool {
        false
    }
}

impl NamedCommand for Ping {
    const SPEC: CommandSpec = CommandSpec::action("test.ping", "Count a ping.", &[]);

    fn from_args(_world: &mut World, _args: &Args) -> Result<Self, CommandError> {
        Ok(Ping)
    }
}

struct PingPlugin;

impl Plugin for PingPlugin {
    fn build(&self, app: &mut App) {
        app.register_command::<Ping>();
        if app.editor_mode() == EditorMode::Window {
            app.add_menu_item("Tools/Ping", "test.ping")
                .add_dock_tab(DockTab::new(DockSlot::Bottom, "Pings", || bsn! {}));
        }
    }
}

fn app(mode: Option<EditorMode>) -> App {
    let mut app = App::new();
    if let Some(mode) = mode {
        app.insert_resource(mode);
    }
    app.add_plugins((OddityCorePlugin, PingPlugin));
    app
}

#[test]
fn with_a_window_the_tab_menu_item_and_command_appear() {
    let mut app = app(Some(EditorMode::Window));
    let tabs = app.world().resource::<DockTabs>();
    assert_eq!(tabs.in_slot(DockSlot::Bottom)[0].title, "Pings");
    let menus = app.world().resource::<MenuBar>().menus();
    assert_eq!(menus[0].name, "Tools");
    assert_eq!(menus[0].items[0].command, "test.ping");

    // The menu item's command runs by name, as the menu will run it.
    let command = menus[0].items[0].command.clone();
    app.world_mut().run_command(&command, json!({})).unwrap();
    assert_eq!(app.world().resource::<Pings>().0, 1);
}

#[test]
fn headless_adds_the_command_without_the_tab() {
    for mode in [Some(EditorMode::Headless), None] {
        let mut app = app(mode);
        assert!(app.world().get_resource::<DockTabs>().is_none());
        assert!(app.world().get_resource::<MenuBar>().is_none());
        assert!(
            app.world()
                .resource::<CommandRegistry>()
                .spec("test.ping")
                .is_some()
        );
        app.world_mut().run_command("test.ping", json!({})).unwrap();
    }
}

#[test]
fn commands_registered_before_the_core_plugin_are_kept() {
    let mut app = App::new();
    app.register_command::<Ping>().add_plugins(OddityCorePlugin);
    assert!(
        app.world()
            .resource::<CommandRegistry>()
            .spec("test.ping")
            .is_some()
    );
}
