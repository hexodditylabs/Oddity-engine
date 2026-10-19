//! `cargo xtask layers`: the crate layer rules from `roadmap/modularity.md`,
//! checked against `cargo metadata`.
//!
//! Each workspace crate declares its place in its `Cargo.toml`:
//!
//! ```toml
//! [package.metadata.oddity]
//! layer = "runtime"   # runtime | tooling | extension | app | dev
//! role = "backend"    # optional: "facade" or "backend"
//! headless = true     # optional: must not reach `bevy` or rendering
//! ```
//!
//! Those three keys are the only ones allowed, so a misspelt key fails the
//! check instead of silently switching a rule off.
//!
//! The rules:
//!
//! - every crate declares a layer;
//! - runtime depends only on runtime; tooling on runtime and tooling;
//!   extensions on runtime and tooling, never on each other; apps on
//!   anything; only `dev` crates (this one) may depend on `dev` crates;
//! - no workspace crate depends on the facade (only games do);
//! - at most one crate is the facade: `oddity`, in the runtime layer;
//! - only the facade depends on a back end;
//! - a headless crate never reaches `bevy` or a rendering or windowing
//!   crate ([`NOT_HEADLESS`]), directly or through other workspace crates.
//!
//! Dev-dependencies are exempt, so tests can use whatever they need.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    env,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use serde_json::Value;

/// The keys `[package.metadata.oddity]` may contain.
const KNOWN_KEYS: [&str; 3] = ["layer", "role", "headless"];

/// The only crate allowed to be the facade; games depend on it by this name.
const FACADE: &str = "oddity";

/// Crates a headless crate must never reach. `bevy` itself is here because
/// its default features pull in the renderer and a window; headless crates
/// use the sub-crates they need (`bevy_ecs`, `bevy_app`, `bevy_reflect`).
const NOT_HEADLESS: &[&str] = &[
    "bevy",
    "bevy_render",
    "bevy_ui",
    "bevy_winit",
    "bevy_window",
    "bevy_feathers",
    "bevy_pbr",
    "bevy_sprite",
    "bevy_core_pipeline",
    "bevy_gizmos",
    "wgpu",
    "winit",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layer {
    Runtime,
    Tooling,
    Extension,
    App,
    Dev,
}

impl Layer {
    fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "runtime" => Self::Runtime,
            "tooling" => Self::Tooling,
            "extension" => Self::Extension,
            "app" => Self::App,
            "dev" => Self::Dev,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::Tooling => "tooling",
            Self::Extension => "extension",
            Self::App => "app",
            Self::Dev => "dev",
        }
    }

    /// Whether a crate in this layer may depend on one in `other`, and if
    /// not, why.
    fn may_depend_on(self, other: Layer) -> Result<(), &'static str> {
        use Layer::*;
        match (self, other) {
            (Dev, _) | (App, Runtime | Tooling | Extension | App) => Ok(()),
            (_, Dev) => Err("only dev tools depend on dev crates"),
            (Runtime, Runtime) => Ok(()),
            (Runtime, _) => Err("a shipped game must never contain editor code"),
            (Tooling | Extension, Runtime | Tooling) => Ok(()),
            (Extension, Extension) => {
                Err("extensions never depend on each other, so each can be removed on its own")
            }
            (Tooling | Extension, _) => Err("a crate depends only on its own layer or a lower one"),
        }
    }
}

/// A workspace crate, as far as the rules care.
#[derive(Clone, Debug, Default)]
struct Crate {
    name: String,
    /// `layer` as written in the manifest.
    layer: Option<String>,
    /// `role` as written in the manifest.
    role: Option<String>,
    headless: bool,
    /// What is wrong with `[package.metadata.oddity]` itself.
    metadata_errors: Vec<String>,
    /// Normal and build dependencies, by package name.
    deps: Vec<String>,
}

impl Crate {
    fn is(&self, role: &str) -> bool {
        self.role.as_deref() == Some(role)
    }
}

pub fn run() -> ExitCode {
    let crates = match workspace_crates() {
        Ok(crates) => crates,
        Err(error) => {
            eprintln!("layers: {error}");
            return ExitCode::FAILURE;
        }
    };
    let problems = check(&crates);
    if problems.is_empty() {
        println!("layers: {} crates follow the layer rules", crates.len());
        return ExitCode::SUCCESS;
    }
    for problem in &problems {
        eprintln!("error: {problem}");
    }
    eprintln!(
        "layers: {} problem(s); see roadmap/modularity.md#layers",
        problems.len()
    );
    ExitCode::FAILURE
}

