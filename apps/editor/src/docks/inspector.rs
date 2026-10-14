//! The Inspector dock, laid out like Godot's.
//!
//! The selected node's icon and name at the top, "Filter Properties" under
//! it, then a category band per component with a row per field. Values are
//! read through reflection (the same path the CLI and MCP use) and shown
//! read-only; editing them, Add Component and prefab overrides arrive in
//! Phase 3.

use bevy::{math::EulerRot, prelude::*, text::FontSourceTemplate};
use oddity_core::{History, Selection, is_in_level, reflect_json};
use oddity_icons::{Icon, UiIcon};
use oddity_widgets::{
    ActivePalette, OddityFonts, Palette,
    field::{SearchText, search_field},
    fonts::paths,
};
use serde_json::Value;

use crate::level::NodeIcon;

pub struct InspectorPlugin;

impl Plugin for InspectorPlugin {
    fn build(&self, app: &mut App) {
        // Exclusive: reading components by reflection needs the whole World.
        app.add_systems(Update, refresh_inspector.run_if(inspector_needs_refresh));
    }
}

/// Components Bevy maintains by itself. They aren't something you edit, so
/// the Inspector leaves them out, as Godot does: the hierarchy is edited in
/// the Hierarchy dock, global transforms and visibility are computed, and the
/// name is the Inspector's title.
const HIDDEN_COMPONENTS: &[&str] = &[
    "ChildOf",
    "Children",
    "GlobalTransform",
    "TransformTreeChanged",
    "InheritedVisibility",
    "ViewVisibility",
    "Name",
];

/// Height of a property row, as in Godot.
const ROW_HEIGHT: f32 = 28.0;

/// The selected node's name.
#[derive(Component, Default, Clone)]
struct InspectorTitle;

/// The selected node's icon.
#[derive(Component, Default, Clone)]
struct InspectorIcon;

/// "Filter Properties".
#[derive(Component, Default, Clone)]
struct InspectorFilter;

/// Holds the categories and rows.
#[derive(Component, Default, Clone)]
struct InspectorList;

/// The dock page.
pub fn inspector_page() -> impl Scene {
    bsn! {
        Node { flex_grow: 1.0, flex_direction: FlexDirection::Column, min_height: px(0) }
        Children [
            Node {
                flex_shrink: 0.0,
                height: px(36),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(6),
                padding: UiRect::horizontal(px(10)),
            }
            Children [
                UiIcon { icon: Icon::godot("Node"), size: 16.0 }
                InspectorIcon
                Node { display: Display::None }
                --
                Text("No node selected")
                InspectorTitle
                TextFont {
                    font: FontSourceTemplate::Handle(paths::UI),
                    font_size: FontSize::Px(14.0),
                }
            ]
            --
            Node { flex_shrink: 0.0, padding: UiRect { left: px(6), right: px(6), bottom: px(6) } }
            Children [ @search_field("Filter Properties") InspectorFilter Node { flex_grow: 1.0 } ]
            --
            Node {
                flex_grow: 1.0,
                min_height: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(2),
                padding: UiRect { left: px(6), right: px(6), bottom: px(6) },
                overflow: Overflow::scroll_y(),
            }
            InspectorList
        ]
    }
}

fn inspector_needs_refresh(
    selection: Res<Selection>,
    history: Res<History>,
    added: Query<(), Added<InspectorList>>,
    names: Query<(), Changed<Name>>,
    filters: Query<(), (With<InspectorFilter>, Changed<SearchText>)>,
) -> bool {
    selection.is_changed()
        || history.is_changed()
        || !added.is_empty()
        || !names.is_empty()
        || !filters.is_empty()
}

/// A field as the Inspector shows it.
#[derive(Clone, Debug, PartialEq)]
struct Property {
    label: String,
    value: Shown,
}

/// How a value is drawn.
#[derive(Clone, Debug, PartialEq)]
enum Shown {
    /// A plain field.
    Text(String),
    /// One small field per axis, letters coloured like Godot's.
    Vector(Vec<(char, String)>),
    /// A dropdown, for enum values.
    Choice(String),
    /// A checkbox.
    Toggle(bool),
    /// A struct inside the component: its fields, indented.
    Nested(Vec<Property>),
}

/// A component's fields, ready to draw.
fn properties(component: &str, value: &Value) -> Vec<Property> {
    match value {
        Value::Object(fields) => fields
            .iter()
            .map(|(name, field)| field_property(component, name, field, 0))
            .collect(),
        // A component that is a single value, e.g. `Visibility` ("Inherited").
        // Strings here are enum variants, so show them as a dropdown.
        Value::String(variant) => vec![Property {
            label: component.to_owned(),
            value: Shown::Choice(variant.clone()),
        }],
        other => vec![Property {
            label: component.to_owned(),
            value: shown(other, 0),
        }],
    }
}

