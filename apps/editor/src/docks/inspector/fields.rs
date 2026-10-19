//! The Inspector's built-in fields, registered through `oddity_plugin_api`
//! like an extension's: x/y/z boxes for `Vec3`, a rotation in degrees for
//! `Quat`, a number box for `f32` and Godot's checkbox for `bool`.
//!
//! Dragging sideways across a number box scrubs it, as in Godot: the cursor
//! shows ↔ over the box and keeps it for the whole drag. The drag is a
//! gesture, so however many frames it lasts it is one undo step.

use bevy::{
    math::EulerRot,
    picking::{
        Pickable,
        cursor::{EntityCursor, OverrideCursor},
        events::{PointerClick, PointerDrag, PointerDragEnd, PointerDragStart},
        pointer::PointerButton,
    },
    prelude::*,
    text::FontSourceTemplate,
    window::SystemCursorIcon,
};
use oddity_core::History;
use oddity_icons::{Icon, UiIcon};
use oddity_plugin_api::{EditorAppExt, EditorMode, FieldTarget, InspectorField};
use oddity_widgets::{ActivePalette, fonts::paths};
use serde_json::{Value, json};

use super::{FIELD_HEIGHT, FONT_SIZE, number, quat_degrees};

/// How far a value moves per pixel dragged. Godot's spin sliders feel about
/// like this at their default step.
const UNITS_PER_PIXEL: f64 = 0.01;
/// How far a rotation turns per pixel dragged.
const DEGREES_PER_PIXEL: f64 = 0.5;

/// The built-in fields, and the systems that paint and scrub them.
pub struct FieldsPlugin;

impl Plugin for FieldsPlugin {
    fn build(&self, app: &mut App) {
        if app.editor_mode() == EditorMode::Window {
            app.add_inspector_field::<Vec3Field>()
                .add_inspector_field::<RotationField>()
                .add_inspector_field::<NumberField>()
                .add_inspector_field::<CheckboxField>()
                .add_systems(Update, paint_fields);
        }
    }
}

/// x/y/z boxes for a `Vec3` (positions, scales).
struct Vec3Field;

impl InspectorField for Vec3Field {
    type Value = Vec3;

    fn draw(_target: &FieldTarget, value: &Value) -> Box<dyn Scene> {
        number_boxes(value, Scrub::Vector)
    }
}

/// A `Quat` as Godot shows rotations: x/y/z in degrees.
struct RotationField;

impl InspectorField for RotationField {
    type Value = Quat;

    fn draw(_target: &FieldTarget, value: &Value) -> Box<dyn Scene> {
        number_boxes(value, Scrub::Rotation)
    }
}

/// One number box.
struct NumberField;

impl InspectorField for NumberField {
    type Value = f32;

    fn draw(_target: &FieldTarget, value: &Value) -> Box<dyn Scene> {
        let text = shown_text(Scrub::Scalar, value).unwrap_or_default();
        let number = number_box(Scrub::Scalar, None, text);
        Box::new(bsn! {
            Node {
                flex_grow: 1.0,
                min_width: px(0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
            }
            Children [ @{number} ]
        })
    }
}

/// Godot's checkbox: its icon and On or Off. Clicking flips it.
struct CheckboxField;

impl InspectorField for CheckboxField {
    type Value = bool;

