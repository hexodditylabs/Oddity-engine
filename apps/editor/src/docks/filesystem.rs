//! The FileSystem dock: the open project's files, as in Godot.
//!
//! The project folder is read once when the editor starts. Watching it for
//! changes, previews and drag-and-drop into the level arrive with asset
//! handling. Folders start collapsed except `res://`, as in Godot, and build
//! output (`target/`) and hidden files are left out.

use std::{
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
};

use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
    text::FontSourceTemplate,
};
use oddity_icons::{Icon, NodeColor};
use oddity_widgets::{
    field::{SearchText, search_field},
    fonts::paths,
    tree::{TreeEvent, TreeItem, TreeView},
};

use super::DimText;
use crate::project::CurrentProject;

pub struct FileSystemPlugin;

impl Plugin for FileSystemPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FileSystemState>()
            .add_systems(Startup, scan_project)
            .add_systems(Update, (handle_events, refresh).chain());
    }
}

/// One file or folder.
#[derive(Debug, Clone)]
struct Entry {
    path: PathBuf,
    name: String,
    /// Folders only, sorted folders first, then by name.
    children: Vec<Entry>,
    is_dir: bool,
}

/// What the dock shows and how it is unfolded.
#[derive(Resource, Default)]
struct FileSystemState {
    root: Option<Entry>,
    expanded: HashSet<PathBuf>,
    selected: Option<PathBuf>,
    /// Row keys back to paths, rebuilt with the rows.
    paths: HashMap<u64, PathBuf>,
}

#[derive(Component, Default, Clone)]
struct FileTree;

#[derive(Component, Default, Clone)]
struct FileFilter;

/// Shown instead of the tree when no project is open.
#[derive(Component, Default, Clone)]
struct NoProjectNote;

pub fn filesystem_page() -> impl Scene {
    bsn! {
        Node { flex_grow: 1.0, flex_direction: FlexDirection::Column, min_height: px(0) }
        Children [
            Node { flex_shrink: 0.0, padding: UiRect::axes(px(6), px(6)) }
            Children [ @search_field("Filter Files") FileFilter Node { flex_grow: 1.0 } ]
            --
            Node {
                flex_grow: 1.0,
                min_height: px(0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::top(px(2)),
                overflow: Overflow::scroll_y(),
            }
            TreeView
            FileTree
            --
            Text("No project open.\nRun: cargo editor -- path/to/game")
            NoProjectNote
            DimText
            Node { display: Display::None, margin: UiRect::all(px(16)) }
            TextLayout { justify: Justify::Center }
            TextFont {
                font: FontSourceTemplate::Handle(paths::UI),
                font_size: FontSize::Px(14.0),
            }
        ]
    }
}

fn scan_project(project: Res<CurrentProject>, mut state: ResMut<FileSystemState>) {
    let Some(project) = &project.0 else {
        return;
    };
    let root = scan(&project.root, "res://".to_owned());
    state.expanded.insert(root.path.clone());
    state.root = Some(root);
}

fn scan(path: &Path, name: String) -> Entry {
    let mut children: Vec<Entry> = fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|item| {
            let name = item.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "target" {
                return None;
            }
            let path = item.path();
            Some(if path.is_dir() {
                scan(&path, name)
            } else {
                Entry {
                    path,
                    name,
                    children: Vec::new(),
                    is_dir: false,
                }
            })
        })
        .collect();
    children.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
    Entry {
        path: path.to_path_buf(),
        name,
        children,
        is_dir: true,
    }
}

/// Godot's icon for a file, by extension.
fn file_icon(entry: &Entry) -> Icon {
    if entry.is_dir {
        return Icon::godot("Folder").tinted(NodeColor::Folder);
    }
    let extension = entry
        .path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    Icon::godot(match extension.as_str() {
        "rs" => "Script",
        "level" | "prefab" | "bsn" => "PackedScene",
        "png" | "jpg" | "jpeg" | "webp" | "svg" | "ktx2" => "ImageTexture",
        "glb" | "gltf" | "obj" => "Mesh",
        "ogg" | "wav" | "mp3" | "flac" => "AudioStream",
        "ttf" | "otf" => "Font",
        "wgsl" => "Shader",
        "toml" | "md" | "txt" | "json" | "ron" => "TextFile",
        _ => "File",
    })
}

fn key_of(path: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

fn refresh(
    mut state: ResMut<FileSystemState>,
    filters: Query<&SearchText, With<FileFilter>>,
    mut trees: Query<(&mut TreeView, &mut Node), With<FileTree>>,
    mut notes: Query<&mut Node, (With<NoProjectNote>, Without<FileTree>)>,
) {
    let has_project = state.root.is_some();
    for mut note in &mut notes {
        let display = if has_project {
            Display::None
        } else {
            Display::Flex
        };
        if note.display != display {
            note.display = display;
        }
    }
    let Some(root) = state.root.clone() else {
        return;
    };
    let filter = filters.iter().next().cloned().unwrap_or_default();
    let mut items = Vec::new();
    let mut keys = HashMap::new();
    push_entry(&root, 0, &state, &filter, &mut items, &mut keys);
    for (mut tree, _) in &mut trees {
        if tree.items != items {
            tree.items.clone_from(&items);
        }
    }
    state.paths = keys;
}

/// Add an entry and its visible contents. Returns whether anything was
/// added, so folders without a match are left out while filtering.
fn push_entry(
    entry: &Entry,
    depth: u16,
    state: &FileSystemState,
    filter: &SearchText,
    items: &mut Vec<TreeItem>,
    keys: &mut HashMap<u64, PathBuf>,
) -> bool {
    let expanded = !filter.is_empty() || state.expanded.contains(&entry.path);
    let key = key_of(&entry.path);
    let index = items.len();
    items.push(TreeItem {
        key,
        depth,
        label: entry.name.clone(),
        icon: Some(file_icon(entry)),
        has_children: !entry.children.is_empty(),
        expanded,
        selected: state.selected.as_ref() == Some(&entry.path),
        dimmed: false,
        buttons: Vec::new(),
    });
    keys.insert(key, entry.path.clone());

    let mut any_child = false;
    if expanded {
        for child in &entry.children {
            any_child |= push_entry(child, depth + 1, state, filter, items, keys);
        }
    }
    // The root always shows, so the tree never goes blank while filtering.
    let keep = depth == 0 || filter.matches(&entry.name) || any_child;
    if !keep {
        items.truncate(index);
    }
    keep
}

fn handle_events(
    mut events: MessageReader<TreeEvent>,
    trees: Query<(), With<FileTree>>,
    mut state: ResMut<FileSystemState>,
) {
    for event in events.read() {
        match *event {
            TreeEvent::Select { tree, key, .. } if trees.contains(tree) => {
                state.selected = state.paths.get(&key).cloned();
            }
            // Double-clicking a folder opens it, as in Godot.
            TreeEvent::Toggle { tree, key } | TreeEvent::Activate { tree, key }
                if trees.contains(tree) =>
            {
                if let Some(path) = state.paths.get(&key).cloned()
                    && path.is_dir()
                    && !state.expanded.remove(&path)
                {
                    state.expanded.insert(path);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_come_first_and_build_output_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("oddity.toml"), "").unwrap();
        fs::write(dir.path().join(".gitignore"), "").unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/main.rs"), "").unwrap();
        fs::create_dir(dir.path().join("target")).unwrap();

        let root = scan(dir.path(), "res://".into());
        let names: Vec<&str> = root.children.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["src", "oddity.toml"]);
        assert_eq!(
            file_icon(&root.children[0].children[0]),
            Icon::godot("Script")
        );
    }
}