fn workspace_crates() -> Result<Vec<Crate>, String> {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .arg("--manifest-path")
        .arg(workspace_manifest())
        .output()
        .map_err(|error| format!("could not run cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let metadata: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("could not read cargo metadata: {error}"))?;
    Ok(parse(&metadata))
}

/// The workspace's root manifest, found from this crate's own location so
/// the check covers the right workspace whatever folder it is run from.
fn workspace_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml")
}

/// The crates in `cargo metadata --no-deps` output, which lists only the
/// workspace's own packages.
fn parse(metadata: &Value) -> Vec<Crate> {
    let text = |value: &Value| value.as_str().map(str::to_owned);
    metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|package| {
            let name = text(&package["name"]).unwrap_or_default();
            let oddity = &package["metadata"]["oddity"];
            let mut metadata_errors = Vec::new();
            match oddity {
                Value::Null => {}
                Value::Object(table) => {
                    for key in table.keys().filter(|key| !KNOWN_KEYS.contains(&key.as_str())) {
                        metadata_errors.push(format!(
                            "`{name}` has unknown key `{key}` in [package.metadata.oddity]; use layer, role or headless"
                        ));
                    }
                }
                _ => metadata_errors.push(format!(
                    "`{name}` has a [package.metadata.oddity] that is not a table"
                )),
            }
            let mut string = |key: &str| match &oddity[key] {
                Value::Null => None,
                Value::String(value) => Some(value.clone()),
                other => {
                    metadata_errors.push(format!(
                        "`{name}` sets {key} = {other}; it must be a string"
                    ));
                    None
                }
            };
            let layer = string("layer");
            let role = string("role");
            let headless = match &oddity["headless"] {
                Value::Null => false,
                Value::Bool(headless) => *headless,
                other => {
                    metadata_errors.push(format!(
                        "`{name}` sets headless = {other}; write true or false, without quotes"
                    ));
                    false
                }
            };
            Crate {
                name,
                layer,
                role,
                headless,
                metadata_errors,
                deps: package["dependencies"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|dep| dep["kind"].as_str() != Some("dev"))
                    .filter_map(|dep| text(&dep["name"]))
                    .collect(),
            }
        })
        .collect()
}

/// Every broken rule, one message each, in crate name order.
fn check(crates: &[Crate]) -> Vec<String> {
    let by_name: BTreeMap<&str, &Crate> = crates.iter().map(|c| (c.name.as_str(), c)).collect();
    let mut problems = Vec::new();

    for krate in by_name.values() {
        let name = &krate.name;
        problems.extend(krate.metadata_errors.iter().cloned());
        let layer = match krate.layer.as_deref() {
            None => {
                problems.push(format!(
                    "`{name}` declares no layer: add `[package.metadata.oddity] layer = \"...\"`"
                ));
                None
            }
            Some(text) => {
                let layer = Layer::parse(text);
                if layer.is_none() {
                    problems.push(format!(
                        "`{name}` declares layer \"{text}\"; use runtime, tooling, extension, app or dev"
                    ));
                }
                layer
            }
        };
        if let Some(role) = krate.role.as_deref()
            && role != "facade"
            && role != "backend"
        {
            problems.push(format!(
                "`{name}` declares role \"{role}\"; use facade or backend"
            ));
        }
        if krate.headless {
            for (banned, path) in not_headless_reached(krate, &by_name) {
                problems.push(if path.is_empty() {
                    format!(
                        "`{name}` is headless but depends on `{banned}`; use bevy_ecs, bevy_app or bevy_reflect"
                    )
                } else {
                    format!(
                        "`{name}` is headless but reaches `{banned}` through `{}`",
                        path.join("` -> `")
                    )
                });
            }
        }

        for dep in krate
            .deps
            .iter()
            .filter_map(|dep| by_name.get(dep.as_str()))
        {
            let dep_name = &dep.name;
            if let (Some(layer), Some(dep_layer)) =
                (layer, dep.layer.as_deref().and_then(Layer::parse))
                && let Err(why) = layer.may_depend_on(dep_layer)
            {
                problems.push(format!(
                    "`{name}` ({}) depends on `{dep_name}` ({}): {why}",
                    layer.name(),
                    dep_layer.name()
                ));
            }
            if dep.is("facade") {
                problems.push(format!(
                    "`{name}` depends on the facade `{dep_name}`: only games do; depend on the runtime crates it re-exports"
                ));
            }
            if dep.is("backend") && !krate.is("facade") {
                problems.push(format!(
                    "`{name}` depends on the back end `{dep_name}`: only the facade does"
                ));
            }
        }
    }

    let facades: Vec<&Crate> = by_name
        .values()
        .copied()
        .filter(|krate| krate.is("facade"))
        .collect();
    if facades.len() > 1 {
        let names: Vec<&str> = facades.iter().map(|krate| krate.name.as_str()).collect();
        problems.push(format!(
            "{} crates declare role \"facade\" (`{}`); only `{FACADE}` is the facade",
            facades.len(),
            names.join("`, `")
        ));
    }
    for facade in facades {
        let name = &facade.name;
        if name != FACADE {
            problems.push(format!(
                "`{name}` declares role \"facade\"; only `{FACADE}` is the facade games depend on"
            ));
        }
        if let Some(layer) = facade.layer.as_deref()
            && layer != "runtime"
        {
            problems.push(format!(
                "the facade `{name}` is in layer \"{layer}\"; it ships in games, so it must be runtime"
            ));
        }
    }
    problems
}

