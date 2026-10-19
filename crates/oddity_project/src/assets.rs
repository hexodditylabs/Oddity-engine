//! Asset IDs and `.import` files (`roadmap/scripting.md`, "Stable IDs and
//! `.import` files").
//!
//! Every asset has an `.import` file next to it holding its ID and import
//! settings, so levels and prefabs refer to it by ID and moving or renaming
//! it breaks nothing:
//!
//! ```text
//! prefabs/bullet.prefab
//! prefabs/bullet.prefab.import    { "id": "a3b1c0de-…", "import": { … } }
//! ```
//!
//! Not `.meta`, Unity's name: Bevy reads `<asset>.meta` as its own loader
//! settings and fails the load if the file is anything else.
//!
//! The assets are every file in the project's folders, except `src/`,
//! `target/` and anything whose name starts with a dot. Files directly in
//! the project folder (`oddity.toml`, `Cargo.toml`, a README) aren't.
//! Paths here are relative to the project folder and use `/`.

use std::{
    collections::{BTreeMap, HashSet},
    fmt, fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::{check_relative, write_atomic};

/// The extension of an asset's import file, after the asset's own name.
pub const IMPORT_EXTENSION: &str = "import";

/// Folders directly in the project folder that hold code, not assets.
const NOT_ASSETS: [&str; 2] = ["src", "target"];

/// An asset's stable ID: a random UUID, written as `a3b1c0de-…`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AssetId(Uuid);

impl AssetId {
    /// A new, random ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// The UUID, for types that store it directly.
    pub fn uuid(self) -> Uuid {
        self.0
    }
}

impl Default for AssetId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for AssetId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl fmt::Display for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.hyphenated().fmt(f)
    }
}

impl fmt::Debug for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AssetId({self})")
    }
}

impl FromStr for AssetId {
    type Err = uuid::Error;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(text).map(Self)
    }
}

/// The contents of an `.import` file.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ImportFile {
    /// The asset's ID.
    pub id: AssetId,
    /// Import settings, as the Import dock edits them; their keys depend on
    /// the kind of asset.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub import: Map<String, Value>,
    /// Keys from a newer editor, kept as they are.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

impl ImportFile {
    /// A file for a new asset: a new ID, no settings.
    pub fn new() -> Self {
        Self::with_id(AssetId::new())
    }

    fn with_id(id: AssetId) -> Self {
        Self {
            id,
            import: Map::new(),
            other: Map::new(),
        }
    }
}

impl Default for ImportFile {
    fn default() -> Self {
        Self::new()
    }
}

/// The import file of the asset at `asset`: `asset` plus `.import`.
pub fn import_path(asset: &Path) -> PathBuf {
    let mut path = asset.as_os_str().to_owned();
    path.push(".");
    path.push(IMPORT_EXTENSION);
    PathBuf::from(path)
}

/// Something went wrong with an asset or its import file.
#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    /// Reading, writing or moving a file failed.
    #[error("{path}: {source}")]
    Io {
        /// The file or folder.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// An `.import` file that isn't valid.
    #[error("{path} is not a valid import file: {message}")]
    Parse {
        /// The file.
        path: PathBuf,
        /// What is wrong, with line and column.
        message: String,
    },
    /// A path that can't name an asset.
    #[error("{path:?} {why}")]
    InvalidPath {
        /// The path as given.
        path: String,
        /// Why not.
        why: String,
    },
    /// Nothing at that path.
    #[error("no file or folder at {0:?}")]
    NotFound(String),
    /// Something is already at that path.
    #[error("{0:?} already exists")]
    AlreadyExists(String),
}

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> AssetError + '_ {
    move |source| AssetError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// A project's assets and their IDs, both ways. Files created, renamed and
/// moved through it keep their IDs; the editor's FileSystem dock goes
/// through it.
#[derive(Debug)]
pub struct AssetIndex {
    root: PathBuf,
    paths: BTreeMap<AssetId, String>,
    ids: BTreeMap<String, AssetId>,
    /// Assets without an ID of their own: no import file, or a copy's,
    /// which holds another asset's ID.
    unidentified: Vec<String>,
    /// IDs given this session to assets that had lost their import file. A
    /// reference with the asset's old ID and its path can still claim it.
    fresh: HashSet<AssetId>,
    /// Import files that couldn't be read. Their assets are left alone, so
    /// their settings aren't overwritten.
    problems: Vec<AssetError>,
}

