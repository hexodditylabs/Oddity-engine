//! Search fields: Godot's "Filter Nodes" and "Filter Properties" boxes.
//!
//! A dark rounded field with a magnifier on the right and dim placeholder
//! text while it is empty. Bevy 0.20's text input has no placeholder of its
//! own (it is on its list of planned features), so the field lays one over
//! the input and hides it once something is typed.
//!
//! The typed text is mirrored into [`SearchText`] on the field, so owners
//! react with `Changed<SearchText>` instead of reaching into the input:
//!
//! ```ignore
//! @search_field("Filter Nodes") HierarchyFilter
//! // ...
//! fn apply(filters: Query<&SearchText, (With<HierarchyFilter>, Changed<SearchText>)>) { .. }
//! ```

use bevy::{
    feathers::{
        controls::{FeathersTextInput, FeathersTextInputContainer},
        theme::ThemeTextColor,
        tokens,
    },
    picking::Pickable,
    prelude::*,
    text::{EditableText, FontSourceTemplate},
};
use oddity_icons::{Icon, UiIcon};

use crate::fonts::paths;

/// Height of a search field, matching Godot's toolbars.
pub const FIELD_HEIGHT: f32 = 28.0;

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(Update, mirror_search_text);
}

/// What has been typed into a [`search_field`].
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Deref)]
pub struct SearchText(pub String);

impl SearchText {
    /// Whether `candidate` matches, ignoring case. Empty text matches everything.
    pub fn matches(&self, candidate: &str) -> bool {
        self.0.is_empty() || candidate.to_lowercase().contains(&self.0.to_lowercase())
    }
}

/// The dim text shown while the field is empty.
#[derive(Component, Clone, Default)]
struct FieldPlaceholder;

/// A search field showing `placeholder` until something is typed.
pub fn search_field(placeholder: &'static str) -> impl Scene {
    bsn! {
        @FeathersTextInputContainer
        Node {
            height: px(FIELD_HEIGHT),
            min_width: px(0),
            padding: UiRect { left: px(8), right: px(6) },
            column_gap: px(4),
        }
        SearchText
        Children [
            Node { flex_grow: 1.0, min_width: px(0), align_items: AlignItems::Center }
            Children [
                @FeathersTextInput
                TextFont { font_size: FontSize::Px(14.0) }
                --
                Text(placeholder)
                FieldPlaceholder
                Node { position_type: PositionType::Absolute, left: px(0) }
                TextFont {
                    font: FontSourceTemplate::Handle(paths::UI),
                    font_size: FontSize::Px(14.0),
                }
                // The theme's disabled-text grey, which is Godot's placeholder colour.
                ThemeTextColor(tokens::TEXT_INPUT_TEXT_DISABLED)
                Pickable::IGNORE
            ]
            --
            UiIcon { icon: Icon::godot("Search"), size: 16.0 }
            Pickable::IGNORE
        ]
    }
}

/// Copy each input's text into its field's [`SearchText`] and show the
/// placeholder only while the text is empty.
fn mirror_search_text(
    inputs: Query<(Entity, &EditableText), Changed<EditableText>>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    mut fields: Query<&mut SearchText>,
    mut placeholders: Query<&mut Visibility, With<FieldPlaceholder>>,
) {
    for (input, editable) in &inputs {
        let Some(field) = parents.iter_ancestors(input).find(|e| fields.contains(*e)) else {
            continue;
        };
        let text = editable.value().to_string();
        let visibility = if text.is_empty() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        for descendant in children.iter_descendants(field) {
            if let Ok(mut shown) = placeholders.get_mut(descendant) {
                shown.set_if_neq(visibility);
            }
        }
        if let Ok(mut search) = fields.get_mut(field) {
            search.set_if_neq(SearchText(text));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_ignores_case_and_empty_matches_all() {
        assert!(SearchText::default().matches("Player"));
        assert!(SearchText("play".into()).matches("Player"));
        assert!(SearchText("PLAY".into()).matches("player"));
        assert!(!SearchText("goblin".into()).matches("Player"));
    }
}
