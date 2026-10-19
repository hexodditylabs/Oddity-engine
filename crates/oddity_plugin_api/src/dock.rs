//! Dock tabs: what each of the editor's docks shows.
//!
//! The editor has four docks with tabs, laid out as in Godot: Scene (top
//! left), FileSystem (bottom left), Inspector (right) and the bottom panel.
//! Each tab is registered with [`EditorAppExt::add_dock_tab`] and the editor
//! builds the docks from [`DockTabs`] when it opens.
//!
//! [`EditorAppExt::add_dock_tab`]: crate::EditorAppExt::add_dock_tab

use std::{borrow::Cow, sync::Arc};

use bevy::prelude::*;

/// Which dock a tab goes in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DockSlot {
    /// Top left: Hierarchy, Import.
    Scene,
    /// Bottom left: FileSystem, History.
    FileSystem,
    /// Right: Inspector, Signals.
    Inspector,
    /// Godot's bottom panel, under the viewport: Files, Output.
    Bottom,
}

/// Builds a tab's page each time the dock is built.
type PageFn = Arc<dyn Fn() -> Box<dyn Scene> + Send + Sync>;

/// A tab in one of the docks: where it goes, its title, its place among the
/// dock's tabs, and the page it shows.
#[derive(Clone)]
pub struct DockTab {
    /// The dock it goes in.
    pub slot: DockSlot,
    /// The text on the tab.
    pub title: Cow<'static, str>,
    /// Lower comes first. The editor's own tabs use 0, 100, 200, ..., so an
    /// extension can go between them; equal orders keep the order they were
    /// added in.
    pub order: i32,
    page: PageFn,
}

impl DockTab {
    /// A tab in `slot` showing the scene `page` builds, at order 0.
    pub fn new<S: Scene>(
        slot: DockSlot,
        title: impl Into<Cow<'static, str>>,
        page: impl Fn() -> S + Send + Sync + 'static,
    ) -> Self {
        Self {
            slot,
            title: title.into(),
            order: 0,
            page: Arc::new(move || Box::new(page())),
        }
    }

    /// Place the tab among the dock's others. See [`DockTab::order`].
    pub fn order(mut self, order: i32) -> Self {
        self.order = order;
        self
    }

    /// A fresh copy of the tab's page.
    pub fn page(&self) -> Box<dyn Scene> {
        (self.page)()
    }
}

impl std::fmt::Debug for DockTab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DockTab")
            .field("slot", &self.slot)
            .field("title", &self.title)
            .field("order", &self.order)
            .finish_non_exhaustive()
    }
}

/// Every registered dock tab.
#[derive(Resource, Default, Debug)]
pub struct DockTabs {
    tabs: Vec<DockTab>,
}

impl DockTabs {
    /// Add a tab.
    pub fn add(&mut self, tab: DockTab) {
        self.tabs.push(tab);
    }

    /// The tabs of one dock, in the order they are shown.
    pub fn in_slot(&self, slot: DockSlot) -> Vec<&DockTab> {
        let mut tabs: Vec<&DockTab> = self.tabs.iter().filter(|tab| tab.slot == slot).collect();
        // Stable, so equal orders keep the order they were added in.
        tabs.sort_by_key(|tab| tab.order);
        tabs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(tabs: &DockTabs, slot: DockSlot) -> Vec<&str> {
        tabs.in_slot(slot)
            .iter()
            .map(|tab| tab.title.as_ref())
            .collect()
    }

    #[test]
    fn tabs_sort_by_order_then_by_when_they_were_added() {
        let mut tabs = DockTabs::default();
        tabs.add(DockTab::new(DockSlot::Scene, "Import", || bsn! {}).order(100));
        tabs.add(DockTab::new(DockSlot::Scene, "Hierarchy", || bsn! {}));
        tabs.add(DockTab::new(DockSlot::Scene, "Physics", || bsn! {}).order(100));
        tabs.add(DockTab::new(DockSlot::Inspector, "Inspector", || bsn! {}));
        assert_eq!(
            titles(&tabs, DockSlot::Scene),
            ["Hierarchy", "Import", "Physics"]
        );
        assert_eq!(titles(&tabs, DockSlot::Inspector), ["Inspector"]);
        assert!(titles(&tabs, DockSlot::Bottom).is_empty());
    }
}