fn field_property(component: &str, name: &str, value: &Value, depth: usize) -> Property {
    // Godot shows a 3D node's transform as Position, Rotation in degrees, Scale.
    if component == "Transform" {
        match name {
            "translation" => {
                return Property {
                    label: "Position".into(),
                    value: shown(value, depth),
                };
            }
            "rotation" => {
                if let Some(degrees) = quat_degrees(value) {
                    return Property {
                        label: "Rotation".into(),
                        value: Shown::Vector(
                            ['x', 'y', 'z']
                                .into_iter()
                                .zip(degrees.to_array())
                                .map(|(axis, d)| (axis, format!("{}°", number(d.into()))))
                                .collect(),
                        ),
                    };
                }
            }
            _ => {}
        }
    }
    Property {
        label: title_case(name),
        value: shown(value, depth),
    }
}

fn shown(value: &Value, depth: usize) -> Shown {
    if let Some(axes) = vector(value) {
        return Shown::Vector(axes);
    }
    match value {
        Value::Null => Shown::Text("None".into()),
        Value::Bool(on) => Shown::Toggle(*on),
        Value::Number(n) => Shown::Text(number(n.as_f64().unwrap_or_default())),
        Value::String(s) => Shown::Text(s.clone()),
        Value::Object(fields) if depth < 2 => Shown::Nested(
            fields
                .iter()
                .map(|(name, field)| field_property("", name, field, depth + 1))
                .collect(),
        ),
        Value::Array(items) => Shown::Text(format!("{} items", items.len())),
        other => Shown::Text(other.to_string()),
    }
}

/// Vectors arrive as `[x, y, z]` or `{"x":..,"y":..,"z":..}` depending on the type.
fn vector(value: &Value) -> Option<Vec<(char, String)>> {
    const AXES: [char; 4] = ['x', 'y', 'z', 'w'];
    let numbers: Vec<f64> = match value {
        Value::Array(items) if (2..=4).contains(&items.len()) => {
            items.iter().map(Value::as_f64).collect::<Option<_>>()?
        }
        Value::Object(fields) if (2..=4).contains(&fields.len()) => AXES[..fields.len()]
            .iter()
            .map(|axis| fields.get(&axis.to_string()).and_then(Value::as_f64))
            .collect::<Option<_>>()?,
        _ => return None,
    };
    Some(
        AXES.into_iter()
            .zip(numbers)
            .map(|(a, n)| (a, number(n)))
            .collect(),
    )
}

/// A quaternion as Godot shows rotations: Euler angles in degrees, YXZ order.
fn quat_degrees(value: &Value) -> Option<Vec3> {
    let numbers: Vec<f32> = match value {
        Value::Array(items) => items
            .iter()
            .map(|v| v.as_f64().map(|n| n as f32))
            .collect::<Option<_>>()?,
        Value::Object(fields) => ["x", "y", "z", "w"]
            .iter()
            .map(|k| fields.get(*k).and_then(Value::as_f64).map(|n| n as f32))
            .collect::<Option<_>>()?,
        _ => return None,
    };
    let [x, y, z, w] = numbers[..] else {
        return None;
    };
    let (yaw, pitch, roll) = Quat::from_xyzw(x, y, z, w).to_euler(EulerRot::YXZ);
    Some(Vec3::new(pitch, yaw, roll) * (180.0 / std::f32::consts::PI))
}

/// Up to three decimals, without trailing zeros, as Godot shows numbers.
fn number(n: f64) -> String {
    let text = format!("{n:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".into()
    } else {
        text.into()
    }
}

