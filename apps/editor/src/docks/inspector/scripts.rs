//! Scripts in the Inspector (`roadmap/scripting.md`, task D2), as Unity
//! shows them:
//!
//! ```text
//! ▾ ☑ Health                                         script
//!     Max            100
//! [ Add Component ]
//! ```
//!
//! A script is data in the editor ([`Scripts`]); its fields come from the
//! game's type description ([`ScriptTypes`]), and are drawn by the same
//! Inspector fields as a component's, through a `script:<type path>`
//! [`FieldTarget`], so every edit is `script.set_field`. The ☑ runs
//! `script.set_enabled`; Add Component lists the game's scripts and runs
//! `script.attach`. A script the last build doesn't have is greyed out and
//! read-only, and loses nothing.

use bevy::{
    feathers::{
        controls::{
            ButtonVariant, FeathersButton, FeathersMenu, FeathersMenuItem, FeathersMenuPopup,
        },
        display::caption,
    },
    picking::{Pickable, events::PointerClick},
    prelude::*,
    ui_widgets::{Activate, MenuButton},
};
use oddity_core::scripts::{ScriptTypes, Scripts};
use oddity_icons::{Icon, UiIcon};
use oddity_plugin_api::FieldTarget;
use oddity_widgets::field::SearchText;
use serde_json::{Value, json};

use super::{Draw, Property, ROW_HEIGHT, Shown, filtered, short_name, shown, title_case};
use crate::actions::run_command;

/// One script, ready to draw.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ScriptSection {
    entity: Entity,
    type_path: String,
    name: String,
    enabled: bool,
    /// Whether the last build has it.
    known: bool,
    pub(super) rows: Vec<Property>,
}

/// The type path an Inspector field is registered under, for the short
/// type names the type description uses.
fn field_type(short: &str) -> Option<&'static str> {
    match short {
        "f32" => Some("f32"),
        "bool" => Some("bool"),
        "Vec3" => Some("glam::Vec3"),
        "Quat" => Some("glam::Quat"),
        _ => None,
    }
}

/// `entity`'s scripts, with the rows matching `filter`.
pub(super) fn sections(
    world: &World,
    entity: Entity,
    filter: &SearchText,
    registered: impl Fn(&str) -> bool,
) -> Vec<ScriptSection> {
    let Some(scripts) = world.get::<Scripts>(entity) else {
        return Vec::new();
    };
    let types = world.get_resource::<ScriptTypes>();
    scripts
        .0
        .iter()
        .filter_map(|script| {
            let described = types.and_then(|types| types.find(&script.type_path));
            let name = described.map_or_else(
                || short_name(&script.type_path).to_owned(),
                |described| described.name.clone(),
            );
            // Fields in the order the game declares them; without a
            // description, the ones the script has.
            let fields: Vec<(String, Option<&str>, Value)> = match described {
                Some(described) => described
                    .fields
                    .iter()
                    .map(|field| {
                        let value = script.values.0.get(&field.name).cloned();
                        (
                            field.name.clone(),
                            Some(field.type_name.as_str()),
                            value.unwrap_or_else(|| field.default.clone()),
                        )
                    })
                    .collect(),
                None => script
                    .values
                    .0
                    .iter()
                    .map(|(name, value)| (name.clone(), None, value.clone()))
                    .collect(),
            };
            let rows = fields
                .into_iter()
                .map(|(field, type_name, value)| {
                    let editable = type_name
                        .and_then(field_type)
                        .filter(|type_path| registered(type_path));
                    let value = match editable {
                        Some(type_path) => Shown::Field {
                            target: FieldTarget {
                                entity,
                                component: format!("script:{}", script.type_path),
                                path: field.clone(),
                            },
                            type_path: type_path.to_owned(),
                            value,
                        },
                        None => shown(&value, 0),
                    };
                    Property {
                        label: title_case(&field),
                        value,
                    }
                })
                .collect();
            let rows = filtered(&name, rows, filter);
            (!rows.is_empty() || filter.matches(&name)).then(|| ScriptSection {
                entity,
                type_path: script.type_path.clone(),
                name,
                enabled: script.enabled,
                known: described.is_some(),
                rows,
            })
        })
        .collect()
}

