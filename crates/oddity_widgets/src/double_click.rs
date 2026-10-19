//! Double-clicks on [`tree`](crate::tree) rows and
//! [`item_grid`](crate::item_grid) cells, counted by key.
//!
//! Bevy counts clicks per entity (`PointerClick::count`). These widgets
//! rebuild every row when the owner's list changes, and the first click of a
//! double-click usually changes the selection, so the second click lands on
//! a freshly spawned entity and Bevy calls it a first click. Counting by
//! widget and key survives the rebuild.

use core::time::Duration;

use bevy::{picking::PickingSettings, platform::time::Instant, prelude::*};

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<LastClick>();
}

/// The last primary click on a row or cell, and when it happened.
#[derive(Resource, Default)]
pub(crate) struct LastClick(Option<(Entity, u64, Instant)>);

impl LastClick {
    /// Record a click on `key` in `widget`. True when it completes a
    /// double-click, within the same interval Bevy's picking uses.
    pub(crate) fn is_double(
        &mut self,
        settings: &PickingSettings,
        widget: Entity,
        key: u64,
    ) -> bool {
        self.is_double_at(Instant::now(), settings.multi_click_interval, widget, key)
    }

    fn is_double_at(&mut self, now: Instant, interval: Duration, widget: Entity, key: u64) -> bool {
        match self.0.take() {
            // Forgetting the click means a third one starts a new pair.
            Some((last_widget, last_key, at))
                if last_widget == widget && last_key == key && now - at <= interval =>
            {
                true
            }
            _ => {
                self.0 = Some((widget, key, now));
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTERVAL: Duration = Duration::from_millis(500);

    #[test]
    fn two_quick_clicks_on_the_same_key_make_a_double_click() {
        let widget = Entity::from_raw_u32(1).unwrap();
        let start = Instant::now();
        let mut last = LastClick::default();
        assert!(!last.is_double_at(start, INTERVAL, widget, 7));
        assert!(last.is_double_at(start + Duration::from_millis(200), INTERVAL, widget, 7));
        assert!(
            !last.is_double_at(start + Duration::from_millis(300), INTERVAL, widget, 7),
            "a third click starts over"
        );
    }

    #[test]
    fn slow_clicks_or_other_keys_are_single_clicks() {
        let widget = Entity::from_raw_u32(1).unwrap();
        let start = Instant::now();
        let mut last = LastClick::default();
        last.is_double_at(start, INTERVAL, widget, 7);
        assert!(!last.is_double_at(start + Duration::from_millis(100), INTERVAL, widget, 8));
        assert!(!last.is_double_at(start + Duration::from_secs(1), INTERVAL, widget, 8));
    }
}