/// `field_name` as Godot labels it: "Field Name".
fn title_case(name: &str) -> String {
    name.split('_')
        .filter(|w| !w.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Godot's icon for a component's category band.
fn category_icon(component: &str) -> Icon {
    Icon::godot(match component {
        // A transform is what makes a Bevy entity a 3D node.
        "Transform" => "Node3D",
        "Visibility" => "GuiVisibilityVisible",
        _ => "Object",
    })
}

/// `Transform` for `bevy_transform::components::transform::Transform`.
fn short_name(type_path: &str) -> &str {
    type_path.rsplit("::").next().unwrap_or(type_path)
}

/// Keep the rows matching the filter. A category whose name matches keeps
/// all its rows.
fn filtered(category: &str, rows: Vec<Property>, filter: &SearchText) -> Vec<Property> {
    if filter.matches(category) {
        rows
    } else {
        rows.into_iter()
            .filter(|p| filter.matches(&p.label))
            .collect()
    }
}

fn refresh_inspector(world: &mut World) {
    let target = world
        .resource::<Selection>()
        .primary()
        .filter(|e| world.get_entity(*e).is_ok() && is_in_level(world, *e));
    let selected_count = world.resource::<Selection>().entities().len();
    let filter = world
        .query_filtered::<&SearchText, With<InspectorFilter>>()
        .iter(world)
        .next()
        .cloned()
        .unwrap_or_default();

    let (title, icon, categories) = match target {
        Some(entity) => {
            let name = world
                .get::<Name>(entity)
                .map_or_else(|| format!("{entity}"), |n| n.as_str().to_owned());
            let title = if selected_count > 1 {
                format!("{name}  (+{} more)", selected_count - 1)
            } else {
                name
            };
            let icon = world.get::<NodeIcon>(entity).cloned().unwrap_or_default().0;
            let mut categories: Vec<(String, Vec<Property>)> =
                reflect_json::components_of(world, entity)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|path| !HIDDEN_COMPONENTS.contains(&short_name(path)))
                    .filter_map(|path| {
                        let short = short_name(&path).to_owned();
                        let value = reflect_json::component_json(world, entity, &path).ok()?;
                        let rows = filtered(&short, properties(&short, &value), &filter);
                        (!rows.is_empty() || filter.matches(&short)).then_some((short, rows))
                    })
                    .collect();
            // Transform first, like Godot's Node3D section; the rest by name.
            categories.sort_by_key(|(name, _)| (name != "Transform", name.clone()));
            (title, Some(icon), categories)
        }
        None => ("No node selected".to_owned(), None, Vec::new()),
    };

    let palette = **world.resource::<ActivePalette>();
    let fonts = world.resource::<OddityFonts>().clone();

    let mut titles = world.query_filtered::<&mut TextColor, With<InspectorTitle>>();
    for mut colour in titles.iter_mut(world) {
        colour.0 = if icon.is_some() {
            palette.font_focus
        } else {
            palette.font_secondary
        };
    }
    let mut titles = world.query_filtered::<&mut Text, With<InspectorTitle>>();
    for mut text in titles.iter_mut(world) {
        text.0.clone_from(&title);
    }
    let mut icons = world.query_filtered::<(&mut UiIcon, &mut Node), With<InspectorIcon>>();
    for (mut ui_icon, mut node) in icons.iter_mut(world) {
        match &icon {
            Some(icon) => {
                if &ui_icon.icon != icon {
                    ui_icon.icon = icon.clone();
                }
                node.display = Display::Flex;
            }
            None => node.display = Display::None,
        }
    }

    let lists: Vec<Entity> = world
        .query_filtered::<Entity, With<InspectorList>>()
        .iter(world)
        .collect();
    for list in lists {
        world.entity_mut(list).despawn_children();
        let draw = Draw {
            palette: &palette,
            fonts: &fonts,
        };
        for (name, rows) in &categories {
            draw.category(world, list, name);
            for row in rows {
                draw.property(world, list, row, 0);
            }
        }
    }
}

/// Spawns Inspector rows with the theme's colours and fonts.
struct Draw<'a> {
    palette: &'a Palette,
    fonts: &'a OddityFonts,
}