impl AssetIndex {
    /// Read every asset's import file under the project folder `root`.
    /// Writes nothing: assets without an ID are listed in
    /// [`unidentified`](Self::unidentified) until
    /// [`identify_all`](Self::identify_all).
    pub fn scan(root: impl AsRef<Path>) -> Result<Self, AssetError> {
        let mut index = Self {
            root: root.as_ref().to_path_buf(),
            paths: BTreeMap::new(),
            ids: BTreeMap::new(),
            unidentified: Vec::new(),
            fresh: HashSet::new(),
            problems: Vec::new(),
        };
        let mut assets = Vec::new();
        for entry in sorted_entries(&index.root)? {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = entry.file_type().map_err(io_error(&entry.path()))?.is_dir();
            if is_dir && !NOT_ASSETS.contains(&name.as_str()) && !name.starts_with('.') {
                collect_assets(&entry.path(), &name, &mut assets)?;
            }
        }
        for asset in assets {
            match read_import(&index.import_file(&asset)) {
                Ok(Some(file)) if !index.paths.contains_key(&file.id) => {
                    index.insert(asset, file.id);
                }
                // A copy made outside the editor, with its original's ID.
                Ok(Some(_)) | Ok(None) => index.unidentified.push(asset),
                Err(problem) => index.problems.push(problem),
            }
        }
        Ok(index)
    }

    /// Give every unidentified asset a new ID and write its import file,
    /// keeping any settings a copied one had. Returns the paths it wrote.
    pub fn identify_all(&mut self) -> Result<Vec<String>, AssetError> {
        let assets = std::mem::take(&mut self.unidentified);
        for (done, asset) in assets.iter().enumerate() {
            let import = self.import_file(asset);
            let lost = !import.exists();
            let mut file = match read_import(&import) {
                Ok(Some(copy)) => copy,
                _ => ImportFile::new(),
            };
            file.id = AssetId::new();
            if let Err(error) = write_import(&import, &file) {
                self.unidentified.extend_from_slice(&assets[done..]);
                return Err(error);
            }
            if lost {
                self.fresh.insert(file.id);
            }
            self.insert(asset.clone(), file.id);
        }
        Ok(assets)
    }

    /// Write a new asset at `path` with `contents`, and its import file
    /// with a new ID. Folders on the way are created.
    pub fn create(&mut self, path: &str, contents: &[u8]) -> Result<AssetId, AssetError> {
        check_asset_path(path)?;
        let full = self.root.join(path);
        if full.exists() {
            return Err(AssetError::AlreadyExists(path.to_owned()));
        }
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).map_err(io_error(parent))?;
        }
        write_atomic(&full, contents).map_err(io_error(&full))?;
        let file = ImportFile::new();
        write_import(&import_path(&full), &file)?;
        self.insert(path.to_owned(), file.id);
        Ok(file.id)
    }

    /// Move or rename the asset or folder at `from` to `to`, import files
    /// along, so every ID stays. Folders on the way are created.
    pub fn move_path(&mut self, from: &str, to: &str) -> Result<(), AssetError> {
        check_asset_path(from)?;
        check_asset_path(to)?;
        if from == to {
            return Ok(());
        }
        let (source, target) = (self.root.join(from), self.root.join(to));
        if fs::symlink_metadata(&source).is_err() {
            return Err(AssetError::NotFound(from.to_owned()));
        }
        // A change of case only: on Windows and macOS `to` already "exists"
        // because it is `from`.
        if target.exists() && !from.eq_ignore_ascii_case(to) {
            return Err(AssetError::AlreadyExists(to.to_owned()));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(io_error(parent))?;
        }
        fs::rename(&source, &target).map_err(io_error(&source))?;
        if source.is_dir() || target.is_dir() {
            // The import files inside moved with the folder.
            self.rename_under(&format!("{from}/"), &format!("{to}/"));
            return Ok(());
        }
        let (old_import, new_import) = (import_path(&source), import_path(&target));
        if old_import.exists()
            && let Err(source_error) = fs::rename(&old_import, &new_import)
        {
            // Put the asset back rather than leave it apart from its ID.
            let _ = fs::rename(&target, &source);
            return Err(io_error(&old_import)(source_error));
        }
        self.rename_exact(from, to);
        Ok(())
    }

    /// The asset `id` refers to, using `hint` (the path saved alongside the
    /// ID) when the ID is unknown: an asset at `hint` that lost its import
    /// file gets `id` back, so every other reference to it works again.
    /// `None` if neither finds it; the editor then offers to re-link.
    pub fn resolve(&mut self, id: AssetId, hint: &str) -> Result<Option<String>, AssetError> {
        if let Some(path) = self.paths.get(&id) {
            return Ok(Some(path.clone()));
        }
        let claimable = match self.ids.get(hint) {
            Some(current) => self.fresh.contains(current),
            None => self.unidentified.iter().any(|asset| asset == hint),
        };
        if !claimable {
            return Ok(None);
        }
        let import = self.import_file(hint);
        let mut file = match read_import(&import) {
            Ok(Some(file)) => file,
            _ => ImportFile::with_id(id),
        };
        file.id = id;
        write_import(&import, &file)?;
        if let Some(previous) = self.ids.remove(hint) {
            self.paths.remove(&previous);
            self.fresh.remove(&previous);
        }
        self.unidentified.retain(|asset| asset != hint);
        self.insert(hint.to_owned(), id);
        Ok(Some(hint.to_owned()))
    }

    /// The path of the asset `id`.
    pub fn path_of(&self, id: AssetId) -> Option<&str> {
        self.paths.get(&id).map(String::as_str)
    }

    /// The ID of the asset at `path`.
    pub fn id_of(&self, path: &str) -> Option<AssetId> {
        self.ids.get(path).copied()
    }

    /// Every identified asset, by path.
    pub fn iter(&self) -> impl Iterator<Item = (&str, AssetId)> {
        self.ids.iter().map(|(path, id)| (path.as_str(), *id))
    }

    /// Assets without an ID of their own yet.
    pub fn unidentified(&self) -> &[String] {
        &self.unidentified
    }

    /// Import files that couldn't be read.
    pub fn problems(&self) -> &[AssetError] {
        &self.problems
    }

    fn import_file(&self, asset: &str) -> PathBuf {
        import_path(&self.root.join(asset))
    }

    fn insert(&mut self, path: String, id: AssetId) {
        self.paths.insert(id, path.clone());
        self.ids.insert(path, id);
    }

    fn rename_exact(&mut self, from: &str, to: &str) {
        if let Some(id) = self.ids.remove(from) {
            self.insert(to.to_owned(), id);
        }
        for asset in &mut self.unidentified {
            if asset == from {
                *asset = to.to_owned();
            }
        }
    }

    fn rename_under(&mut self, from: &str, to: &str) {
        let moved: Vec<(String, AssetId)> = self
            .ids
            .iter()
            .filter(|(path, _)| path.starts_with(from))
            .map(|(path, id)| (path.clone(), *id))
            .collect();
        for (path, id) in moved {
            self.ids.remove(&path);
            self.insert(format!("{to}{}", &path[from.len()..]), id);
        }
        for asset in &mut self.unidentified {
            if let Some(rest) = asset.strip_prefix(from) {
                *asset = format!("{to}{rest}");
            }
        }
    }
}

