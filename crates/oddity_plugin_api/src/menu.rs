//! The menu bar: top-level menus and the items in them.
//!
//! A menu item is a label and a registered command id, nothing else, so
//! whatever a menu can do, the CLI, MCP and scripts can do too
//! (`roadmap/commands.md`, rule 1). Version 0 has one level of items under
//! each menu.

use std::borrow::Cow;

use bevy::prelude::*;

/// One entry in a menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    /// The text shown.
    pub label: Cow<'static, str>,
    /// The id of the command it runs, with no arguments.
    pub command: Cow<'static, str>,
}

/// A top-level menu and its items, in the order they were added.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    /// The name in the menu bar.
    pub name: Cow<'static, str>,
    /// Its entries.
    pub items: Vec<MenuItem>,
}

/// Every registered menu, in menu-bar order.
#[derive(Resource, Clone, Debug, Default)]
pub struct MenuBar {
    menus: Vec<Menu>,
}

impl MenuBar {
    /// The menus, left to right.
    pub fn menus(&self) -> &[Menu] {
        &self.menus
    }

    /// Add a top-level menu at the right end, unless it is already there.
    pub fn add_menu(&mut self, name: impl Into<Cow<'static, str>>) {
        self.menu_mut(name.into());
    }

    /// Add an item at `path` (`"Menu/Item"`) that runs `command`. A menu
    /// that doesn't exist yet is added at the right end.
    ///
    /// # Panics
    ///
    /// If `path` has no `/`: every item belongs to a menu.
    pub fn add_item(&mut self, path: &str, command: impl Into<Cow<'static, str>>) {
        let Some((menu, label)) = path.split_once('/') else {
            panic!("menu item `{path}` needs a menu: write it as \"Menu/Item\"");
        };
        self.menu_mut(Cow::Owned(menu.to_owned()))
            .items
            .push(MenuItem {
                label: Cow::Owned(label.to_owned()),
                command: command.into(),
            });
    }

    fn menu_mut(&mut self, name: Cow<'static, str>) -> &mut Menu {
        let index = match self.menus.iter().position(|menu| menu.name == name) {
            Some(index) => index,
            None => {
                self.menus.push(Menu {
                    name,
                    items: Vec::new(),
                });
                self.menus.len() - 1
            }
        };
        &mut self.menus[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_go_under_their_menu_and_new_menus_go_last() {
        let mut bar = MenuBar::default();
        bar.add_menu("File");
        bar.add_menu("Edit");
        bar.add_item("Edit/Undo", "history.undo");
        bar.add_item("Physics/Fit Shape", "physics.fit_shape");
        bar.add_menu("File");

        let names: Vec<&str> = bar.menus().iter().map(|m| m.name.as_ref()).collect();
        assert_eq!(names, ["File", "Edit", "Physics"]);
        assert_eq!(
            bar.menus()[1].items,
            [MenuItem {
                label: "Undo".into(),
                command: "history.undo".into(),
            }]
        );
    }

    #[test]
    #[should_panic(expected = "needs a menu")]
    fn an_item_without_a_menu_is_a_mistake() {
        MenuBar::default().add_item("Undo", "history.undo");
    }
}