/// Every crate in [`NOT_HEADLESS`] that `krate` reaches through its
/// non-dev dependencies, each with the shortest chain of workspace crates in
/// between (empty for a direct dependency).
fn not_headless_reached<'a>(
    krate: &'a Crate,
    by_name: &BTreeMap<&str, &'a Crate>,
) -> BTreeMap<&'a str, Vec<&'a str>> {
    let mut reached = BTreeMap::new();
    let mut seen = BTreeSet::from([krate.name.as_str()]);
    let mut queue = VecDeque::from([(krate, Vec::new())]);
    while let Some((current, path)) = queue.pop_front() {
        for dep in &current.deps {
            let dep = dep.as_str();
            if NOT_HEADLESS.contains(&dep) {
                reached.entry(dep).or_insert_with(|| path.clone());
            } else if let Some(next) = by_name.get(dep)
                && seen.insert(dep)
            {
                let mut next_path = path.clone();
                next_path.push(dep);
                queue.push_back((*next, next_path));
            }
        }
    }
    reached
}

#[cfg(test)]
mod tests {
    use super::*;

    fn krate(name: &str, layer: &str, deps: &[&str]) -> Crate {
        Crate {
            name: name.into(),
            layer: Some(layer.into()),
            deps: deps.iter().map(|d| (*d).into()).collect(),
            ..Crate::default()
        }
    }

    fn with_role(mut krate: Crate, role: &str) -> Crate {
        krate.role = Some(role.into());
        krate
    }

    /// Asserts exactly one problem, mentioning `needle`.
    fn fails_with(crates: &[Crate], needle: &str) {
        let problems = check(crates);
        assert_eq!(problems.len(), 1, "{problems:#?}");
        assert!(problems[0].contains(needle), "{problems:#?}");
    }

    #[test]
    fn todays_workspace_passes() {
        let mut core = krate("oddity_core", "tooling", &["bevy_ecs", "bevy_app"]);
        core.headless = true;
        let crates = [
            core,
            krate("oddity_icons", "tooling", &["bevy"]),
            krate("oddity_widgets", "tooling", &["bevy", "oddity_icons"]),
            krate("oddity_project", "runtime", &["toml"]),
            krate(
                "oddity-editor",
                "app",
                &[
                    "oddity_core",
                    "oddity_icons",
                    "oddity_project",
                    "oddity_widgets",
                ],
            ),
            krate("xtask", "dev", &["serde_json"]),
        ];
        assert_eq!(check(&crates), Vec::<String>::new());
    }

    #[test]
    fn runtime_must_not_depend_on_tooling() {
        fails_with(
            &[
                krate("oddity_level", "runtime", &["oddity_core"]),
                krate("oddity_core", "tooling", &[]),
            ],
            "`oddity_level` (runtime) depends on `oddity_core` (tooling)",
        );
    }

    #[test]
    fn extensions_must_not_depend_on_each_other() {
        fails_with(
            &[
                krate(
                    "oddity_steam_editor",
                    "extension",
                    &["oddity_physics_editor"],
                ),
                krate("oddity_physics_editor", "extension", &[]),
            ],
            "extensions never depend on each other",
        );
    }

    #[test]
    fn only_the_facade_depends_on_a_back_end() {
        let rapier = with_role(krate("oddity_physics_rapier", "runtime", &[]), "backend");
        fails_with(
            &[
                krate("oddity_physics", "runtime", &["oddity_physics_rapier"]),
                rapier.clone(),
            ],
            "depends on the back end `oddity_physics_rapier`",
        );
        let facade = with_role(
            krate("oddity", "runtime", &["oddity_physics_rapier"]),
            "facade",
        );
        assert_eq!(check(&[facade, rapier]), Vec::<String>::new());
    }

    #[test]
    fn workspace_crates_must_not_depend_on_the_facade() {
        fails_with(
            &[
                krate("oddity_physics_editor", "extension", &["oddity"]),
                with_role(krate("oddity", "runtime", &[]), "facade"),
            ],
            "depends on the facade `oddity`",
        );
    }

    #[test]
    fn headless_crates_must_not_depend_on_bevy() {
        let mut core = krate("oddity_core", "tooling", &["bevy"]);
        core.headless = true;
        fails_with(&[core], "is headless but depends on `bevy`");
    }