/// A folder's entries, by name, so scans give the same result everywhere.
fn sorted_entries(dir: &Path) -> Result<Vec<fs::DirEntry>, AssetError> {
    let mut entries = fs::read_dir(dir)
        .map_err(io_error(dir))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io_error(dir))?;
    entries.sort_by_key(fs::DirEntry::file_name);
    Ok(entries)
}

/// Every asset in `dir` (at `relative` in the project), depth first.
/// Symbolic links and junctions aren't followed: they could loop.
fn collect_assets(dir: &Path, relative: &str, assets: &mut Vec<String>) -> Result<(), AssetError> {
    for entry in sorted_entries(dir)? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = format!("{relative}/{name}");
        let kind = entry.file_type().map_err(io_error(&entry.path()))?;
        if kind.is_dir() {
            collect_assets(&entry.path(), &path, assets)?;
        } else if kind.is_file() && !is_import_file(&name) {
            assets.push(path);
        }
    }
    Ok(())
}

fn is_import_file(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|extension| extension == IMPORT_EXTENSION)
}

/// `path` must be relative, use `/`, stay inside the project and be in an
/// asset folder.
fn check_asset_path(path: &str) -> Result<(), AssetError> {
    let invalid = |why: &str| AssetError::InvalidPath {
        path: path.to_owned(),
        why: why.to_owned(),
    };
    check_relative(path).map_err(|why| invalid(&why))?;
    let mut parts = path.split('/');
    let first = parts.next().unwrap_or_default();
    if parts.next().is_none() {
        return Err(invalid(
            "is in the project folder itself; assets live in folders",
        ));
    }
    if NOT_ASSETS.contains(&first) {
        return Err(invalid("is in a code folder, not an asset folder"));
    }
    if path
        .split('/')
        .any(|part| part.is_empty() || part.starts_with('.'))
    {
        return Err(invalid("has an empty or hidden (.) part"));
    }
    if is_import_file(path) {
        return Err(invalid("is an import file, not an asset"));
    }
    Ok(())
}

/// The import file at `path`; `None` if there is none.
fn read_import(path: &Path) -> Result<Option<ImportFile>, AssetError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(io_error(path)(source)),
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|error| AssetError::Parse {
            path: path.to_path_buf(),
            message: error.to_string(),
        })
}

fn write_import(path: &Path, file: &ImportFile) -> Result<(), AssetError> {
    let mut text = serde_json::to_string_pretty(file).map_err(|error| AssetError::Parse {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    text.push('\n');
    write_atomic(path, text.as_bytes()).map_err(io_error(path))
}
