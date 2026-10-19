//! Inspector fields: how the Inspector draws and edits a field of one Rust
//! type. The editor's own Vec3, rotation, number and checkbox fields register
//! this way, and so do an extension's.
//!
//! ```ignore
//! struct CelsiusField;
//!
//! impl InspectorField for CelsiusField {
//!     type Value = Celsius;
//!
//!     fn draw(target: &FieldTarget, value: &Value) -> Box<dyn Scene> {
//!         let text = format!("{} °C", value);
//!         Box::new(bsn! { Text(text) })
//!     }
//! }
//!
//! app.add_inspector_field::<CelsiusField>();
//! ```
//!
//! A field changes the value only through its [`FieldTarget`], which runs
//! `component.set`, so every edit can be undone. A drag wraps its edits in a
//! gesture (`History::begin_gesture`) so the whole drag is one undo step.

use std::collections::HashMap;

use bevy::{prelude::*, reflect::TypePath};
use oddity_core::{CommandError, EditorWorldExt, reflect_json, scripts::Scripts};
use serde_json::{Value, json};

/// How the Inspector draws one field type.
pub trait InspectorField: Send + Sync + 'static {
    /// The Rust type it draws: every field of this type uses it.
    type Value: TypePath;

    /// The field's editor, showing `value` as reflection writes it as JSON.
    /// Its edits go to `target`.
    fn draw(target: &FieldTarget, value: &Value) -> Box<dyn Scene>;
}

/// Draws one field.
pub type DrawFieldFn = fn(&FieldTarget, &Value) -> Box<dyn Scene>;

/// Every registered Inspector field, by the type path of what it draws.
#[derive(Resource, Default, Debug, Clone)]
pub struct InspectorFields {
    fields: HashMap<&'static str, DrawFieldFn>,
}

impl InspectorFields {
    /// Draw fields of type `F::Value` with `F`, replacing any field before.
    pub fn add<F: InspectorField>(&mut self) {
        self.fields.insert(F::Value::type_path(), F::draw);
    }

    /// The field that draws values of `type_path`, if one is registered.
    pub fn get(&self, type_path: &str) -> Option<DrawFieldFn> {
        self.fields.get(type_path).copied()
    }
}

/// Where a field's edits go: one field of one component on one node.
/// Fields keep it on their root entity. A script's field (scripts are data
/// in the editor) is addressed as the component `script:<type path>`, and
/// its edits go through `script.set_field`.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct FieldTarget {
    /// The node.
    pub entity: Entity,
    /// The component's type path, or `script:<type path>` for a script.
    pub component: String,
    /// The field inside it, as `component.set` takes it (empty: all of it).
    pub path: String,
}

impl FieldTarget {
    /// The field's value now, as JSON.
    pub fn get(&self, world: &World) -> Result<Value, CommandError> {
        let Some(script) = self.script() else {
            return reflect_json::field_json(world, self.entity, &self.component, &self.path);
        };
        let missing = || CommandError::MissingComponent {
            entity: self.entity,
            component: script.to_owned(),
        };
        let values = &world
            .get::<Scripts>(self.entity)
            .and_then(|scripts| scripts.get(script))
            .ok_or_else(missing)?
            .values
            .0;
        let mut steps = self.path.split('.');
        let mut at = steps.next().and_then(|first| values.get(first));
        for step in steps {
            at = at.and_then(|value| value.get(step));
        }
        Ok(at.cloned().unwrap_or(Value::Null))
    }

    /// The script's type path, if this is a script's field.
    pub fn script(&self) -> Option<&str> {
        self.component.strip_prefix("script:")
    }

    /// Set the field to `value` (JSON in the field's own shape) with
    /// `component.set` (`script.set_field` for a script), as the user.
    pub fn set(&self, world: &mut World, value: Value) -> Result<Value, CommandError> {
        if let Some(script) = self.script() {
            return world.run_command(
                "script.set_field",
                json!({
                    "entity": self.entity.to_bits(),
                    "script": script,
                    "field": self.path,
                    "value": value,
                }),
            );
        }
        world.run_command(
            "component.set",
            json!({
                "entity": self.entity.to_bits(),
                "component": self.component,
                "path": self.path,
                "value": value,
            }),
        )
    }
}