    fn draw(_target: &FieldTarget, value: &Value) -> Box<dyn Scene> {
        // Not `on`: inside `bsn!` that name is the observer syntax.
        let checked = value.as_bool().unwrap_or_default();
        let icon = Icon::godot(if checked {
            "GuiChecked"
        } else {
            "GuiUnchecked"
        });
        let text = if checked { "On" } else { "Off" };
        Box::new(bsn! {
            Node {
                flex_grow: 1.0,
                min_width: px(0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(2),
            }
            on(toggle_checkbox)
            Children [
                UiIcon { icon: {icon.clone()}, size: 16.0 } Pickable::IGNORE
                --
                @field_text(text.to_owned()) Pickable::IGNORE
            ]
        })
    }
}

fn toggle_checkbox(click: On<PointerClick>, mut commands: Commands) {
    if click.button != PointerButton::Primary {
        return;
    }
    let checkbox = click.event_target();
    commands.queue(move |world: &mut World| {
        let Some(target) = target_of(world, checkbox) else {
            return;
        };
        let on = target.get(world).ok().and_then(|v| v.as_bool());
        if let Some(on) = on
            && let Err(error) = target.set(world, json!(!on))
        {
            warn!("Could not change the field: {error}");
        }
    });
}

/// What a number box edits, and how its value is shown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Scrub {
    /// The whole field is one number.
    Scalar,
    /// One axis of a vector.
    Vector(usize),
    /// One axis of a rotation, in degrees.
    Rotation(usize),
}

impl Scrub {
    /// The field's value after dragging `pixels` from where it was at
    /// `start`, and the text the box then shows.
    pub(super) fn scrubbed(self, start: &Value, pixels: f32) -> Option<(Value, String)> {
        let pixels = f64::from(pixels);
        match self {
            Self::Scalar => {
                let value = start.as_f64()? + pixels * UNITS_PER_PIXEL;
                Some((json!(value), number(value)))
            }
            Self::Vector(axis) => {
                let mut axes = numbers(start)?;
                *axes.get_mut(axis)? += pixels * UNITS_PER_PIXEL;
                Some((same_shape(start, &axes), number(axes[axis])))
            }
            Self::Rotation(axis) => {
                let mut degrees = quat_degrees(start)?.to_array().map(f64::from);
                *degrees.get_mut(axis)? += pixels * DEGREES_PER_PIXEL;
                let [x, y, z] = degrees.map(|d| (d as f32).to_radians());
                let quat = Quat::from_euler(EulerRot::YXZ, y, x, z);
                let value = same_shape(start, &quat.to_array().map(f64::from));
                Some((value, format!("{}°", number(degrees[axis]))))
            }
        }
    }
}

/// The text a box shows for `value`.
fn shown_text(scrub: Scrub, value: &Value) -> Option<String> {
    match scrub {
        Scrub::Scalar => value.as_f64().map(number),
        Scrub::Vector(axis) => numbers(value)?.get(axis).copied().map(number),
        Scrub::Rotation(axis) => {
            let degrees = quat_degrees(value)?.to_array();
            Some(format!("{}°", number((*degrees.get(axis)?).into())))
        }
    }
}

/// A vector's numbers, written as `[x, y, z]` or `{"x": .., "y": .., ..}`.
fn numbers(value: &Value) -> Option<Vec<f64>> {
    match value {
        Value::Array(items) => items.iter().map(Value::as_f64).collect(),
        Value::Object(fields) => ["x", "y", "z", "w"][..fields.len().min(4)]
            .iter()
            .map(|axis| fields.get(*axis).and_then(Value::as_f64))
            .collect(),
        _ => None,
    }
}

/// `axes` written the way `like` was, so `component.set` reads it back.
fn same_shape(like: &Value, axes: &[f64]) -> Value {
    match like {
        Value::Object(_) => Value::Object(
            ["x", "y", "z", "w"]
                .iter()
                .zip(axes)
                .map(|(axis, n)| ((*axis).to_owned(), json!(n)))
                .collect(),
        ),
        _ => json!(axes),
    }
}

/// One box per axis, letters coloured like Godot's.
fn number_boxes(value: &Value, scrub: fn(usize) -> Scrub) -> Box<dyn Scene> {
    let axes = numbers(value).map_or(3, |axes| axes.len().min(3));
    let boxes: Vec<_> = ['x', 'y', 'z'][..axes]
        .iter()
        .enumerate()
        .map(|(axis, letter)| {
            let scrub = scrub(axis);
            let text = shown_text(scrub, value).unwrap_or_default();
            number_box(scrub, Some(*letter), text)
        })
        .collect();
    Box::new(bsn! {
        Node {
            flex_grow: 1.0,
            min_width: px(0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(2),
        }
        Children [ {boxes} ]
    })
}

/// What a number box edits. Its field's [`FieldTarget`] is on an ancestor.
#[derive(Component, Clone, Copy, Debug)]
struct NumberScrub(Scrub);

impl Default for NumberScrub {
    fn default() -> Self {
        Self(Scrub::Scalar)
    }
}

/// The field's value when the drag on this box began.
#[derive(Component, Clone, Debug)]
struct ScrubStart(Value);

/// Godot's dark value box.
#[derive(Component, Default, Clone)]
struct FieldBox;

/// An axis letter, coloured like Godot's x/y/z.
#[derive(Component, Default, Clone)]
struct AxisLetter(char);

/// Value text in the theme's font colour.
#[derive(Component, Default, Clone)]
struct FieldText;

/// The number in a box, rewritten while scrubbing.
#[derive(Component, Default, Clone)]
struct BoxNumber;

fn number_box(scrub: Scrub, letter: Option<char>, text: String) -> impl Scene {
    let letter = letter.map(|letter| {
        bsn! {
            @field_text(letter.to_string())
            AxisLetter(letter)
            Pickable::IGNORE
        }
    });
    bsn! {
        Node {
            flex_grow: 1.0,
            flex_basis: px(0),
            min_width: px(0),
            height: px(FIELD_HEIGHT),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(6),
            padding: UiRect::horizontal(px(6)),
            border_radius: BorderRadius::all(px(3)),
            overflow: Overflow::clip(),
        }
        FieldBox
        BackgroundColor
        NumberScrub({scrub})
        EntityCursor::System(SystemCursorIcon::EwResize)
        on(start_scrub)
        on(drag_scrub)
        on(end_scrub)
        Children [
            {letter}
            --
            @field_text(text) BoxNumber Pickable::IGNORE
        ]
    }
}

fn field_text(text: String) -> impl Scene {
    bsn! {
        Text(text)
        FieldText
        TextFont {
            font: FontSourceTemplate::Handle(paths::UI),
            font_size: FontSize::Px(FONT_SIZE),
        }
        TextLayout { linebreak: LineBreak::NoWrap }
    }
}

fn start_scrub(drag: On<PointerDragStart>, mut commands: Commands) {
    if drag.button != PointerButton::Primary {
        return;
    }
    let entity = drag.event_target();
    commands.queue(move |world: &mut World| {
        let Some(start) = target_of(world, entity).and_then(|t| t.get(world).ok()) else {
            return;
        };
        world.entity_mut(entity).insert(ScrubStart(start));
        world.resource_mut::<History>().begin_gesture();
        // The pointer soon leaves the box; the cursor shouldn't change back.
        if let Some(mut cursor) = world.get_resource_mut::<OverrideCursor>() {
            cursor.0 = Some(EntityCursor::System(SystemCursorIcon::EwResize));
        }
    });
}

fn drag_scrub(drag: On<PointerDrag>, mut commands: Commands) {
    if drag.button != PointerButton::Primary {
        return;
    }
    let (entity, pixels) = (drag.event_target(), drag.distance.x);
    commands.queue(move |world: &mut World| {
        let (Some(NumberScrub(scrub)), Some(ScrubStart(start)), Some(target)) = (
            world.get::<NumberScrub>(entity).copied(),
            world.get::<ScrubStart>(entity).cloned(),
            target_of(world, entity),
        ) else {
            return;
        };
        if let Some(text) = scrub_to(world, &target, scrub, &start, pixels) {
            show_number(world, entity, text);
        }
    });
}

fn end_scrub(drag: On<PointerDragEnd>, mut commands: Commands) {
    if drag.button != PointerButton::Primary {
        return;
    }
    let entity = drag.event_target();
    commands.queue(move |world: &mut World| {
        if let Ok(mut number_box) = world.get_entity_mut(entity) {
            number_box.remove::<ScrubStart>();
        }
        world.resource_mut::<History>().end_gesture();
        if let Some(mut cursor) = world.get_resource_mut::<OverrideCursor>() {
            cursor.0 = None;
        }
    });
}

/// The [`FieldTarget`] of the field `entity` is part of.
fn target_of(world: &World, entity: Entity) -> Option<FieldTarget> {
    let mut current = Some(entity);
    while let Some(entity) = current {
        if let Some(target) = world.get::<FieldTarget>(entity) {
            return Some(target.clone());
        }
        current = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    None
}

/// Set the field to where `pixels` of drag from `start` takes it. Returns
/// the text to show. Between `begin_gesture` and `end_gesture` the edits
/// merge into one undo step.
pub(super) fn scrub_to(
    world: &mut World,
    target: &FieldTarget,
    scrub: Scrub,
    start: &Value,
    pixels: f32,
) -> Option<String> {
    let (value, text) = scrub.scrubbed(start, pixels)?;
    match target.set(world, value) {
        Ok(_) => Some(text),
        Err(error) => {
            warn!("Could not change the field: {error}");
            None
        }
    }
}

/// The Inspector doesn't rebuild during a drag (that would despawn the box
/// being dragged), so the box shows its own new value.
fn show_number(world: &mut World, number_box: Entity, text: String) {
    let children: Vec<Entity> = world
        .get::<Children>(number_box)
        .map(|children| children.to_vec())
        .unwrap_or_default();
    for child in children {
        if world.get::<BoxNumber>(child).is_some()
            && let Some(mut shown) = world.get_mut::<Text>(child)
        {
            shown.0.clone_from(&text);
        }
    }
}

/// Colour new fields from the theme.
fn paint_fields(
    palette: Res<ActivePalette>,
    mut boxes: Query<&mut BackgroundColor, Added<FieldBox>>,
    mut texts: Query<(&mut TextColor, Option<&AxisLetter>), Added<FieldText>>,
) {
    for mut background in &mut boxes {
        background.0 = palette.dark_1;
    }
    for (mut colour, letter) in &mut texts {
        colour.0 = match letter.map(|letter| letter.0) {
            Some('x') => palette.axis_x,
            Some('y') => palette.axis_y,
            Some('z') => palette.axis_z,
            Some(_) => palette.font_secondary,
            None => palette.font,
        };
    }
}