impl Draw<'_> {
    fn text(&self, text: impl Into<String>, colour: Color, bold: bool) -> impl Bundle {
        (
            Text::new(text),
            TextFont {
                font: if bold {
                    &self.fonts.ui_bold
                } else {
                    &self.fonts.ui
                }
                .clone()
                .into(),
                font_size: FontSize::Px(14.0),
                ..default()
            },
            TextLayout {
                linebreak: LineBreak::NoWrap,
                ..default()
            },
            TextColor(colour),
        )
    }

    /// Godot's category band: icon and bold name, centred on a lighter strip.
    fn category(&self, world: &mut World, list: Entity, name: &str) {
        let band = world
            .spawn((
                Node {
                    height: Val::Px(ROW_HEIGHT),
                    flex_shrink: 0.0,
                    margin: UiRect::top(Val::Px(4.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(self.palette.category),
                ChildOf(list),
            ))
            .id();
        world.spawn((UiIcon::new(category_icon(name), 16.0), ChildOf(band)));
        world.spawn((
            self.text(name, self.palette.font_focus, true),
            ChildOf(band),
        ));
    }

    /// A label on the left, the value on the right, as in Godot.
    fn property(&self, world: &mut World, list: Entity, property: &Property, depth: usize) {
        let row = world
            .spawn((
                Node {
                    height: Val::Px(ROW_HEIGHT),
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    padding: UiRect::left(Val::Px(8.0 + depth as f32 * 12.0)),
                    ..default()
                },
                ChildOf(list),
            ))
            .id();
        let label = world
            .spawn((
                Node {
                    flex_basis: Val::Percent(42.0),
                    flex_shrink: 0.0,
                    overflow: Overflow::clip(),
                    ..default()
                },
                ChildOf(row),
            ))
            .id();
        world.spawn((
            self.text(&property.label, self.palette.font, false),
            ChildOf(label),
        ));

        let value = world
            .spawn((
                Node {
                    flex_grow: 1.0,
                    min_width: Val::Px(0.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(2.0),
                    ..default()
                },
                ChildOf(row),
            ))
            .id();
        match &property.value {
            Shown::Text(text) => {
                let field = self.field(world, value);
                world.spawn((self.text(text, self.palette.font, false), ChildOf(field)));
            }
            Shown::Choice(text) => {
                let field = self.field(world, value);
                world.entity_mut(field).insert(Node {
                    justify_content: JustifyContent::SpaceBetween,
                    ..self.field_node()
                });
                world.spawn((self.text(text, self.palette.font, false), ChildOf(field)));
                world.spawn((
                    UiIcon::new(Icon::godot("GuiOptionArrow"), 16.0),
                    ChildOf(field),
                ));
            }
            Shown::Toggle(on) => {
                let icon = if *on { "GuiChecked" } else { "GuiUnchecked" };
                world.spawn((UiIcon::new(Icon::godot(icon), 16.0), ChildOf(value)));
                let text = if *on { "On" } else { "Off" };
                world.spawn((self.text(text, self.palette.font, false), ChildOf(value)));
            }
            Shown::Vector(axes) => {
                for (axis, number) in axes {
                    let field = self.field(world, value);
                    let colour = match axis {
                        'x' => self.palette.axis_x,
                        'y' => self.palette.axis_y,
                        'z' => self.palette.axis_z,
                        _ => self.palette.font_secondary,
                    };
                    world.spawn((self.text(axis.to_string(), colour, false), ChildOf(field)));
                    world.spawn((self.text(number, self.palette.font, false), ChildOf(field)));
                }
            }
            Shown::Nested(fields) => {
                for field in fields {
                    self.property(world, list, field, depth + 1);
                }
            }
        }
    }

    fn field_node(&self) -> Node {
        Node {
            flex_grow: 1.0,
            flex_basis: Val::Px(0.0),
            min_width: Val::Px(0.0),
            height: Val::Px(24.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(6.0),
            padding: UiRect::horizontal(Val::Px(6.0)),
            border_radius: BorderRadius::all(Val::Px(3.0)),
            overflow: Overflow::clip(),
            ..default()
        }
    }

    /// Godot's dark value box.
    fn field(&self, world: &mut World, parent: Entity) -> Entity {
        world
            .spawn((
                self.field_node(),
                BackgroundColor(self.palette.dark_1),
                ChildOf(parent),
            ))
            .id()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn numbers_read_like_godot() {
        assert_eq!(number(4.0), "4");
        assert_eq!(number(0.5), "0.5");
        assert_eq!(number(1.23456), "1.235");
        assert_eq!(number(-0.0001), "0");
    }

    #[test]
    fn transform_shows_position_rotation_in_degrees_and_scale() {
        let half_turn = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let value = json!({
            "translation": [0.0, 4.0, 8.0],
            "rotation": [half_turn.x, half_turn.y, half_turn.z, half_turn.w],
            "scale": {"x": 1.0, "y": 1.0, "z": 1.0},
        });
        let rows = properties("Transform", &value);
        let labels: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["Position", "Rotation", "Scale"]);
        assert_eq!(
            rows[0].value,
            Shown::Vector(vec![
                ('x', "0".into()),
                ('y', "4".into()),
                ('z', "8".into())
            ])
        );
        assert_eq!(
            rows[1].value,
            Shown::Vector(vec![
                ('x', "0°".into()),
                ('y', "90°".into()),
                ('z', "0°".into())
            ])
        );
    }

    #[test]
    fn single_value_components_show_as_a_dropdown() {
        let rows = properties("Visibility", &json!("Hidden"));
        assert_eq!(rows[0].label, "Visibility");
        assert_eq!(rows[0].value, Shown::Choice("Hidden".into()));
    }

    #[test]
    fn filter_keeps_matching_rows_or_whole_matching_category() {
        let rows = properties(
            "Transform",
            &json!({"translation": [0, 0, 0], "scale": [1, 1, 1]}),
        );
        let only_scale = filtered("Transform", rows.clone(), &SearchText("sca".into()));
        assert_eq!(only_scale.len(), 1);
        let all = filtered("Transform", rows, &SearchText("trans".into()));
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn labels_are_title_case() {
        assert_eq!(title_case("cast_shadows"), "Cast Shadows");
    }
}
