//! Cargo's JSON messages (`--message-format=json-diagnostic-rendered-ansi`):
//! one JSON object per line on stdout.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How serious a compiler message is.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// Stops the build.
    Error,
    /// Worth fixing.
    Warning,
    /// Everything else (`note`, `help`, `failure-note`).
    Note,
}

/// Where a message points: the file, and the line and column, from 1.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Location {
    /// The file, absolute.
    pub file: PathBuf,
    /// The line, from 1.
    pub line: u32,
    /// The column, from 1.
    pub column: u32,
}

/// One compiler message.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Error, warning or note.
    pub level: Level,
    /// The one-line message: "cannot find value `x` in this scope".
    pub message: String,
    /// The code, such as `E0425`, if it has one.
    pub code: Option<String>,
    /// Where it points, if anywhere.
    pub location: Option<Location>,
    /// The full message as the compiler prints it, without colours.
    pub rendered: String,
}

/// What one line of cargo's output says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CargoMessage {
    /// A compiler message.
    Diagnostic(Diagnostic),
    /// A built program.
    Executable(PathBuf),
    /// The build ended.
    Finished {
        /// Whether it worked.
        success: bool,
    },
    /// Anything else (built libraries, build scripts).
    Other,
}

/// Read one line of cargo's JSON output. `None` for a line that isn't JSON.
pub fn parse_line(line: &str) -> Option<CargoMessage> {
    let value: Value = serde_json::from_str(line).ok()?;
    Some(match value["reason"].as_str() {
        Some("compiler-message") => match diagnostic(&value) {
            Some(diagnostic) => CargoMessage::Diagnostic(diagnostic),
            None => CargoMessage::Other,
        },
        Some("compiler-artifact") => match value["executable"].as_str() {
            Some(executable) => CargoMessage::Executable(executable.into()),
            None => CargoMessage::Other,
        },
        Some("build-finished") => CargoMessage::Finished {
            success: value["success"].as_bool().unwrap_or(false),
        },
        _ => CargoMessage::Other,
    })
}

fn diagnostic(value: &Value) -> Option<Diagnostic> {
    let message = &value["message"];
    let level = match message["level"].as_str()? {
        "error" | "error: internal compiler error" => Level::Error,
        "warning" => Level::Warning,
        _ => Level::Note,
    };
    // Paths are relative to the crate's workspace, or absolute for a crate
    // outside it (the game, seen from its runner).
    let package_dir = value["manifest_path"]
        .as_str()
        .and_then(|manifest| Path::new(manifest).parent());
    let location = message["spans"]
        .as_array()
        .and_then(|spans| {
            spans
                .iter()
                .find(|span| span["is_primary"].as_bool() == Some(true))
        })
        .and_then(|span| {
            let file = Path::new(span["file_name"].as_str()?);
            let file = match package_dir {
                Some(dir) if file.is_relative() => dir.join(file),
                _ => file.to_path_buf(),
            };
            Some(Location {
                file,
                line: u32::try_from(span["line_start"].as_u64()?).ok()?,
                column: u32::try_from(span["column_start"].as_u64()?).ok()?,
            })
        });
    Some(Diagnostic {
        level,
        message: message["message"].as_str()?.to_owned(),
        code: message["code"]["code"].as_str().map(str::to_owned),
        location,
        rendered: strip_ansi(message["rendered"].as_str().unwrap_or_default()),
    })
}

/// `text` without its terminal colour codes (`ESC [ ... m`).
pub fn strip_ansi(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            plain.push(c);
            continue;
        }
        if chars.next() == Some('[') {
            for code in chars.by_ref() {
                if code.is_ascii_alphabetic() {
                    break;
                }
            }
        }
    }
    plain
}
