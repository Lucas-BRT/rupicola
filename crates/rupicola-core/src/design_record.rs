use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

pub const DIRECTORY: &str = "design";

#[derive(Deserialize, Serialize)]
pub struct Project {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Brief,
    Direction,
    Tactic,
    Decision,
}

impl Kind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Brief => "Brief",
            Self::Direction => "Art direction",
            Self::Tactic => "Tactics",
            Self::Decision => "Decisions",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Proposed,
    Adopted,
    Rejected,
}

impl Status {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Proposed => "Proposed",
            Self::Adopted => "Adopted",
            Self::Rejected => "Rejected",
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct Entry {
    pub id: String,
    pub timestamp_ms: u64,
    pub kind: Kind,
    pub status: Status,
    pub title: String,
    pub note: String,
    pub surface: Option<String>,
    pub supersedes: Option<String>,
    pub reference: Option<String>,
    pub evidence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

fn read<T: serde::de::DeserializeOwned>(file: &Path) -> io::Result<T> {
    let metadata = fs::symlink_metadata(file)?;
    if !metadata.is_file() || metadata.len() > 128 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "design record must be a regular file of 128 KiB or less",
        ));
    }
    serde_json::from_slice(&fs::read(file)?).map_err(io::Error::other)
}

/// # Errors
///
/// Returns an error if the root is not a directory or its project record cannot be read or decoded.
pub fn project(root: &Path) -> io::Result<Project> {
    if !fs::symlink_metadata(root)?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "design project must be a directory",
        ));
    }
    read(&root.join("project.json"))
}

/// # Errors
///
/// Returns an error if directory traversal fails, a record cannot be read or decoded, or its ID does not match the file name.
pub fn entries(root: &Path) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.file_name() == Some(std::ffi::OsStr::new("project.json"))
            || path.extension() != Some(std::ffi::OsStr::new("json"))
        {
            continue;
        }
        let entry: Entry = read(&path)?;
        if path.file_stem().and_then(|id| id.to_str()) != Some(entry.id.as_str()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "design record ID does not match its file",
            ));
        }
        entries.push(entry);
    }
    entries.sort_by(|a, b| (b.timestamp_ms, &b.id).cmp(&(a.timestamp_ms, &a.id)));
    Ok(entries)
}

#[must_use]
pub fn superseded(entries: &[Entry]) -> std::collections::BTreeSet<&str> {
    entries
        .iter()
        .filter_map(|entry| entry.supersedes.as_deref())
        .collect()
}
