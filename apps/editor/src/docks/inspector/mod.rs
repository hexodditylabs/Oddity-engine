//! The Inspector dock, laid out like Godot's.
//!
//! The selected node's icon and name at the top, "Filter Properties" under
//! it, then a category band per component with a row per field. Values are
//! read through reflection (the same path the CLI and MCP use).
//!
//! A field whose Rust type has a registered Inspector field (`fields.rs`, or
//! an extension's) is drawn and edited by it, through `component.set`. Other
//! fields are shown read-only until a field for their type is registered.
//! Scripts follow the components, with Add Component under them (see
//! `scripts`); prefab overrides arrive later in Phase 3.

mod fields;
mod scripts;

use bevy::{math::EulerRot, prelude::*, text::FontSourceTemplate};
use oddity_core::{
    CommandExecuted, History, NodeKind, Selection, is_in_level,
    reflect_json::{self, FieldInfo},
};
use oddity_icons::{Icon, UiIcon};
use oddity_plugin_api::{
    DockSlot, DockTab, EditorAppExt, EditorMode, FieldTarget, InspectorFields, NodeTypes,
};
use oddity_widgets::{
    ActivePalette, OddityFonts, Palette,
    field::{SearchText, search_field},
    fonts::paths,
};
use serde_json::Value;

use super::FIRST_TAB;

/// The Inspector tab, and the system that fills it.
pub struct InspectorPlugin;

