//! The open project's files, in two linked pages, like Godot's FileSystem in
//! split mode (and Unity's Project window), spread over two docks:
//!
//! ```text
//!  left dock: FileSystem       bottom panel: Files
//! ┌──────────────────┐        ┌─ res:// › src › enemies ──── [Filter Files 🔍] ┐
//! │ ▾ res://         │        │  [📁]      [📄]       [📄]                       │
//! │   ▸ assets       │  ───▶  │  ai       goblin.rs  orc.rs                       │
//! │   ▾ src          │        │                                                  │
//! │       enemies    │        │                                                  │
//! └──────────────────┘        └──────────────────────────────────────────────────┘
//! ```
//!
//! The tree shows folders only; picking one shows its contents in the grid.
//! Double-clicking a folder in the grid opens it, and the breadcrumb leads
//! back up. Typing in the filter searches the whole project, as Unity's
//! search does.
//!
//! The project folder is read once when the editor starts. Watching it for
//! changes, previews and drag-and-drop into the level arrive in Phase 3.
//! The project's build output (`target/`) and hidden files are left out.
//! The tree and grid are rebuilt only when what they show changes.

use std::{
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
};

use bevy::{
    ecs::schedule::SystemCondition,
    picking::{Pickable, events::PointerClick, pointer::PointerButton},
    platform::collections::{HashMap, HashSet},
    prelude::*,
    text::{EditableText, FontSourceTemplate},
    ui_widgets::ScrollArea,
};
use oddity_icons::{Icon, NodeColor, UiIcon};
use oddity_plugin_api::{DockSlot, DockTab, EditorAppExt, EditorMode};
use oddity_widgets::{
    ActivePalette, OddityFonts,
    field::{SearchText, search_field},
    fonts::paths,
    hover::{HoverFill, hover_fill_out, hover_fill_over},
    item_grid::{GridCell, GridEvent, GridItem, ItemGrid},
    tree::{TreeEvent, TreeItem, TreeView},
};

use super::{DimText, FIRST_TAB};
use crate::project::CurrentProject;

/// The FileSystem tab (folders) and the bottom panel's Files tab (the chosen
/// folder's contents), and the systems that keep them in step.
pub struct FileSystemPlugin;

