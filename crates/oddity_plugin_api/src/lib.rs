//! The door into the Oddity editor (`roadmap/modularity.md`).
//!
//! An extension is a plain Bevy plugin. Anything Bevy can already do
//! (systems, observers, gizmos) it does directly; this crate covers only the
//! places where the editor has to *list* things, through [`EditorAppExt`] on
//! `App`:
//!
//! - commands, queries and node types, added in every mode, so the CLI, MCP
//!   and tests can use them;
//! - dock tabs, menu items and Inspector fields, added only when the editor
//!   has a window.
//!
//! ```ignore
//! impl Plugin for PhysicsEditorPlugin {
//!     fn build(&self, app: &mut App) {
//!         // Always: the CLI, MCP and tests can use physics too.
//!         app.register_command::<FitShape>(); // physics.fit_shape
//!
//!         // Only with a window.
//!         if app.editor_mode() == EditorMode::Window {
//!             app.add_menu_item("Tools/Fit Shape to Mesh", "physics.fit_shape")
//!                 .add_dock_tab(DockTab::new(DockSlot::Bottom, "Physics", physics_page));
//!         }
//!     }
//! }
//! ```
//!
//! The editor's own docks and menus register the same way, so a third-party
//! plugin gets nothing less than a built-in feature.
//!
//! This is version 0: internal and unstable until Phase 6 makes it the
//! public `EditorPlugin` API. Its names are a future public contract.

pub mod dock;
pub mod inspector;
pub mod menu;
pub mod nodes;

use std::borrow::Cow;

use bevy::prelude::*;
use oddity_core::{CommandRegistry, CommandSpec, NamedCommand, QueryFn};

pub use dock::{DockSlot, DockTab, DockTabs};
pub use inspector::{FieldTarget, InspectorField, InspectorFields};
pub use menu::{Menu, MenuBar, MenuItem};
pub use nodes::{NodeType, NodeTypes};

/// Whether the editor has a window. The editor sets it before adding any
/// plugin; without it, plugins see [`EditorMode::Headless`].
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EditorMode {
    /// The editor window, with docks and menus.
    Window,
    /// No window: the CLI, MCP and tests. Only commands, queries and data.
    #[default]
    Headless,
}

/// Registering editor extensions on an `App`. See the crate docs.
pub trait EditorAppExt {
    /// Whether the editor has a window. Add UI only for [`EditorMode::Window`].
    fn editor_mode(&self) -> EditorMode;

    /// Add a command by its type, so it can be run by name (from menus, the
    /// CLI, MCP and scripts). Do this in every mode.
    fn register_command<C: NamedCommand>(&mut self) -> &mut Self;

    /// Add a read-only query. Do this in every mode.
    fn register_query(&mut self, spec: CommandSpec, run: QueryFn) -> &mut Self;

    /// Add a node type to Add Node and `node.create`, replacing one with the
    /// same name. Do this in every mode.
    fn register_node(&mut self, node_type: NodeType) -> &mut Self;

    /// Add a tab to one of the docks. Window mode only.
    fn add_dock_tab(&mut self, tab: DockTab) -> &mut Self;

    /// Draw every field of type `F::Value` in the Inspector with `F`.
    /// Window mode only.
    fn add_inspector_field<F: InspectorField>(&mut self) -> &mut Self;

    /// Add a top-level menu after the ones already there, if it is new.
    /// Window mode only.
    fn add_menu(&mut self, name: impl Into<Cow<'static, str>>) -> &mut Self;

    /// Add an item at `path` (`"Menu/Item"`) that runs `command`, a
    /// registered command id. Window mode only.
    fn add_menu_item(&mut self, path: &str, command: impl Into<Cow<'static, str>>) -> &mut Self;
}

impl EditorAppExt for App {
    fn editor_mode(&self) -> EditorMode {
        self.world()
            .get_resource::<EditorMode>()
            .copied()
            .unwrap_or_default()
    }

    fn register_command<C: NamedCommand>(&mut self) -> &mut Self {
        self.world_mut()
            .get_resource_or_insert_with(CommandRegistry::with_builtins)
            .register_named::<C>();
        self
    }

    fn register_query(&mut self, spec: CommandSpec, run: QueryFn) -> &mut Self {
        self.world_mut()
            .get_resource_or_insert_with(CommandRegistry::with_builtins)
            .register_query(spec, run);
        self
    }

    fn register_node(&mut self, node_type: NodeType) -> &mut Self {
        if !self.world().contains_resource::<NodeTypes>() {
            self.init_resource::<NodeTypes>();
            nodes::register(self);
        }
        self.world_mut().resource_mut::<NodeTypes>().add(node_type);
        self
    }

    fn add_dock_tab(&mut self, tab: DockTab) -> &mut Self {
        self.world_mut().get_resource_or_init::<DockTabs>().add(tab);
        self
    }

    fn add_inspector_field<F: InspectorField>(&mut self) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<InspectorFields>()
            .add::<F>();
        self
    }

    fn add_menu(&mut self, name: impl Into<Cow<'static, str>>) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<MenuBar>()
            .add_menu(name);
        self
    }

    fn add_menu_item(&mut self, path: &str, command: impl Into<Cow<'static, str>>) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<MenuBar>()
            .add_item(path, command);
        self
    }
}