    #[test]
    fn headless_crates_must_not_reach_rendering_through_workspace_crates() {
        let mut core = krate("oddity_core", "tooling", &["bevy_ecs", "oddity_plugin_api"]);
        core.headless = true;
        fails_with(
            &[
                core.clone(),
                krate("oddity_plugin_api", "tooling", &["oddity_widgets"]),
                krate("oddity_widgets", "tooling", &["bevy"]),
            ],
            "`oddity_core` is headless but reaches `bevy` through `oddity_plugin_api` -> `oddity_widgets`",
        );
        let mut core = krate("oddity_core", "tooling", &["oddity_widgets"]);
        core.headless = true;
        fails_with(
            &[core, krate("oddity_widgets", "tooling", &["bevy"])],
            "`oddity_core` is headless but reaches `bevy` through `oddity_widgets`",
        );
    }

    #[test]
    fn headless_crates_must_not_depend_on_rendering_crates() {
        for banned in ["bevy_render", "bevy_ui", "bevy_winit", "wgpu", "winit"] {
            let mut core = krate("oddity_core", "tooling", &["bevy_ecs", banned]);
            core.headless = true;
            fails_with(
                &[core],
                &format!("`oddity_core` is headless but depends on `{banned}`"),
            );
        }
        let mut core = krate(
            "oddity_core",
            "tooling",
            &["bevy_ecs", "bevy_app", "bevy_reflect", "oddity_project"],
        );
        core.headless = true;
        let crates = [core, krate("oddity_project", "runtime", &["toml"])];
        assert_eq!(check(&crates), Vec::<String>::new());
    }

    #[test]
    fn metadata_rejects_unknown_keys_and_non_boolean_headless() {
        let metadata = serde_json::json!({
            "packages": [{
                "name": "oddity_core",
                "metadata": { "oddity": {
                    "layer": "tooling",
                    "roles": "backend",
                    "headless": "true",
                } },
                "dependencies": [],
            }],
        });
        let problems = check(&parse(&metadata));
        assert_eq!(problems.len(), 2, "{problems:#?}");
        assert!(
            problems.iter().any(|p| p.contains("unknown key `roles`")),
            "{problems:#?}"
        );
        assert!(
            problems
                .iter()
                .any(|p| p.contains("sets headless = \"true\"; write true or false")),
            "{problems:#?}"
        );
        let metadata = serde_json::json!({
            "packages": [{
                "name": "oddity_core",
                "metadata": { "oddity": { "layer": "tooling", "headles": true } },
                "dependencies": [],
            }],
        });
        fails_with(&parse(&metadata), "unknown key `headles`");
    }

    #[test]
    fn the_facade_is_oddity_alone_in_the_runtime_layer() {
        fails_with(
            &[with_role(krate("oddity_physics", "runtime", &[]), "facade")],
            "`oddity_physics` declares role \"facade\"",
        );
        fails_with(
            &[with_role(krate("oddity", "tooling", &[]), "facade")],
            "the facade `oddity` is in layer \"tooling\"",
        );
        let problems = check(&[
            with_role(krate("oddity", "runtime", &[]), "facade"),
            with_role(krate("oddity_game", "runtime", &[]), "facade"),
        ]);
        assert!(
            problems
                .iter()
                .any(|p| p.contains("2 crates declare role \"facade\" (`oddity`, `oddity_game`)")),
            "{problems:#?}"
        );
        assert_eq!(
            check(&[with_role(krate("oddity", "runtime", &[]), "facade")]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn checks_the_workspace_this_tool_belongs_to() {
        let manifest = std::fs::read_to_string(workspace_manifest()).unwrap();
        assert!(manifest.contains("[workspace]"), "{manifest}");
        assert!(manifest.contains("\"tools/*\""), "{manifest}");
    }

    #[test]
    fn every_crate_declares_a_valid_layer() {
        let mut missing = krate("oddity_new", "", &[]);
        missing.layer = None;
        fails_with(&[missing], "declares no layer");
        fails_with(
            &[krate("oddity_new", "editor", &[])],
            "declares layer \"editor\"",
        );
    }

    #[test]
    fn only_dev_tools_depend_on_dev_crates() {
        fails_with(
            &[
                krate("oddity-editor", "app", &["xtask"]),
                krate("xtask", "dev", &[]),
            ],
            "only dev tools depend on dev crates",
        );
    }

    #[test]
    fn dev_dependencies_are_exempt() {
        let metadata = serde_json::json!({
            "packages": [{
                "name": "oddity_project",
                "metadata": { "oddity": { "layer": "runtime" } },
                "dependencies": [
                    { "name": "toml", "kind": null },
                    { "name": "oddity_core", "kind": "dev" },
                ],
            }],
        });
        let crates = parse(&metadata);
        assert_eq!(crates[0].layer.as_deref(), Some("runtime"));
        assert_eq!(crates[0].deps, ["toml"]);
    }
}