/// The game's scripts `entity` doesn't have yet, by name: Add Component's
/// list.
pub(super) fn addable(world: &World, entity: Entity) -> Vec<(String, String)> {
    let Some(types) = world.get_resource::<ScriptTypes>() else {
        return Vec::new();
    };
    let attached = world.get::<Scripts>(entity);
    let mut choices: Vec<(String, String)> = types
        .scripts
        .iter()
        .filter(|script| attached.is_none_or(|scripts| scripts.get(&script.type_path).is_none()))
        .map(|script| (script.name.clone(), script.type_path.clone()))
        .collect();
    choices.sort();
    choices
}

/// The ☑ of one script.
#[derive(Component, Clone)]
struct ScriptToggle {
    entity: Entity,
    type_path: String,
    enabled: bool,
}

fn toggle(click: On<PointerClick>, toggles: Query<&ScriptToggle>, mut commands: Commands) {
    if let Ok(toggle) = toggles.get(click.entity) {
        run_command(
            &mut commands,
            "script.set_enabled",
            json!({
                "entity": toggle.entity.to_bits(),
                "script": toggle.type_path,
                "enabled": !toggle.enabled,
            }),
        );
    }
}

impl Draw<'_> {
    /// A script's band: its ☑, the script icon and its name, and "script"
    /// on the right, as Unity marks them.
    pub(super) fn script_band(&self, world: &mut World, list: Entity, section: &ScriptSection) {
        let band = world
            .spawn((
                Node {
                    height: Val::Px(ROW_HEIGHT),
                    flex_shrink: 0.0,
                    margin: UiRect::top(Val::Px(4.0)),
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    padding: UiRect::horizontal(Val::Px(8.0)),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(self.palette.category),
                ChildOf(list),
            ))
            .id();
        let checkbox = if section.enabled {
            "GuiChecked"
        } else {
            "GuiUnchecked"
        };
        world
            .spawn((
                UiIcon::new(Icon::godot(checkbox), 16.0),
                ScriptToggle {
                    entity: section.entity,
                    type_path: section.type_path.clone(),
                    enabled: section.enabled,
                },
                ChildOf(band),
            ))
            .observe(toggle);
        world.spawn((
            UiIcon::new(Icon::godot("Script"), 16.0),
            Pickable::IGNORE,
            ChildOf(band),
        ));
        let name_colour = if section.known {
            self.palette.font_focus
        } else {
            self.palette.font_disabled
        };
        world.spawn((self.text(&section.name, name_colour, true), ChildOf(band)));
        world.spawn((
            Node {
                flex_grow: 1.0,
                ..default()
            },
            ChildOf(band),
        ));
        let tag = if section.known {
            "script"
        } else {
            "not in the last build"
        };
        world.spawn((
            self.text(tag, self.palette.font_secondary, false),
            ChildOf(band),
        ));
    }
}

/// Add Component: a button whose menu lists `choices` (name, type path).
pub(super) fn add_component(entity: Entity, choices: Vec<(String, String)>) -> impl Scene {
    let items: Vec<_> = choices
        .into_iter()
        .map(|(name, type_path)| add_component_item(entity, name, type_path))
        .collect();
    bsn! {
        Node {
            flex_shrink: 0.0,
            justify_content: JustifyContent::Center,
            margin: UiRect::vertical(px(10)),
        }
        Children [
            @FeathersMenu
            Children [
                @FeathersButton {
                    @caption: bsn! { @caption("Add Component") },
                    @variant: ButtonVariant::Normal,
                }
                MenuButton
                --
                @FeathersMenuPopup
                Children [ {items} ]
            ]
        ]
    }
}

fn add_component_item(entity: Entity, name: String, type_path: String) -> impl Scene {
    bsn! {
        @FeathersMenuItem {
            @caption: bsn_list! {
                UiIcon { icon: {Icon::godot("Script")}, size: 16.0 }
                Pickable::IGNORE
                --
                @caption(name)
            },
        }
        Node { column_gap: px(6) }
        on(move |_: On<Activate>, mut commands: Commands| {
            run_command(
                &mut commands,
                "script.attach",
                json!({ "entity": entity.to_bits(), "script": type_path }),
            );
        })
    }
}
