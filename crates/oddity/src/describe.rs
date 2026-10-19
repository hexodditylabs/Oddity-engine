//! `--oddity-describe` (`roadmap/scripting.md`, "Build and describe"): the
//! game's scripts, signals and the types their fields use, as JSON, so the
//! editor can show and edit them without ever running game code.

use std::collections::BTreeMap;

use bevy::reflect::{
    GenericInfo, PartialReflect, TypeInfo, TypeRegistry, enums::VariantInfo,
    serde::TypedReflectSerializer,
};
use oddity_script::{ReflectScript, ReflectSignal, attributes::Export};
use serde_json::{Value, json};

/// Crates whose types the editor knows by itself: they aren't described.
const KNOWN: [&str; 7] = [
    "core::",
    "alloc::",
    "std::",
    "glam::",
    "bevy_",
    "oddity_script::",
    "uuid::",
];

/// The description of every script and signal in `registry`.
pub fn describe(registry: &TypeRegistry) -> Value {
    let mut types = BTreeMap::new();
    let mut scripts: Vec<Value> = registry
        .iter_with_data::<ReflectScript>()
        .filter_map(|(registration, script)| {
            describe_script(registry, registration.type_info(), script, &mut types)
        })
        .collect();
    scripts.sort_by(|a, b| a["type_path"].as_str().cmp(&b["type_path"].as_str()));
    let mut signals: Vec<&str> = registry
        .iter_with_data::<ReflectSignal>()
        .map(|(registration, _)| registration.type_info().type_path())
        .collect();
    signals.sort_unstable();
    json!({
        "scripts": scripts,
        "components": [],
        "signals": signals.iter().map(|path| json!({ "type_path": path })).collect::<Vec<_>>(),
        "types": types,
    })
}

fn describe_script(
    registry: &TypeRegistry,
    info: &TypeInfo,
    script: &ReflectScript,
    types: &mut BTreeMap<String, Value>,
) -> Option<Value> {
    let fields = info.as_struct().ok()?;
    let defaults = registry
        .get(info.type_id())?
        .data::<bevy::reflect::std_traits::ReflectDefault>()?
        .default();
    let defaults = defaults.reflect_ref().as_struct().ok()?;
    let mut saved = Vec::new();
    let mut events = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        if !field.has_attribute::<Export>() {
            continue;
        }
        let field_info = registry.get_type_info(field.type_id());
        collect_types(registry, field_info, types);
        let default = defaults
            .field_at(index)
            .and_then(|value| to_json(value, registry))
            .unwrap_or(Value::Null);
        saved.push(json!({
            "name": field.name(),
            "type": type_name(field_info, field.type_path()),
            "default": default,
        }));
        if field
            .type_path()
            .starts_with("oddity_script::events::Event<")
        {
            events.push(json!({
                "field": field.name(),
                "arg": field_info.and_then(event_arg),
            }));
        }
    }
    let listeners: Vec<Value> = script
        .listeners()
        .into_iter()
        .map(|(name, arg)| json!({ "name": name, "arg": arg }))
        .collect();
    Some(json!({
        "type_path": info.type_path(),
        "name": info.type_path_table().short_path(),
        "id": script.id(),
        "order": script.order(),
        "file": script.file().display().to_string().replace('\\', "/"),
        "fields": saved,
        "listeners": listeners,
        "events": events,
    }))
}

/// What an `Event<T>` carries: `T`'s type path, or `None` for `Event`.
fn event_arg(info: &TypeInfo) -> Option<&'static str> {
    match info.generics().iter().next()? {
        GenericInfo::Type(arg) if !arg.type_path().ends_with("NoValue") => Some(arg.type_path()),
        _ => None,
    }
}

/// How a field's type is written: a type described in `types` by its full
/// path, a known one by its short name (`f32`, `Vec3`, `NodeRef`).
fn type_name(info: Option<&TypeInfo>, type_path: &str) -> String {
    match info {
        Some(info) if !is_known(type_path) => info.type_path().to_owned(),
        // `Event` is how scripts write `Event<NoValue>`.
        Some(info) => info.type_path_table().short_path().replace("<NoValue>", ""),
        None => type_path.to_owned(),
    }
}

fn is_known(type_path: &str) -> bool {
    KNOWN.iter().any(|prefix| type_path.starts_with(prefix)) || !type_path.contains("::")
}

/// Describe the game's own structs and enums that `info` is or contains.
fn collect_types(
    registry: &TypeRegistry,
    info: Option<&TypeInfo>,
    types: &mut BTreeMap<String, Value>,
) {
    let Some(info) = info else {
        return;
    };
    for generic in info.generics().iter() {
        if let GenericInfo::Type(arg) = generic {
            collect_types(registry, registry.get_type_info(arg.type_id()), types);
        }
    }
    let path = info.type_path();
    if is_known(path) || types.contains_key(path) {
        return;
    }
    let field = |name: Option<&str>, type_id, type_path: &str| {
        let field_info = registry.get_type_info(type_id);
        let mut value = json!({ "type": type_name(field_info, type_path) });
        if let Some(name) = name {
            value["name"] = name.into();
        }
        (value, field_info)
    };
    let mut nested = Vec::new();
    let description = match info {
        TypeInfo::Struct(info) => {
            let fields: Vec<Value> = info
                .iter()
                .map(|f| {
                    let (value, inner) = field(Some(f.name()), f.type_id(), f.type_path());
                    nested.push(inner);
                    value
                })
                .collect();
            json!({ "kind": "struct", "fields": fields })
        }
        TypeInfo::TupleStruct(info) => {
            let fields: Vec<Value> = info
                .iter()
                .map(|f| {
                    let (value, inner) = field(None, f.type_id(), f.type_path());
                    nested.push(inner);
                    value
                })
                .collect();
            json!({ "kind": "tuple_struct", "fields": fields })
        }
        TypeInfo::Enum(info) => {
            let variants: Vec<Value> = info
                .iter()
                .map(|variant| {
                    let (kind, fields): (&str, Vec<Value>) = match variant {
                        VariantInfo::Unit(_) => ("unit", Vec::new()),
                        VariantInfo::Tuple(v) => (
                            "tuple",
                            v.iter()
                                .map(|f| {
                                    let (value, inner) = field(None, f.type_id(), f.type_path());
                                    nested.push(inner);
                                    value
                                })
                                .collect(),
                        ),
                        VariantInfo::Struct(v) => (
                            "struct",
                            v.iter()
                                .map(|f| {
                                    let (value, inner) =
                                        field(Some(f.name()), f.type_id(), f.type_path());
                                    nested.push(inner);
                                    value
                                })
                                .collect(),
                        ),
                    };
                    json!({ "name": variant.name(), "kind": kind, "fields": fields })
                })
                .collect();
            json!({ "kind": "enum", "variants": variants })
        }
        _ => return,
    };
    types.insert(path.to_owned(), description);
    for inner in nested {
        collect_types(registry, inner, types);
    }
}

fn to_json(value: &dyn PartialReflect, registry: &TypeRegistry) -> Option<Value> {
    serde_json::to_value(TypedReflectSerializer::new(value, registry)).ok()
}