impl Plugin for FileSystemPlugin {
    fn build(&self, app: &mut App) {
        if app.editor_mode() == EditorMode::Window {
            app.add_dock_tab(
                DockTab::new(DockSlot::FileSystem, "FileSystem", folders_page).order(FIRST_TAB),
            )
            // First in the bottom panel and open at start, so picking a
            // folder shows its contents.
            .add_dock_tab(DockTab::new(DockSlot::Bottom, "Files", files_page).order(FIRST_TAB));
        }
        app.init_resource::<FileSystemState>()
            .add_systems(Startup, scan_project)
            .add_systems(
                Update,
                (
                    (handle_tree_events, handle_grid_events),
                    (
                        refresh_tree.run_if(
                            resource_changed::<FileSystemState>
                                .or_else(any_match_filter::<Added<FolderTree>>),
                        ),
                        refresh_grid.run_if(
                            resource_changed::<FileSystemState>
                                .or_else(
                                    any_match_filter::<(With<FileFilter>, Changed<SearchText>)>,
                                )
                                .or_else(any_match_filter::<Added<FileGrid>>),
                        ),
                        refresh_breadcrumb,
                        show_browser,
                    ),
                )
                    .chain(),
            );
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

impl Entry {
    /// The entry at `path`, searching only the branch that can contain it.
    fn find(&self, path: &Path) -> Option<&Entry> {
        if self.path == path {
            return Some(self);
        }
        self.children
            .iter()
            .filter(|child| path.starts_with(&child.path))
            .find_map(|child| child.find(path))
    }

    /// Every entry under this one whose name matches, depth first.
    fn matching<'a>(&'a self, filter: &SearchText, out: &mut Vec<&'a Entry>) {
        for child in &self.children {
            if filter.matches(&child.name) {
                out.push(child);
            }
            child.matching(filter, out);
        }
    }
}

/// The file a FileSystem cell stands for: what was dragged, when a cell is
/// dropped somewhere.
pub(crate) fn file_of_cell(world: &World, cell: Entity) -> Option<PathBuf> {
    let cell = world.get::<GridCell>(cell)?;
    world.get::<FileGrid>(cell.grid)?;
    world
        .get_resource::<FileSystemState>()?
        .paths
        .get(&cell.key)
        .cloned()
}

/// What the page shows and how it is unfolded.
#[derive(Resource, Default)]
struct FileSystemState {
    root: Option<Entry>,
    /// Folders unfolded in the tree.
    expanded: HashSet<PathBuf>,
    /// The folder shown in the grid.
    current: Option<PathBuf>,
    /// The file or folder picked in the grid.
    selected: Option<PathBuf>,
    /// Row and cell keys back to paths, for every entry in the project.
    paths: HashMap<u64, PathBuf>,
}

impl FileSystemState {
    /// Show `folder` in the grid and unfold the tree down to it.
    fn open(&mut self, folder: PathBuf) {
        if let Some(root) = &self.root {
            for ancestor in folder.ancestors().skip(1) {
                if !ancestor.starts_with(&root.path) {
                    break;
                }
                self.expanded.insert(ancestor.to_path_buf());
            }
        }
        self.selected = None;
        self.current = Some(folder);
    }
}

/// The folder tree.
#[derive(Component, Default, Clone)]
struct FolderTree;

/// The grid of the current folder's contents.
#[derive(Component, Default, Clone)]
struct FileGrid;

#[derive(Component, Default, Clone)]
struct FileFilter;

/// The path above the grid. Remembers which folder it shows, so it is only
/// rebuilt when that changes.
#[derive(Component, Default, Clone)]
struct Breadcrumb {
    shown: Option<PathBuf>,
}

/// One folder in the breadcrumb; clicking it opens that folder.
#[derive(Component, Clone)]
struct Crumb(PathBuf);

/// A page's content, hidden while no project is open.
#[derive(Component, Default, Clone)]
struct Browser;

/// Shown instead of a page's content when no project is open.
#[derive(Component, Default, Clone)]
struct NoProjectNote;

/// The FileSystem tab of the left dock: the folder tree.
fn folders_page() -> impl Scene {
    bsn! {
        Node { flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Column }
        Children [
            Node {
                flex_grow: 1.0,
                min_height: px(0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::top(px(4)),
                overflow: Overflow::scroll_y(),
            }
            Browser
            ScrollArea
            TreeView
            FolderTree
            --
            @no_project_note()
        ]
    }
}

/// The Files tab of the bottom panel: breadcrumb and filter above, the grid
/// of the chosen folder's contents below.
fn files_page() -> impl Scene {
    bsn! {
        Node { flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Column }
        Children [
            Node {
                flex_grow: 1.0,
                min_height: px(0),
                flex_direction: FlexDirection::Column,
                padding: UiRect { left: px(6), right: px(6), top: px(6) },
            }
            Browser
            Children [
                Node {
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(8),
                    padding: UiRect::bottom(px(6)),
                }
                Children [
                    Node {
                        flex_grow: 1.0,
                        min_width: px(0),
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        overflow: Overflow::clip(),
                    }
                    Breadcrumb
                    --
                    @search_field("Filter Files") FileFilter Node { width: px(220) }
                ]
                --
                Node { flex_grow: 1.0, min_height: px(0), overflow: Overflow::scroll_y() }
                ScrollArea
                ItemGrid
                FileGrid
            ]
            --
            @no_project_note()
        ]
    }
}

fn no_project_note() -> impl Scene {
    bsn! {
        Text("No project open.\nRun: cargo editor -- path/to/game")
        NoProjectNote
        DimText
        Node { display: Display::None, margin: UiRect::all(px(16)) }
        TextLayout { justify: Justify::Center }
        TextFont {
            font: FontSourceTemplate::Handle(paths::UI),
            font_size: FontSize::Px(14.0),
        }
    }
}

fn scan_project(project: Res<CurrentProject>, mut state: ResMut<FileSystemState>) {
    let Some(project) = &project.0 else {
        return;
    };
    let root = scan(&project.root, "res://".to_owned(), &mut Vec::new());
    remember_keys(&root, &mut state.paths);
    state.expanded.insert(root.path.clone());
    state.current = Some(root.path.clone());
    state.root = Some(root);
}

/// Every entry's key, so clicks on rows and cells find their paths.
fn remember_keys(entry: &Entry, paths: &mut HashMap<u64, PathBuf>) {
    paths.insert(key_of(&entry.path), entry.path.clone());
    for child in &entry.children {
        remember_keys(child, paths);
    }
}

/// Read a folder and everything in it. `inside` holds the real paths of the
/// folders being read above this one, so a link back up (a symlink or a
/// Windows junction to a parent) is listed but not followed forever.
fn scan(path: &Path, name: String, inside: &mut Vec<PathBuf>) -> Entry {
    let folder = |children| Entry {
        path: path.to_path_buf(),
        name: name.clone(),
        children,
        is_dir: true,
    };
    let real = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if inside.contains(&real) {
        warn!(
            "{} leads back to a folder it is inside; not opening it",
            path.display()
        );
        return folder(Vec::new());
    }
    let items = match fs::read_dir(path) {
        Ok(items) => items,
        Err(error) => {
            warn!("Could not read {}: {error}", path.display());
            return folder(Vec::new());
        }
    };
    // Cargo's build output only at the project's top, not a folder that
    // happens to be called "target" deeper in.
    let at_top = inside.is_empty();
    inside.push(real);
    let mut children = Vec::new();
    for item in items {
        let item = match item {
            Ok(item) => item,
            Err(error) => {
                warn!("Could not read an entry of {}: {error}", path.display());
                continue;
            }
        };
        let child_name = item.file_name().to_string_lossy().into_owned();
        let child_path = item.path();
        let is_dir = child_path.is_dir();
        if child_name.starts_with('.') || (at_top && is_dir && child_name == "target") {
            continue;
        }
        children.push(if is_dir {
            scan(&child_path, child_name, inside)
        } else {
            Entry {
                path: child_path,
                name: child_name,
                children: Vec::new(),
                is_dir: false,
            }
        });
    }
    inside.pop();
    children.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
    folder(children)
}

/// The icon for a file's type, by extension: Godot's, except that scripts
/// get the @icons gear.
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
    if extension == "rs" {
        return Icon::at("cog");
    }
    Icon::godot(match extension.as_str() {
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

/// A grid cell for an entry. Until previews arrive, files show Godot's
/// generic big thumbnail with their type icon as a badge, as Godot does
/// for files it has no preview of.
fn grid_item(entry: &Entry, selected: bool) -> GridItem {
    let (icon, badge) = if entry.is_dir {
        (
            Icon::godot("FolderBigThumb").tinted(NodeColor::Folder),
            None,
        )
    } else {
        (Icon::godot("FileBigThumb"), Some(file_icon(entry)))
    };
    GridItem {
        key: key_of(&entry.path),
        label: entry.name.clone(),
        icon,
        badge,
        selected,
    }
}

fn key_of(path: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

fn show_browser(
    state: Res<FileSystemState>,
    mut browsers: Query<&mut Node, (With<Browser>, Without<NoProjectNote>)>,
    mut notes: Query<&mut Node, (With<NoProjectNote>, Without<Browser>)>,
) {
    let (browser, note) = if state.root.is_some() {
        (Display::Flex, Display::None)
    } else {
        (Display::None, Display::Flex)
    };
    for mut node in &mut browsers {
        if node.display != browser {
            node.display = browser;
        }
    }
    for mut node in &mut notes {
        if node.display != note {
            node.display = note;
        }
    }
}

fn refresh_tree(state: Res<FileSystemState>, mut trees: Query<&mut TreeView, With<FolderTree>>) {
    let Some(root) = state.root.as_ref() else {
        return;
    };
    let mut items = Vec::new();
    push_folder(root, 0, &state, &mut items);
    for mut tree in &mut trees {
        if tree.items != items {
            tree.items.clone_from(&items);
        }
    }
}

/// Add a folder row and, if it is unfolded, the folders inside it.
fn push_folder(entry: &Entry, depth: u16, state: &FileSystemState, items: &mut Vec<TreeItem>) {
    let expanded = state.expanded.contains(&entry.path);
    items.push(TreeItem {
        key: key_of(&entry.path),
        depth,
        label: entry.name.clone(),
        icon: Some(file_icon(entry)),
        has_children: entry.children.iter().any(|child| child.is_dir),
        expanded,
        selected: state.current.as_ref() == Some(&entry.path),
        dimmed: false,
        buttons: Vec::new(),
    });
    if expanded {
        for child in entry.children.iter().filter(|child| child.is_dir) {
            push_folder(child, depth + 1, state, items);
        }
    }
}

fn refresh_grid(
    state: Res<FileSystemState>,
    filters: Query<&SearchText, With<FileFilter>>,
    mut grids: Query<&mut ItemGrid, With<FileGrid>>,
) {
    let Some(root) = state.root.as_ref() else {
        return;
    };
    let filter = filters.iter().next().cloned().unwrap_or_default();
    let entries: Vec<&Entry> = if filter.is_empty() {
        state
            .current
            .as_deref()
            .and_then(|current| root.find(current))
            .map(|folder| folder.children.iter().collect())
            .unwrap_or_default()
    } else {
        let mut found = Vec::new();
        root.matching(&filter, &mut found);
        found
    };
    let items: Vec<GridItem> = entries
        .into_iter()
        .map(|entry| grid_item(entry, state.selected.as_ref() == Some(&entry.path)))
        .collect();
    for mut grid in &mut grids {
        if grid.items != items {
            grid.items.clone_from(&items);
        }
    }
}

/// Rebuild the breadcrumb when the current folder changes: `res://`, then
/// each folder down to the current one.
fn refresh_breadcrumb(
    mut commands: Commands,
    state: Res<FileSystemState>,
    palette: Res<ActivePalette>,
    fonts: Res<OddityFonts>,
    mut crumbs: Query<(Entity, &mut Breadcrumb)>,
) {
    let (Some(root), Some(current)) = (&state.root, &state.current) else {
        return;
    };
    for (bar, mut breadcrumb) in &mut crumbs {
        if breadcrumb.shown.as_ref() == Some(current) {
            continue;
        }
        breadcrumb.shown = Some(current.clone());
        commands.entity(bar).despawn_children();

        let mut folders: Vec<&Path> = current
            .ancestors()
            .take_while(|folder| folder.starts_with(&root.path))
            .collect();
        folders.reverse();
        for (index, folder) in folders.into_iter().enumerate() {
            if index > 0 {
                commands.spawn((
                    UiIcon::new(Icon::godot("GuiTreeArrowRight"), 12.0),
                    Node {
                        margin: UiRect::horizontal(Val::Px(2.0)),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    Pickable::IGNORE,
                    ChildOf(bar),
                ));
            }
            let label = if folder == root.path {
                root.name.clone()
            } else {
                folder
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
            };
            let last = folder == current.as_path();
            commands
                .spawn((
                    Node {
                        height: Val::Px(24.0),
                        flex_shrink: 0.0,
                        align_items: AlignItems::Center,
                        padding: UiRect::horizontal(Val::Px(6.0)),
                        border_radius: BorderRadius::all(Val::Px(3.0)),
                        ..default()
                    },
                    Crumb(folder.to_path_buf()),
                    HoverFill::default(),
                    ChildOf(bar),
                ))
                .observe(hover_fill_over)
                .observe(hover_fill_out)
                .observe(open_crumb)
                .with_child((
                    Text::new(label),
                    TextFont {
                        font: fonts.ui.clone().into(),
                        font_size: FontSize::Px(14.0),
                        ..default()
                    },
                    TextColor(if last {
                        palette.font
                    } else {
                        palette.font_secondary
                    }),
                    Pickable::IGNORE,
                ));
        }
    }
}

fn open_crumb(click: On<PointerClick>, crumbs: Query<&Crumb>, mut state: ResMut<FileSystemState>) {
    if click.button != PointerButton::Primary {
        return;
    }
    if let Ok(Crumb(folder)) = crumbs.get(click.event_target()) {
        state.open(folder.clone());
    }
}

fn handle_tree_events(
    mut events: MessageReader<TreeEvent>,
    trees: Query<(), With<FolderTree>>,
    mut state: ResMut<FileSystemState>,
) {
    for event in events.read() {
        match *event {
            TreeEvent::Select { tree, key, .. } if trees.contains(tree) => {
                if let Some(folder) = state.paths.get(&key).cloned() {
                    state.open(folder);
                }
            }
            // Double-clicking a folder unfolds it, as in Godot.
            TreeEvent::Toggle { tree, key } | TreeEvent::Activate { tree, key }
                if trees.contains(tree) =>
            {
                if let Some(path) = state.paths.get(&key).cloned()
                    && !state.expanded.remove(&path)
                {
                    state.expanded.insert(path);
                }
            }
            _ => {}
        }
    }
}

fn handle_grid_events(
    mut events: MessageReader<GridEvent>,
    grids: Query<(), With<FileGrid>>,
    mut state: ResMut<FileSystemState>,
    filters: Query<Entity, With<FileFilter>>,
    children: Query<&Children>,
    mut inputs: Query<&mut EditableText>,
) {
    for event in events.read() {
        match *event {
            GridEvent::Select { grid, key, .. } if grids.contains(grid) => {
                state.selected = state.paths.get(&key).cloned();
            }
            // Opening files (levels, prefabs, scripts) arrives in Phase 3.
            GridEvent::Activate { grid, key } if grids.contains(grid) => {
                if let Some(path) = state.paths.get(&key).cloned()
                    && path.is_dir()
                {
                    state.open(path);
                    // Opening a folder found by the filter shows that
                    // folder, as Unity does, not the search results still.
                    for filter in &filters {
                        for descendant in children.iter_descendants(filter) {
                            if let Ok(mut input) = inputs.get_mut(descendant) {
                                input.clear();
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> (tempfile::TempDir, Entry) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("oddity.toml"), "").unwrap();
        fs::write(dir.path().join(".gitignore"), "").unwrap();
        fs::create_dir_all(dir.path().join("src/enemies")).unwrap();
        fs::write(dir.path().join("src/main.rs"), "").unwrap();
        fs::write(dir.path().join("src/enemies/goblin.rs"), "").unwrap();
        fs::create_dir(dir.path().join("target")).unwrap();
        let root = scan(dir.path(), "res://".into(), &mut Vec::new());
        (dir, root)
    }

    #[test]
    fn folders_come_first_and_build_output_is_skipped() {
        let (_dir, root) = project();
        let names: Vec<&str> = root.children.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["src", "oddity.toml"]);
        assert_eq!(file_icon(&root.children[0].children[1]), Icon::at("cog"));
    }

    #[test]
    fn only_the_projects_own_target_folder_is_left_out() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("target")).unwrap();
        fs::create_dir_all(dir.path().join("assets/target")).unwrap();
        let root = scan(dir.path(), "res://".into(), &mut Vec::new());
        let names: Vec<&str> = root.children.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["assets"]);
        assert_eq!(root.children[0].children[0].name, "target");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_back_up_the_tree_is_listed_but_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("assets")).unwrap();
        std::os::unix::fs::symlink(dir.path(), dir.path().join("assets/loop")).unwrap();
        let root = scan(dir.path(), "res://".into(), &mut Vec::new());
        let looped = &root.children[0].children[0];
        assert_eq!(looped.name, "loop");
        assert!(looped.is_dir && looped.children.is_empty());
    }

    #[test]
    fn opening_a_folder_unfolds_the_tree_down_to_it() {
        let (dir, root) = project();
        let mut state = FileSystemState {
            root: Some(root),
            ..default()
        };
        let enemies = dir.path().join("src/enemies");
        state.open(enemies.clone());
        assert_eq!(state.current, Some(enemies));
        assert!(state.expanded.contains(dir.path()));
        assert!(state.expanded.contains(&dir.path().join("src")));
    }

    #[test]
    fn the_filter_searches_the_whole_project() {
        let (_dir, root) = project();
        let mut found = Vec::new();
        root.matching(&SearchText("goblin".into()), &mut found);
        let names: Vec<&str> = found.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["goblin.rs"]);
    }

    #[test]
    fn files_show_a_thumbnail_with_their_type_as_a_badge() {
        let (_dir, root) = project();
        let main = root.find(&root.path.join("src/main.rs")).unwrap();
        let item = grid_item(main, false);
        assert_eq!(item.icon, Icon::godot("FileBigThumb"));
        assert_eq!(item.badge, Some(Icon::at("cog")));
    }
}