impl Plugin for InspectorPlugin {
    fn build(&self, app: &mut App) {
        // Exclusive: reading components by reflection needs the whole World.
        app.add_plugins(fields::FieldsPlugin)
            .add_systems(Update, refresh_inspector.run_if(inspector_needs_refresh));
        if app.editor_mode() == EditorMode::Window {
            app.add_dock_tab(
                DockTab::new(DockSlot::Inspector, "Inspector", inspector_page).order(FIRST_TAB),
            );
        }
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
    // The node's type: its icon, beside the title.
    "NodeKind",
    // Drawn one band per script (see `scripts`).
    "Scripts",
];

/// Height of a property row, as in Godot.
const ROW_HEIGHT: f32 = 28.0;
/// Height of a value box inside a row, as in Godot.
const FIELD_HEIGHT: f32 = 24.0;
/// The Inspector's text size, as in Godot.
const FONT_SIZE: f32 = 14.0;

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
fn inspector_page() -> impl Scene {
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
    mut executed: MessageReader<CommandExecuted>,
    history: Res<History>,
    mut missed: Local<bool>,
    selection: Res<Selection>,
    // A build that brings new script types redraws the scripts' fields.
    script_types: Res<oddity_core::scripts::ScriptTypes>,
    added: Query<(), Added<InspectorList>>,
    names: Query<(), Changed<Name>>,
    filters: Query<(), (With<InspectorFilter>, Changed<SearchText>)>,
) -> bool {
    // Read first: `||` would skip it and leave the messages for next frame.
    let edited = executed.read().count() > 0;
    // Rebuilding mid-drag would despawn the box being dragged, which ends
    // the drag; the box shows its own value, and the rest catches up after.
    if history.in_gesture() {
        *missed |= edited;
        return false;
    }
    let caught_up = std::mem::take(&mut *missed);
    edited
        || caught_up
        || selection.is_changed()
        || script_types.is_changed()
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
    /// Drawn and edited by the Inspector field registered for its type.
    Field {
        /// Where its edits go.
        target: FieldTarget,
        /// Its Rust type, which picks the field.
        type_path: String,
        /// Its value as JSON.
        value: Value,
    },
}

/// A component's fields, ready to draw. `registered` says whether a field
/// type has an Inspector field to draw it.
fn properties(
    entity: Entity,
    component_path: &str,
    component: &str,
    fields: &[FieldInfo],
    registered: impl Fn(&str) -> bool,
) -> Vec<Property> {
    fields
        .iter()
        .map(|field| {
            let label = if field.name.is_empty() {
                component.to_owned()
            } else {
                field_label(component, &field.name)
            };
            let value = if registered(&field.type_path) {
                Shown::Field {
                    target: FieldTarget {
                        entity,
                        component: component_path.to_owned(),
                        path: field.name.clone(),
                    },
                    type_path: field.type_path.clone(),
                    value: field.value.clone(),
                }
            } else {
                match &field.value {
                    // A component that is one value, e.g. `Visibility`
                    // ("Inherited"): strings there are enum variants, so
                    // they show as a dropdown.
                    Value::String(variant) if field.name.is_empty() => {
                        Shown::Choice(variant.clone())
                    }
                    other => shown(other, 0),
                }
            };
            Property { label, value }
        })
        .collect()
}

/// Godot shows a 3D node's `translation` as Position; other names are
/// title-cased.
fn field_label(component: &str, name: &str) -> String {
    match (component, name) {
        ("Transform", "translation") => "Position".into(),
        _ => title_case(name),
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
                .map(|(name, field)| Property {
                    label: title_case(name),
                    value: shown(field, depth + 1),
                })
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
    let field_types = world
        .get_resource::<InspectorFields>()
        .cloned()
        .unwrap_or_default();
    let filter = world
        .query_filtered::<&SearchText, With<InspectorFilter>>()
        .iter(world)
        .next()
        .cloned()
        .unwrap_or_default();

    let script_sections = target
        .map(|entity| {
            scripts::sections(world, entity, &filter, |type_path| {
                field_types.get(type_path).is_some()
            })
        })
        .unwrap_or_default();
    let addable = target.map(|entity| (entity, scripts::addable(world, entity)));

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
            let icon = world.get_resource::<NodeTypes>().map_or_else(
                || Icon::godot("Node"),
                |types| types.icon_for(world.get::<NodeKind>(entity)),
            );
            let mut categories: Vec<(String, Vec<Property>)> =
                reflect_json::components_of(world, entity)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|path| !HIDDEN_COMPONENTS.contains(&short_name(path)))
                    .filter_map(|path| {
                        let short = short_name(&path).to_owned();
                        let fields = reflect_json::fields_of(world, entity, &path).ok()?;
                        let rows = properties(entity, &path, &short, &fields, |type_path| {
                            field_types.get(type_path).is_some()
                        });
                        let rows = filtered(&short, rows, &filter);
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
            fields: &field_types,
        };
        for (name, rows) in &categories {
            draw.category(world, list, name);
            for row in rows {
                draw.property(world, list, row, 0);
            }
        }
        for section in &script_sections {
            draw.script_band(world, list, section);
            for row in &section.rows {
                draw.property(world, list, row, 0);
            }
        }
        if let Some((entity, choices)) = addable.clone().filter(|(_, choices)| !choices.is_empty())
        {
            world
                .entity_mut(list)
                .queue_spawn_related_scenes::<Children>(vec![Box::new(scripts::add_component(
                    entity, choices,
                )) as Box<dyn Scene>]);
        }
    }
}

/// Spawns Inspector rows with the theme's colours and fonts.
struct Draw<'a> {
    palette: &'a Palette,
    fonts: &'a OddityFonts,
    fields: &'a InspectorFields,
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
                font_size: FontSize::Px(FONT_SIZE),
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
            Shown::Field {
                target,
                type_path,
                value: field_value,
            } => {
                if let Some(draw) = self.fields.get(type_path) {
                    let scene = draw(target, field_value);
                    let mut container = world.entity_mut(value);
                    container.insert(target.clone());
                    container.queue_spawn_related_scenes::<Children>(vec![scene]);
                }
            }
        }
    }

    fn field_node(&self) -> Node {
        Node {
            flex_grow: 1.0,
            flex_basis: Val::Px(0.0),
            min_width: Val::Px(0.0),
            height: Val::Px(FIELD_HEIGHT),
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
    use bevy::reflect::TypePath;
    use oddity_core::{EditorWorldExt, LevelRoot, headless_app};
    use oddity_plugin_api::InspectorField;
    use serde_json::json;

    use super::{
        fields::{Scrub, scrub_to},
        *,
    };

    fn field(name: &str, type_path: &str, value: Value) -> FieldInfo {
        FieldInfo {
            name: name.into(),
            type_path: type_path.into(),
            value,
        }
    }

    fn rows(component: &str, fields: &[FieldInfo], registered: &[&str]) -> Vec<Property> {
        let entity = Entity::from_raw_u32(1).unwrap();
        properties(entity, component, component, fields, |t| {
            registered.contains(&t)
        })
    }

    #[test]
    fn a_scripts_fields_come_from_the_type_description_and_edit_through_it() {
        use oddity_core::scripts::{ScriptInstance, ScriptTypes, ScriptValues, Scripts};

        let mut world = World::new();
        world.insert_resource(
            serde_json::from_value::<ScriptTypes>(json!({ "scripts": [{
                "type_path": "game::Player", "name": "Player",
                "fields": [
                    { "name": "speed", "type": "f32", "default": 6.0 },
                    { "name": "muzzle", "type": "NodeRef", "default": null }
                ]
            }] }))
            .unwrap(),
        );
        let values = |value: Value| ScriptValues(value.as_object().unwrap().clone());
        let node = world
            .spawn(Scripts(vec![
                ScriptInstance {
                    type_path: "game::Player".into(),
                    enabled: true,
                    values: values(json!({ "speed": 9.5 })),
                },
                ScriptInstance {
                    type_path: "game::Removed".into(),
                    enabled: false,
                    values: values(json!({ "hp": 3 })),
                },
            ]))
            .id();
        let sections = scripts::sections(&world, node, &SearchText::default(), |t| t == "f32");
        assert_eq!(sections.len(), 2);
        let player = &sections[0].rows;
        assert_eq!(player[0].label, "Speed");
        assert!(
            matches!(&player[0].value, Shown::Field { target, value, .. }
                if target.component == "script:game::Player" && target.path == "speed" && *value == json!(9.5)),
            "{:?}",
            player[0].value
        );
        assert_eq!(
            player[1].value,
            Shown::Text("None".into()),
            "NodeRef: read-only until D3"
        );
        assert_eq!(
            sections[1].rows[0].value,
            Shown::Text("3".into()),
            "kept, read-only"
        );
    }

    #[test]
    fn numbers_read_like_godot() {
        assert_eq!(number(4.0), "4");
        assert_eq!(number(0.5), "0.5");
        assert_eq!(number(1.23456), "1.235");
        assert_eq!(number(-0.0001), "0");
    }

    #[test]
    fn transform_rows_are_position_rotation_and_scale_drawn_by_their_fields() {
        let fields = [
            field("translation", "glam::Vec3", json!([0.0, 4.0, 8.0])),
            field("rotation", "glam::Quat", json!([0.0, 0.0, 0.0, 1.0])),
            field("scale", "glam::Vec3", json!([1.0, 1.0, 1.0])),
        ];
        let rows = rows("Transform", &fields, &["glam::Vec3", "glam::Quat"]);
        let labels: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["Position", "Rotation", "Scale"]);
        assert!(
            matches!(&rows[1].value, Shown::Field { type_path, target, .. }
                if type_path == "glam::Quat" && target.path == "rotation")
        );
    }

    #[test]
    fn fields_without_a_registered_type_stay_read_only() {
        let rows = rows("Health", &[field("max", "f32", json!(10.0))], &[]);
        assert_eq!(rows[0].value, Shown::Text("10".into()));
    }

    #[test]
    fn single_value_components_show_as_a_dropdown() {
        let fields = [field(
            "",
            "bevy_camera::visibility::Visibility",
            json!("Hidden"),
        )];
        let rows = rows("Visibility", &fields, &[]);
        assert_eq!(rows[0].label, "Visibility");
        assert_eq!(rows[0].value, Shown::Choice("Hidden".into()));
    }

    #[test]
    fn filter_keeps_matching_rows_or_whole_matching_category() {
        let fields = [
            field("translation", "glam::Vec3", json!([0, 0, 0])),
            field("scale", "glam::Vec3", json!([1, 1, 1])),
        ];
        let rows = rows("Transform", &fields, &[]);
        let only_scale = filtered("Transform", rows.clone(), &SearchText("sca".into()));
        assert_eq!(only_scale.len(), 1);
        let all = filtered("Transform", rows, &SearchText("trans".into()));
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn labels_are_title_case() {
        assert_eq!(title_case("cast_shadows"), "Cast Shadows");
    }

    /// A plugin's own value type, and the field it draws it with.
    #[derive(Reflect, Clone, Copy, Debug, Default)]
    struct Celsius(f32);

    struct CelsiusField;

    impl InspectorField for CelsiusField {
        type Value = Celsius;

        fn draw(_target: &FieldTarget, value: &Value) -> Box<dyn Scene> {
            let text = format!("{value} °C");
            Box::new(bsn! { Text(text) })
        }
    }

    #[derive(Component, Reflect, Default)]
    #[reflect(Component, Default)]
    struct Thermometer {
        reading: Celsius,
    }

    #[test]
    fn a_plugins_field_type_is_drawn_with_its_registered_field() {
        let mut app = headless_app();
        app.insert_resource(EditorMode::Window)
            .register_type::<Thermometer>()
            .add_inspector_field::<CelsiusField>();
        let world = app.world_mut();
        let root = world.spawn((Name::new("Main"), LevelRoot)).id();
        let node = world.spawn((Thermometer::default(), ChildOf(root))).id();

        let component = Thermometer::type_path();
        let fields = reflect_json::fields_of(world, node, component).unwrap();
        let registry = world.resource::<InspectorFields>();
        let rows = properties(node, component, "Thermometer", &fields, |t| {
            registry.get(t).is_some()
        });
        assert!(
            matches!(&rows[0].value, Shown::Field { type_path, .. } if type_path == Celsius::type_path())
        );
        assert!(registry.get(Celsius::type_path()).is_some());
    }

    /// A level with one node at the origin, and its translation as a field.
    fn transform_node() -> (App, Entity, FieldTarget) {
        let mut app = headless_app();
        app.register_type::<Transform>();
        let world = app.world_mut();
        let root = world.spawn((Name::new("Main"), LevelRoot)).id();
        let node = world
            .spawn((Name::new("Crate"), Transform::default(), ChildOf(root)))
            .id();
        let target = FieldTarget {
            entity: node,
            component: Transform::type_path().into(),
            path: "translation".into(),
        };
        (app, node, target)
    }

    #[test]
    fn dragging_a_transform_value_is_one_undo_step() {
        let (mut app, node, target) = transform_node();
        let world = app.world_mut();
        let start = target.get(world).unwrap();

        world.resource_mut::<History>().begin_gesture();
        for pixels in 1..=30 {
            let shown = scrub_to(world, &target, Scrub::Vector(0), &start, pixels as f32);
            assert!(shown.is_some());
        }
        world.resource_mut::<History>().end_gesture();

        let x = |world: &World| world.get::<Transform>(node).unwrap().translation.x;
        assert!((x(world) - 0.3).abs() < 1e-5, "x is {}", x(world));
        assert_eq!(world.resource::<History>().undo_len(), 1);
        world.undo().unwrap();
        assert_eq!(x(world), 0.0, "one undo puts it back where the drag began");
    }

    #[test]
    fn dragging_a_rotation_turns_it_in_degrees() {
        let (mut app, node, mut target) = transform_node();
        target.path = "rotation".into();
        let world = app.world_mut();
        let start = target.get(world).unwrap();
        let shown = scrub_to(world, &target, Scrub::Rotation(1), &start, 180.0);
        assert_eq!(shown.as_deref(), Some("90°"));
        let rotation = world.get::<Transform>(node).unwrap().rotation;
        let expected = Quat::from_rotation_y(90f32.to_radians());
        // The dot product, not `angle_between`: its `acos` is too coarse in
        // f32 to compare nearly equal rotations.
        assert!(
            rotation.dot(expected).abs() > 0.999_999,
            "{rotation:?} is not {expected:?}"
        );
    }
}
